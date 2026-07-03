//! M0: the hand-written-blob harness (JIT.md §19). Machine-code blobs are
//! installed through the real primJITInstall path and executed inside the
//! full VM — entry, exit dispositions, leaf/allocating/exiting stubs, a
//! patched site, back-edge re-entry after preemption, and cross-tier calls
//! and returns through the flat loop — all without any Smalltalk-generated
//! code. This decouples all VM-side risk from all compiler-side risk.
//!
//! Every blob's method also carries the *equivalent bytecode*, so the same
//! program is interpretable (J4); tests assert native execution actually
//! happened via the jit counters.

#![cfg(target_arch = "x86_64")]

use smallishtalk::asm::Insn;
use smallishtalk::fixture::MethodBuilder;
use smallishtalk::treaty::*;
use smallishtalk::value::Value;
use smallishtalk::vm::{Vm, VmError};

// ---------------------------------------------------------------------------
// A minimal test-local AMD64 emitter. The real assembler is Smalltalk (M1);
// this exists only to write M0 fixture blobs legibly.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
struct R(u8);
const RAX: R = R(0);
const RCX: R = R(1);
const RDX: R = R(2);
const RBX: R = R(3); // FP
const RSI: R = R(6);
const RDI: R = R(7);
const R8: R = R(8);
const R9: R = R(9);
const R10: R = R(10);
const R11: R = R(11);
const R13: R = R(13); // LNK
const R14: R = R(14); // GBL

const CC_Z: u8 = 0x4;
const CC_NZ: u8 = 0x5;
const CC_LE: u8 = 0xE;

#[derive(Default)]
struct Asm {
    b: Vec<u8>,
    labels: Vec<Option<usize>>,
    fixups: Vec<(usize, usize)>, // (position of rel32, label)
}

impl Asm {
    fn new() -> Asm {
        Asm::default()
    }

    fn label(&mut self) -> usize {
        self.labels.push(None);
        self.labels.len() - 1
    }

    fn bind(&mut self, l: usize) {
        assert!(self.labels[l].is_none());
        self.labels[l] = Some(self.b.len());
    }

    fn here(&self) -> usize {
        self.b.len()
    }

    fn finish(mut self) -> Vec<u8> {
        for (pos, l) in std::mem::take(&mut self.fixups) {
            let target = self.labels[l].expect("unbound label");
            let rel = (target as i64 - (pos as i64 + 4)) as i32;
            self.b[pos..pos + 4].copy_from_slice(&rel.to_le_bytes());
        }
        self.b
    }

    fn rex(&mut self, w: u8, reg: u8, idx: u8, base: u8) {
        let v = 0x40 | w << 3 | (reg >> 3) << 2 | (idx >> 3) << 1 | (base >> 3);
        if v != 0x40 || w != 0 {
            self.b.push(v);
        }
    }

    /// mod=10 (disp32) memory operand [base + disp].
    fn mem(&mut self, reg: u8, base: R, disp: i32) {
        if base.0 & 7 == 4 {
            self.b.push(0x80 | (reg & 7) << 3 | 4);
            self.b.push(0x24); // SIB: no index, base=rsp/r12
        } else {
            self.b.push(0x80 | (reg & 7) << 3 | (base.0 & 7));
        }
        self.b.extend_from_slice(&disp.to_le_bytes());
    }

    /// mod=10 SIB operand [base + index<<scale + disp].
    fn mem_idx(&mut self, reg: u8, base: R, idx: R, scale: u8, disp: i32) {
        assert!(idx.0 & 7 != 4, "rsp cannot index");
        self.b.push(0x80 | (reg & 7) << 3 | 4);
        self.b.push(scale << 6 | (idx.0 & 7) << 3 | (base.0 & 7));
        self.b.extend_from_slice(&disp.to_le_bytes());
    }

    fn mov_rm(&mut self, dst: R, base: R, disp: i32) {
        self.rex(1, dst.0, 0, base.0);
        self.b.push(0x8B);
        self.mem(dst.0, base, disp);
    }

    fn mov_mr(&mut self, base: R, disp: i32, src: R) {
        self.rex(1, src.0, 0, base.0);
        self.b.push(0x89);
        self.mem(src.0, base, disp);
    }

    fn mov_mr_idx(&mut self, base: R, idx: R, scale: u8, disp: i32, src: R) {
        self.rex(1, src.0, idx.0, base.0);
        self.b.push(0x89);
        self.mem_idx(src.0, base, idx, scale, disp);
    }

    /// 32-bit load (zero-extends): mov dst32, [base + idx<<scale + disp].
    fn mov32_rm_idx(&mut self, dst: R, base: R, idx: R, scale: u8, disp: i32) {
        self.rex(0, dst.0, idx.0, base.0);
        self.b.push(0x8B);
        self.mem_idx(dst.0, base, idx, scale, disp);
    }

    fn mov_ri32(&mut self, r: R, imm: u32) {
        if r.0 >= 8 {
            self.b.push(0x41);
        }
        self.b.push(0xB8 + (r.0 & 7));
        self.b.extend_from_slice(&imm.to_le_bytes());
    }

    fn mov_mi32(&mut self, base: R, disp: i32, imm: i32) {
        self.rex(1, 0, 0, base.0);
        self.b.push(0xC7);
        self.mem(0, base, disp);
        self.b.extend_from_slice(&imm.to_le_bytes());
    }

    fn mov_rr(&mut self, dst: R, src: R) {
        self.rex(1, src.0, 0, dst.0);
        self.b.push(0x89);
        self.b.push(0xC0 | (src.0 & 7) << 3 | (dst.0 & 7));
    }

    fn add_rr(&mut self, dst: R, src: R) {
        self.rex(1, src.0, 0, dst.0);
        self.b.push(0x01);
        self.b.push(0xC0 | (src.0 & 7) << 3 | (dst.0 & 7));
    }

    fn add_ri8(&mut self, r: R, imm: i8) {
        self.rex(1, 0, 0, r.0);
        self.b.push(0x83);
        self.b.push(0xC0 | (r.0 & 7));
        self.b.push(imm as u8);
    }

    fn and_ri32(&mut self, r: R, imm: u32) {
        self.rex(1, 0, 0, r.0);
        self.b.push(0x81);
        self.b.push(0xE0 | (r.0 & 7));
        self.b.extend_from_slice(&imm.to_le_bytes());
    }

    fn cmp_ri32(&mut self, r: R, imm: i32) {
        self.rex(1, 0, 0, r.0);
        self.b.push(0x81);
        self.b.push(0xF8 | (r.0 & 7));
        self.b.extend_from_slice(&imm.to_le_bytes());
    }

    /// 32-bit compare; returns the offset of the imm32 (the Annex J.5
    /// patchable class-compare shape).
    fn cmp32_ri(&mut self, r: R, imm: u32) -> usize {
        if r.0 >= 8 {
            self.b.push(0x41);
        }
        self.b.push(0x81);
        self.b.push(0xF8 | (r.0 & 7));
        let off = self.b.len();
        self.b.extend_from_slice(&imm.to_le_bytes());
        off
    }

    fn test_rr(&mut self, a: R, b: R) {
        self.rex(1, b.0, 0, a.0);
        self.b.push(0x85);
        self.b.push(0xC0 | (b.0 & 7) << 3 | (a.0 & 7));
    }

    fn shift(&mut self, r: R, ext: u8, n: u8) {
        self.rex(1, 0, 0, r.0);
        self.b.push(0xC1);
        self.b.push(0xC0 | ext << 3 | (r.0 & 7));
        self.b.push(n);
    }

    fn sar_ri(&mut self, r: R, n: u8) {
        self.shift(r, 7, n);
    }

    fn shr_ri(&mut self, r: R, n: u8) {
        self.shift(r, 5, n);
    }

    fn shl_ri(&mut self, r: R, n: u8) {
        self.shift(r, 4, n);
    }

    fn lea(&mut self, dst: R, base: R, idx: R, scale: u8, disp: i32) {
        self.rex(1, dst.0, idx.0, base.0);
        self.b.push(0x8D);
        self.mem_idx(dst.0, base, idx, scale, disp);
    }

    /// cmp byte [base + disp], imm8
    fn cmp_m8_i8(&mut self, base: R, disp: i32, imm: u8) {
        self.rex(0, 0, 0, base.0);
        self.b.push(0x80);
        self.mem(7, base, disp);
        self.b.push(imm);
    }

    fn jcc(&mut self, cc: u8, l: usize) {
        self.b.push(0x0F);
        self.b.push(0x80 + cc);
        self.fixups.push((self.b.len(), l));
        self.b.extend_from_slice(&[0; 4]);
    }

    fn jmp(&mut self, l: usize) {
        self.b.push(0xE9);
        self.fixups.push((self.b.len(), l));
        self.b.extend_from_slice(&[0; 4]);
    }

    /// jmp rel32 whose field is a JIT_PATCH_TARGET site; initially aimed
    /// at `l`. Returns the offset of the rel32.
    fn jmp_patchable(&mut self, l: usize) -> usize {
        self.b.push(0xE9);
        let off = self.b.len();
        self.fixups.push((off, l));
        self.b.extend_from_slice(&[0; 4]);
        off
    }

    fn jmp_r(&mut self, r: R) {
        if r.0 >= 8 {
            self.b.push(0x41);
        }
        self.b.push(0xFF);
        self.b.push(0xE0 | (r.0 & 7));
    }

    fn jmp_m(&mut self, base: R, disp: i32) {
        self.rex(0, 0, 0, base.0);
        self.b.push(0xFF);
        self.mem(4, base, disp);
    }

    fn call_m(&mut self, base: R, disp: i32) {
        self.rex(0, 0, 0, base.0);
        self.b.push(0xFF);
        self.mem(2, base, disp);
    }
}

// ---------------------------------------------------------------------------
// Blob building blocks (the Annex-frozen conventions, hand-written)
// ---------------------------------------------------------------------------

/// FP := stackBody + frameOffset*8 (address of frame slot 0). Leaves the
/// untagged frame offset in RCX and the Process address in RAX.
fn emit_derive_fp(a: &mut Asm) {
    a.mov_rm(RAX, R14, GBL_ACTIVE_PROCESS as i32);
    a.mov_rm(RBX, RAX, (8 + 8 * PROCESS_STACK) as i32);
    a.mov_rm(RCX, RAX, (8 + 8 * PROCESS_FRAME_OFFSET) as i32);
    a.sar_ri(RCX, 1);
    a.lea(RBX, RBX, RCX, 3, 8);
}

/// Frame bytecode slot k, as a displacement off FP.
fn slot(k: usize) -> i32 {
    (8 * (FRAME_RECEIVER + k)) as i32
}

/// The return sequence (Annex J.5, return template): result (raw tagged
/// word) in RDX, FP valid. Delivers to the caller and jumps to its
/// compiled return point, or exits (interpreted / unlinked / no-site
/// caller), or halts (base frame).
fn emit_return_rdx(a: &mut Asm) {
    let not_base = a.label();
    let exit_path = a.label();

    a.mov_rm(RAX, RBX, (8 * FRAME_RETINFO) as i32); // tagged returnInfo
    a.sar_ri(RAX, 1);
    a.mov_rm(RCX, RBX, (8 * FRAME_CALLER) as i32);
    a.sar_ri(RCX, 1);
    a.test_rr(RCX, RCX);
    a.jcc(CC_NZ, not_base);
    // Base frame: park the result for the loop and halt.
    a.mov_mr(R14, GBL_HALT_VALUE as i32, RDX);
    a.mov_ri32(RAX, DISP_HALT as u32);
    a.jmp_m(R13, (8 * LNK_EXIT) as i32);

    a.bind(not_base);
    a.mov_rm(RSI, R14, GBL_ACTIVE_PROCESS as i32);
    a.mov_rm(RDI, RSI, (8 + 8 * PROCESS_STACK) as i32);
    a.lea(RDI, RDI, RCX, 3, 8); // caller FP
    // caller dest slot := result
    a.mov_rr(R8, RAX);
    a.shr_ri(R8, RETINFO_DEST_SHIFT as u8);
    a.and_ri32(R8, 0xFF);
    a.mov_mr_idx(RDI, R8, 3, slot(0), RDX);
    // Process.frameOffset := tagged(caller offset)
    a.lea(R9, RCX, RCX, 0, 1);
    a.mov_mr(RSI, (8 + 8 * PROCESS_FRAME_OFFSET) as i32, R9);
    // caller method -> vmState -> handle
    a.mov_rm(R10, RDI, (8 * FRAME_METHOD) as i32);
    a.mov_rm(R10, R10, (8 + 8 * METHOD_VMSTATE) as i32);
    a.sar_ri(R10, 1);
    a.shr_ri(R10, VMSTATE_HANDLE_SHIFT as u8);
    a.test_rr(R10, R10);
    a.jcc(CC_Z, exit_path);
    a.add_ri8(R10, -1);
    a.shl_ri(R10, 4); // * JIT_HANDLE_ENTRY_BYTES
    a.mov_rm(R11, R14, GBL_HANDLES as i32);
    a.add_rr(R10, R11);
    a.mov_rm(R11, R10, 0); // code base (0 = unlinked)
    a.test_rr(R11, R11);
    a.jcc(CC_Z, exit_path);
    a.mov_rm(R10, R10, 8); // returnPoints
    a.mov_rr(R9, RAX);
    a.and_ri32(R9, 0xFF); // site index
    a.cmp_ri32(R9, RETINFO_NO_SITE as i32);
    a.jcc(CC_Z, exit_path);
    a.mov32_rm_idx(R9, R10, R9, 2, 0); // retpts[site]
    a.mov_ri32(RAX, JIT_RETPOINT_NONE); // (rax re-read in exit_path)
    a.cmp_rr32(R9, RAX);
    a.jcc(CC_Z, exit_path);
    a.add_rr(R11, R9);
    a.jmp_r(R11);

    a.bind(exit_path);
    // Process.pc := tagged(resume pc from returnInfo) — RAX may have been
    // clobbered above, so re-read returnInfo from the *popped* frame; the
    // frame words are still intact (pops don't clear).
    a.mov_rm(RAX, RBX, (8 * FRAME_RETINFO) as i32);
    a.sar_ri(RAX, 1);
    a.shr_ri(RAX, RETINFO_PC_SHIFT as u8);
    a.lea(R11, RAX, RAX, 0, 1);
    a.mov_rm(RSI, R14, GBL_ACTIVE_PROCESS as i32);
    a.mov_mr(RSI, (8 + 8 * PROCESS_PC) as i32, R11);
    a.mov_ri32(RAX, DISP_EXIT as u32);
    a.jmp_m(R13, (8 * LNK_EXIT) as i32);
}

impl Asm {
    /// cmp r32, r32 (no REX.W) — for u32 sentinel compares.
    fn cmp_rr32(&mut self, a_: R, b_: R) {
        self.rex(0, b_.0, 0, a_.0);
        self.b.push(0x39);
        self.b.push(0xC0 | (b_.0 & 7) << 3 | (a_.0 & 7));
    }
}

/// Serialize the Annex J.4 maps layout.
fn maps(
    entry_off: u32,
    reentry: &[(u32, u32)],
    retpts: &[u32],
    patches: &[(u32, u8, u8)],
) -> Vec<u8> {
    let mut m = Vec::new();
    let w = |m: &mut Vec<u8>, v: u32| m.extend_from_slice(&v.to_le_bytes());
    w(&mut m, entry_off);
    w(&mut m, reentry.len() as u32);
    w(&mut m, retpts.len() as u32);
    w(&mut m, patches.len() as u32);
    for &(pc, off) in reentry {
        w(&mut m, pc);
        w(&mut m, off);
    }
    for &r in retpts {
        w(&mut m, r);
    }
    for &(off, kind, site) in patches {
        w(&mut m, off);
        w(&mut m, kind as u32 | (site as u32) << 8);
    }
    m
}

fn tagged(n: i64) -> i32 {
    i32::try_from(n * 2 + 1).unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Entry + halt: a native base-frame method returns a constant through the
/// full return sequence's halt path.
#[test]
fn native_method_runs_and_halts() {
    let mut vm = Vm::bare_test();
    vm.jit_init();
    let method = MethodBuilder::new(0, 4)
        .insns(vec![Insn::LoadInt { d: 1, imm: 42 }, Insn::Ret { a: 1 }])
        .build(&mut vm);

    let mut a = Asm::new();
    emit_derive_fp(&mut a);
    a.mov_ri32(RDX, tagged(42) as u32);
    emit_return_rdx(&mut a);
    let code = a.finish();

    vm.jit_install(method, &code, &maps(0, &[], &[], &[])).unwrap();
    let r = vm.call(method, Value::from_int(0), &[]).unwrap();
    assert_eq!(r.as_int(), 42);
    assert_eq!(vm.counters.jit_enters, 1, "must actually run native");
}

/// Differential: the same method runs interpreted when not installed.
#[test]
fn same_method_interpreted_matches() {
    let mut vm = Vm::bare_test();
    let method = MethodBuilder::new(0, 4)
        .insns(vec![Insn::LoadInt { d: 1, imm: 42 }, Insn::Ret { a: 1 }])
        .build(&mut vm);
    let r = vm.call(method, Value::from_int(0), &[]).unwrap();
    assert_eq!(r.as_int(), 42);
    assert_eq!(vm.counters.jit_enters, 0);
}

/// Interpreted caller sends to a compiled callee; the callee's return
/// sequence delivers into the interpreted frame and exits to the loop.
#[test]
fn interpreted_calls_compiled_and_is_returned_into() {
    let mut vm = Vm::bare_test();
    vm.jit_init();
    let callee = MethodBuilder::new(0, 4)
        .insns(vec![Insn::LoadInt { d: 1, imm: 99 }, Insn::Ret { a: 1 }])
        .build(&mut vm);
    let sel = vm.intern("m0Answer");
    let int_class = vm.class_table_at(CLASS_SMALLINTEGER);
    vm.install_method(int_class, sel, callee);

    let mut a = Asm::new();
    emit_derive_fp(&mut a);
    a.mov_ri32(RDX, tagged(99) as u32);
    emit_return_rdx(&mut a);
    vm.jit_install(callee, &a.finish(), &maps(0, &[], &[], &[]))
        .unwrap();

    let caller = MethodBuilder::new(0, 10)
        .insns(vec![
            Insn::LoadInt { d: 5, imm: 7 },
            Insn::Send { d: 1, r: 5, site: 0 },
            Insn::Ret { a: 1 },
        ])
        .site_named(&mut vm, "m0Answer", 0)
        .build(&mut vm);
    let r = vm.call(caller, Value::from_int(0), &[]).unwrap();
    assert_eq!(r.as_int(), 99);
    assert_eq!(vm.counters.jit_enters, 1);

    // Replacing the callee unlinks its code; the next call interprets the
    // replacement (JIT.md §12).
    let replacement = MethodBuilder::new(0, 4)
        .insns(vec![Insn::LoadInt { d: 1, imm: 77 }, Insn::Ret { a: 1 }])
        .build(&mut vm);
    vm.install_method(int_class, sel, replacement);
    assert_eq!(vm.counters.jit_unlinks, 1);
    let r = vm.call(caller, Value::from_int(0), &[]).unwrap();
    assert_eq!(r.as_int(), 77);
    assert_eq!(vm.counters.jit_enters, 1, "unlinked code never re-entered");
}

/// Compiled caller calls an interpreted callee through the CALL_INTERP
/// glue; the interpreter runs the callee; the return re-enters the
/// compiled caller at returnPoints[site] (the M0 cross-tier exit test).
#[test]
fn compiled_calls_interpreted_and_is_returned_into() {
    let mut vm = Vm::bare_test();
    vm.jit_init();

    // Interpreted callee on SmallInteger.
    let callee = MethodBuilder::new(0, 4)
        .insns(vec![Insn::LoadInt { d: 1, imm: 33 }, Insn::Ret { a: 1 }])
        .build(&mut vm);
    let sel = vm.intern("m0Interp");
    let int_class = vm.class_table_at(CLASS_SMALLINTEGER);
    vm.install_method(int_class, sel, callee);

    // Compiled caller: bytecode equivalent is LOADINT r6,7; SEND d1,r6,s0;
    // RET r1 — the blob emulates the send template with a pre-filled
    // inline cache (heap side), targeting CALL_INTERP.
    let caller = MethodBuilder::new(0, 12)
        .insns(vec![
            Insn::LoadInt { d: 6, imm: 7 },
            Insn::Send { d: 1, r: 6, site: 0 },
            Insn::Ret { a: 1 },
        ])
        .site_named(&mut vm, "m0Interp", 0)
        .build(&mut vm);
    // Pre-fill the heap send-site cache so the blob can read cacheMethod.
    let sites = vm.heap.slot(caller.as_ptr(), METHOD_SEND_SITES);
    vm.heap.set_slot_raw(
        sites.as_ptr(),
        SITE_CACHE_CLASS,
        Value::from_int(CLASS_SMALLINTEGER as i64),
    );
    vm.heap.set_slot_raw(sites.as_ptr(), SITE_CACHE_METHOD, callee);

    const RCV: usize = 6; // receiver bytecode slot
    let mut a = Asm::new();
    emit_derive_fp(&mut a); // RCX = my frame offset
    // receiver := 7 (tagged 15)
    a.mov_mi32(RBX, slot(RCV), tagged(7));
    // callee control words at my bytecode slots RCV-4..RCV-1, i.e. FP
    // displacements 8*RCV.. (callee offset = myOff + RCV):
    // callerFrameOffset := tagged(myOff)
    a.lea(R9, RCX, RCX, 0, 1);
    a.mov_mr(RBX, (8 * RCV) as i32, R9);
    // returnInfo := tagged(site 0 | dest 1 << 8 | resume pc 2 << 16)
    let ri: i64 = (1 << RETINFO_DEST_SHIFT) | (2 << RETINFO_PC_SHIFT);
    a.mov_mi32(RBX, (8 * RCV + 8) as i32, tagged(ri));
    // method := my sites[SITE_CACHE_METHOD]
    a.mov_rm(R10, RBX, (8 * FRAME_METHOD) as i32);
    a.mov_rm(R10, R10, (8 + 8 * METHOD_SEND_SITES) as i32);
    a.mov_rm(R10, R10, (8 + 8 * SITE_CACHE_METHOD) as i32);
    a.mov_mr(RBX, (8 * RCV + 16) as i32, R10);
    // serial++: flags := tagged(serial << 32)
    a.mov_rm(RSI, R14, GBL_ACTIVE_PROCESS as i32);
    a.mov_rm(R8, RSI, (8 + 8 * PROCESS_SERIAL_COUNTER) as i32);
    a.sar_ri(R8, 1);
    a.add_ri8(R8, 1);
    a.lea(R9, R8, R8, 0, 1);
    a.mov_mr(RSI, (8 + 8 * PROCESS_SERIAL_COUNTER) as i32, R9);
    a.mov_rr(R9, R8);
    a.shl_ri(R9, (SERIAL_SHIFT + 1) as u8);
    a.add_ri8(R9, 1);
    a.mov_mr(RBX, (8 * RCV + 24) as i32, R9);
    // Process.frameOffset := tagged(myOff + RCV)
    a.mov_rr(R9, RCX);
    a.add_ri8(R9, RCV as i8);
    a.lea(R9, R9, R9, 0, 1);
    a.mov_mr(RSI, (8 + 8 * PROCESS_FRAME_OFFSET) as i32, R9);
    // Into the interpreter (patched target would be here in M3).
    a.jmp_m(R13, (8 * LNK_CALL_INTERP) as i32);
    // --- send continuation (returnPoints[0]) ---
    let retpt = a.here() as u32;
    emit_derive_fp(&mut a);
    a.mov_rm(RDX, RBX, slot(1)); // result was delivered to my dest slot 1
    emit_return_rdx(&mut a);

    vm.jit_install(caller, &a.finish(), &maps(0, &[], &[retpt], &[]))
        .unwrap();

    let r = vm.call(caller, Value::from_int(0), &[]).unwrap();
    assert_eq!(r.as_int(), 33);
    // Two native episodes: entry..CALL_INTERP, then re-entry at the
    // return point through the do_return edge.
    assert_eq!(vm.counters.jit_enters, 2);
}

/// A native loop with a back-edge safepoint poll survives a forced
/// process switch and resumes *in native code* via the re-entry map
/// (JIT.md §9 — the no-OSR de-tiering trap).
#[test]
fn back_edge_reentry_after_preemption() {
    let mut vm = Vm::bare_test();
    vm.jit_init();

    const N: i64 = 5000;
    // Bytecode equivalent (also the interpretable fallback):
    //  0 LOADINT r1, 0      (sum)
    //  1 LOADINT r2, N      (i)
    //  2 LOADINT r3, 0
    //  3 GT r4, r2, r3      <- loop head, re-entry pc
    //  4 JUMPFALSE r4, +4   -> 9
    //  5 ADD r1, r1, r2
    //  6 LOADINT r5, 1
    //  7 SUB r2, r2, r5
    //  8 JUMP -6            -> 3
    //  9 RET r1
    let method = MethodBuilder::new(0, 8)
        .insns(vec![
            Insn::LoadInt { d: 1, imm: 0 },
            Insn::LoadInt { d: 2, imm: N as i16 },
            Insn::LoadInt { d: 3, imm: 0 },
            Insn::Gt { d: 4, a: 2, b: 3 },
            Insn::JumpFalse { a: 4, off: 4 },
            Insn::Add { d: 1, a: 1, b: 2 },
            Insn::LoadInt { d: 5, imm: 1 },
            Insn::Sub { d: 2, a: 2, b: 5 },
            Insn::Jump { off: -6 },
            Insn::Ret { a: 1 },
        ])
        .build(&mut vm);

    let mut a = Asm::new();
    let loop_head_l = a.label();
    let cont = a.label();
    let exit_loop = a.label();
    let exit_disp = a.label();
    emit_derive_fp(&mut a);
    a.mov_mi32(RBX, slot(1), tagged(0)); // sum
    a.mov_mi32(RBX, slot(2), tagged(N)); // i
    a.mov_mi32(RBX, slot(3), tagged(0));
    let loop_head = a.here() as u32;
    a.bind(loop_head_l);
    emit_derive_fp(&mut a); // re-entry point: FP must be re-derived
    // safepoint poll (back edge, bytecode pc 3)
    a.mov_rm(R8, R14, GBL_SAFEPOINT_PTR as i32);
    a.cmp_m8_i8(R8, 0, 0);
    a.jcc(CC_Z, cont);
    a.mov_rm(RDI, R14, GBL_VM as i32);
    a.mov_ri32(RSI, 3); // bytecode pc as template immediate
    a.call_m(R13, (8 * LNK_SAFEPOINT) as i32);
    a.test_rr(RAX, RAX);
    a.jcc(CC_NZ, exit_disp);
    a.jmp(loop_head_l); // re-derive FP after an exiting stub
    a.bind(cont);
    a.mov_rm(RDX, RBX, slot(2)); // i (tagged)
    a.cmp_ri32(RDX, 1); // tagged 0
    a.jcc(CC_LE, exit_loop);
    a.mov_rm(R9, RBX, slot(1));
    a.add_rr(R9, RDX);
    a.add_ri8(R9, -1); // tagged add
    a.mov_mr(RBX, slot(1), R9);
    a.add_ri8(RDX, -2); // i -= 1 (tagged)
    a.mov_mr(RBX, slot(2), RDX);
    a.jmp(loop_head_l);
    a.bind(exit_loop);
    a.mov_rm(RDX, RBX, slot(1));
    emit_return_rdx(&mut a);
    a.bind(exit_disp);
    a.jmp_m(R13, (8 * LNK_EXIT) as i32);

    vm.jit_install(method, &a.finish(), &maps(0, &[(3, loop_head)], &[], &[]))
        .unwrap();

    // A higher-priority process becomes runnable; arming the safepoint
    // forces the native loop's poll to preempt.
    let other = MethodBuilder::new(0, 4)
        .insns(vec![Insn::LoadInt { d: 1, imm: 1 }, Insn::Ret { a: 1 }])
        .build(&mut vm);
    let other_p = vm.spawn_process(other, Value::from_int(0), &[]).unwrap();
    vm.heap
        .set_slot_raw(other_p.as_ptr(), PROCESS_PRIORITY, Value::from_int(6));
    vm.make_runnable(other_p);
    vm.safepoint
        .armed
        .store(true, std::sync::atomic::Ordering::Relaxed);

    let main = vm
        .spawn_process(method, Value::from_int(0), &[])
        .unwrap();
    let r = vm.run(main).unwrap();
    assert_eq!(r.as_int(), N * (N + 1) / 2);
    assert!(
        vm.counters.jit_enters >= 2,
        "loop must resume natively after preemption (enters = {})",
        vm.counters.jit_enters
    );
    assert!(vm.counters.process_switches >= 1);
}

/// The patch routines rewrite the frozen site shapes: class immediate and
/// branch target (fill semantics per JIT.md §8, tested mechanically).
#[test]
fn patch_class_and_target() {
    let mut vm = Vm::bare_test();
    vm.jit_init();
    let method = MethodBuilder::new(0, 4)
        .insns(vec![Insn::LoadInt { d: 1, imm: 0 }, Insn::Ret { a: 1 }])
        .build(&mut vm);

    let mut a = Asm::new();
    let miss = a.label();
    emit_derive_fp(&mut a);
    a.mov_ri32(R11, 42); // pretend receiver class index
    let class_imm_off = a.cmp32_ri(R11, 0) as u32; // empty cache: never matches
    a.jcc(CC_NZ, miss);
    let target_rel_off = a.jmp_patchable(miss) as u32; // initially -> miss
    let hit = a.here() as u32;
    a.mov_ri32(RDX, tagged(1) as u32);
    emit_return_rdx(&mut a);
    a.bind(miss);
    a.mov_ri32(RDX, tagged(0) as u32);
    emit_return_rdx(&mut a);

    let handle = vm
        .jit_install(
            method,
            &a.finish(),
            &maps(
                0,
                &[],
                &[],
                &[
                    (class_imm_off, JIT_PATCH_CLASS, 0),
                    (target_rel_off, JIT_PATCH_TARGET, 0),
                ],
            ),
        )
        .unwrap();

    let r = vm.call(method, Value::from_int(0), &[]).unwrap();
    assert_eq!(r.as_int(), 0, "empty cache takes the miss path");

    // Fill: class matches, target -> hit.
    let (code_off, base) = {
        let jit = vm.jit.as_ref().unwrap();
        (jit.handles[handle].code_off, jit.cache.base_addr())
    };
    vm.jit_patch_class(handle, 0, 42);
    vm.jit_patch_target(handle, 1, (base + code_off + hit as usize) as u64);
    let r = vm.call(method, Value::from_int(0), &[]).unwrap();
    assert_eq!(r.as_int(), 1, "patched site takes the hit path");

    // flush_caches clears compiled class immediates back to the miss path.
    vm.flush_caches();
    let r = vm.call(method, Value::from_int(0), &[]).unwrap();
    assert_eq!(r.as_int(), 0, "invalidation walk cleared the compiled site");
}

/// ALLOC stub: allocation from native code, including a forced scavenge
/// under the call (J1/J2: the moving GC needs no cooperation from code).
#[test]
fn alloc_stub_with_gc_under_it() {
    let mut vm = Vm::bare_test();
    vm.jit_init();
    let method = MethodBuilder::new(0, 4)
        .insns(vec![Insn::LoadNil { d: 1 }, Insn::Ret { a: 1 }])
        .build(&mut vm);

    let mut a = Asm::new();
    let ok = a.label();
    emit_derive_fp(&mut a);
    a.mov_rm(RDI, R14, GBL_VM as i32);
    a.mov_ri32(RSI, CLASS_ARRAY);
    a.mov_ri32(RDX, FMT_PTRS as u32);
    a.mov_ri32(RCX, 2);
    a.call_m(R13, (8 * LNK_ALLOC) as i32);
    a.test_rr(RAX, RAX);
    a.jcc(CC_NZ, ok);
    a.mov_ri32(RAX, DISP_ERROR as u32);
    a.jmp_m(R13, (8 * LNK_EXIT) as i32);
    a.bind(ok);
    a.mov_rr(RDX, RAX);
    emit_derive_fp(&mut a); // allocating stub: re-derive FP (clobbers RAX/RCX)
    a.mov_mr(RBX, slot(1), RDX); // J2: value to a frame slot
    a.mov_rm(RDX, RBX, slot(1));
    emit_return_rdx(&mut a);
    let code = a.finish();
    vm.jit_install(method, &code, &maps(0, &[], &[], &[])).unwrap();

    // Plain run.
    let r = vm.call(method, Value::from_int(0), &[]).unwrap();
    assert!(r.is_ptr());
    assert_eq!(vm.heap.header(r.as_ptr()).class_index(), CLASS_ARRAY);
    assert_eq!(vm.heap.num_slots(r.as_ptr()), 2);

    // Now nearly fill young space so the ALLOC stub must scavenge while
    // native code is on the (OS) stack.
    let nil = vm.nil();
    while vm.heap.young_from.bytes_remaining() > 64 {
        let n = ((vm.heap.young_from.bytes_remaining() - 32) / 8).min(512);
        if vm.heap.alloc_ptrs(CLASS_ARRAY, n, nil).is_none() {
            break;
        }
    }
    let before = vm.scavenge_count;
    let r = vm.call(method, Value::from_int(0), &[]).unwrap();
    assert!(r.is_ptr());
    assert_eq!(vm.heap.header(r.as_ptr()).class_index(), CLASS_ARRAY);
    assert!(vm.scavenge_count > before, "the stub must have collected");
}

/// Leaf stub: BARRIER_REMEMBER marks an old object and pushes it on the
/// SSB exactly once.
#[test]
fn barrier_stub_is_a_leaf() {
    let mut vm = Vm::bare_test();
    vm.jit_init();
    let method = MethodBuilder::new(0, 4)
        .insns(vec![Insn::LoadNil { d: 1 }, Insn::Ret { a: 1 }])
        .build(&mut vm);
    // An old-space object to remember: the method itself.
    let target = method;

    let mut a = Asm::new();
    emit_derive_fp(&mut a);
    a.mov_rm(RDI, R14, GBL_VM as i32);
    a.mov_rm(RSI, RBX, (8 * FRAME_METHOD) as i32); // the method (old space)
    a.call_m(R13, (8 * LNK_BARRIER_REMEMBER) as i32);
    a.mov_rm(RDI, R14, GBL_VM as i32); // idempotence: call again
    a.mov_rm(RSI, RBX, (8 * FRAME_METHOD) as i32);
    a.call_m(R13, (8 * LNK_BARRIER_REMEMBER) as i32);
    a.mov_rm(RDX, R14, GBL_NIL as i32);
    emit_return_rdx(&mut a);
    let code = a.finish();
    vm.jit_install(method, &code, &maps(0, &[], &[], &[])).unwrap();

    let r = vm.call(method, Value::from_int(0), &[]).unwrap();
    assert_eq!(r, vm.nil());
    // The stub remembered the method exactly once, despite two calls
    // (the run loop independently remembers other old objects, e.g. the
    // scheduler — count only the method's entry).
    let times = vm.heap.ssb.iter().filter(|&&o| o == target.as_ptr()).count();
    assert_eq!(times, 1, "remembered exactly once");
    assert!(vm.heap.header(target.as_ptr()).gc_bits() & GC_BIT_REMEMBERED != 0);
}

/// An unimplemented (M3) stub reports DISP_ERROR and the error surfaces
/// as a VmError from the run loop.
#[test]
fn todo_stub_surfaces_error() {
    let mut vm = Vm::bare_test();
    vm.jit_init();
    let method = MethodBuilder::new(0, 4)
        .insns(vec![Insn::LoadNil { d: 1 }, Insn::Ret { a: 1 }])
        .build(&mut vm);
    let mut a = Asm::new();
    emit_derive_fp(&mut a);
    a.mov_rm(RDI, R14, GBL_VM as i32);
    a.call_m(R13, (8 * LNK_MUST_BE_BOOLEAN) as i32);
    a.jmp_m(R13, (8 * LNK_EXIT) as i32); // rax = disposition from the stub
    let code = a.finish();
    vm.jit_install(method, &code, &maps(0, &[], &[], &[])).unwrap();
    match vm.call(method, Value::from_int(0), &[]) {
        Err(VmError::Fatal(msg)) => assert!(msg.contains("unimplemented"), "{msg}"),
        other => panic!("expected fatal error, got {other:?}"),
    }
}

/// primJITInstall validates the maps structurally.
#[test]
fn install_validates_maps() {
    let mut vm = Vm::bare_test();
    vm.jit_init();
    let method = MethodBuilder::new(0, 4)
        .insns(vec![Insn::LoadNil { d: 1 }, Insn::Ret { a: 1 }])
        .build(&mut vm);
    let code = vec![0xC3u8; 8];
    // entry offset out of bounds
    assert!(vm.jit_install(method, &code, &maps(64, &[], &[], &[])).is_err());
    // truncated maps
    assert!(vm.jit_install(method, &code, &[1, 2, 3]).is_err());
    // unsorted re-entry map
    assert!(vm
        .jit_install(method, &code, &maps(0, &[(5, 0), (3, 0)], &[], &[]))
        .is_err());
    // bad patch kind
    assert!(vm
        .jit_install(method, &code, &maps(0, &[], &[], &[(0, 7, 0)]))
        .is_err());
}

/// Flush-all unlinks everything and resets the cache; the flushed method
/// keeps running (interpreted).
#[test]
fn flush_all_resets() {
    let mut vm = Vm::bare_test();
    vm.jit_init();
    let method = MethodBuilder::new(0, 4)
        .insns(vec![Insn::LoadInt { d: 1, imm: 5 }, Insn::Ret { a: 1 }])
        .build(&mut vm);
    let mut a = Asm::new();
    emit_derive_fp(&mut a);
    a.mov_ri32(RDX, tagged(5) as u32);
    emit_return_rdx(&mut a);
    let code = a.finish();
    vm.jit_install(method, &code, &maps(0, &[], &[], &[])).unwrap();
    assert_eq!(vm.call(method, Value::from_int(0), &[]).unwrap().as_int(), 5);
    assert_eq!(vm.counters.jit_enters, 1);

    let used_before = vm.jit.as_ref().unwrap().cache.used();
    vm.jit_flush_all().unwrap();
    assert!(vm.jit.as_ref().unwrap().cache.used() < used_before);
    assert_eq!(vm.vmstate_of(method) >> VMSTATE_HANDLE_SHIFT, 0);
    assert_eq!(vm.call(method, Value::from_int(0), &[]).unwrap().as_int(), 5);
    assert_eq!(vm.counters.jit_enters, 1, "flushed code never re-entered");
}

/// GC moves the compilation queue's methods and the handle table's method
/// roots correctly (queue survives a scavenge).
#[test]
fn queue_and_handles_are_gc_roots() {
    let mut vm = Vm::bare_test();
    vm.jit_init();
    vm.jit_control(JITCTL_SET_THRESHOLD, 2).unwrap();
    let method = MethodBuilder::new(0, 4)
        .insns(vec![Insn::LoadInt { d: 1, imm: 9 }, Insn::Ret { a: 1 }])
        .build(&mut vm);
    let sel = vm.intern("m0Bump");
    let int_class = vm.class_table_at(CLASS_SMALLINTEGER);
    vm.install_method(int_class, sel, method);

    let caller = MethodBuilder::new(0, 10)
        .insns(vec![
            Insn::LoadInt { d: 5, imm: 3 },
            Insn::Send { d: 1, r: 5, site: 0 },
            Insn::LoadInt { d: 5, imm: 3 },
            Insn::Send { d: 2, r: 5, site: 1 },
            Insn::Ret { a: 1 },
        ])
        .site_named(&mut vm, "m0Bump", 0)
        .site_named(&mut vm, "m0Bump", 1)
        .build(&mut vm);
    vm.call(caller, Value::from_int(0), &[]).unwrap();
    // Two activations -> threshold 2 tripped -> queued + signal.
    assert_eq!(vm.counters.jit_trips, 1);
    assert_eq!(vm.jit.as_ref().unwrap().queue.len(), 1);
    let qm = vm.jit.as_ref().unwrap().queue[0];
    assert_eq!(qm, method);
    // The jitSemaphore got an excess signal (no waiter yet).
    let sem = vm.specials()[SPECIAL_JIT_SEMAPHORE];
    assert_eq!(
        vm.heap.slot(sem.as_ptr(), SEMAPHORE_EXCESS_SIGNALS).as_int(),
        1
    );

    vm.collect_young().unwrap();
    vm.collect_old().unwrap();
    let qm = vm.jit.as_ref().unwrap().queue[0];
    // Old-space method may have moved in the compact; the queue entry
    // must still point at a CompiledMethod with the queued bit set.
    assert_eq!(
        vm.heap.header(qm.as_ptr()).class_index(),
        CLASS_COMPILEDMETHOD
    );
    assert!(vm.vmstate_of(qm) & (1 << VMSTATE_QUEUED_SHIFT) != 0);
    assert_eq!(vm.jit_next_request(), qm);
    assert_eq!(vm.jit_next_request(), vm.nil());
}
