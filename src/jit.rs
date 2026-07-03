//! The VM side of the JIT (JIT.md Parts II and V): code cache, handle
//! table, globals page, linkage table, transition trampoline and in-cache
//! glue, runtime stubs, patch routines, tiering trip, and the flat-loop
//! transitions (JIT Invariant J5: the interpreter loop is the sole
//! dispatcher; native execution is exactly one trampoline invocation deep).
//!
//! Nothing here selects or encodes an instruction the image is responsible
//! for: the compiler (Smalltalk, Part III) emits code and metadata; this
//! module installs it, patches immediate fields at Annex-frozen shapes, and
//! runs it. The only machine code owned by the VM is the fixed entry
//! trampoline and the two in-cache glue sequences (exit path, CALL_INTERP).
//!
//! AMD64 only for now; the ARM64 backend is milestone M7.

use crate::interp::Regs;
use crate::treaty::*;
use crate::value::Value;
use crate::vm::{fatal, Vm, VmError};

// ---------------------------------------------------------------------------
// The globals page (GBL register target; offsets are Treaty constants)
// ---------------------------------------------------------------------------

#[repr(C)]
pub struct GlobalsPage {
    /// *mut Vm, stored fresh at every trampoline entry (the Vm may move
    /// between entries; it cannot move during one).
    pub vm: u64,
    /// Address of the active Process object (synced at VM-time: entry,
    /// after GC, after process switch).
    pub active_process: u64,
    /// Pointer to the safepoint `armed` AtomicBool (stable Arc allocation).
    pub safepoint_ptr: u64,
    /// Young-space bounds for the inline write-barrier filter.
    pub young_base: u64,
    pub young_limit: u64,
    /// Native SP saved by the trampoline; the exit glue restores it.
    pub exit_sp: u64,
    /// Base-frame return value (raw tagged word), set by the return
    /// template's halt path just before DISP_HALT.
    pub halt_value: u64,
    /// nil/true/false object addresses (fixed for the VM process lifetime:
    /// the first three old-space objects, never moved by compaction).
    pub nil: u64,
    pub true_v: u64,
    pub false_v: u64,
    /// Flat runtime handle table (RuntimeHandleEntry array).
    pub handles: u64,
    /// Linkage table base — what LNK is loaded from at trampoline entry.
    pub lnk: u64,
    /// The class table (Value per class index) for the CLASSOF template.
    pub class_table: u64,
    pub _pad: [u64; 3],
}

const _: () = {
    assert!(std::mem::offset_of!(GlobalsPage, vm) == GBL_VM);
    assert!(std::mem::offset_of!(GlobalsPage, active_process) == GBL_ACTIVE_PROCESS);
    assert!(std::mem::offset_of!(GlobalsPage, safepoint_ptr) == GBL_SAFEPOINT_PTR);
    assert!(std::mem::offset_of!(GlobalsPage, young_base) == GBL_YOUNG_BASE);
    assert!(std::mem::offset_of!(GlobalsPage, young_limit) == GBL_YOUNG_LIMIT);
    assert!(std::mem::offset_of!(GlobalsPage, exit_sp) == GBL_EXIT_SP);
    assert!(std::mem::offset_of!(GlobalsPage, halt_value) == GBL_HALT_VALUE);
    assert!(std::mem::offset_of!(GlobalsPage, nil) == GBL_NIL);
    assert!(std::mem::offset_of!(GlobalsPage, true_v) == GBL_TRUE);
    assert!(std::mem::offset_of!(GlobalsPage, false_v) == GBL_FALSE);
    assert!(std::mem::offset_of!(GlobalsPage, handles) == GBL_HANDLES);
    assert!(std::mem::offset_of!(GlobalsPage, lnk) == GBL_LNK);
    assert!(std::mem::offset_of!(GlobalsPage, class_table) == GBL_CLASS_TABLE);
    assert!(std::mem::size_of::<GlobalsPage>() == GBL_SIZE);
};

/// One entry of the flat native-format handle table compiled code indexes
/// (JIT_HANDLE_ENTRY_BYTES). `code == 0` means unlinked.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RuntimeHandleEntry {
    pub code: u64,
    /// Pointer to the handle's returnPoints array (u32 native offsets,
    /// JIT_RETPOINT_NONE = no compiled return point for that site).
    pub retpts: u64,
}

const _: () = assert!(std::mem::size_of::<RuntimeHandleEntry>() == JIT_HANDLE_ENTRY_BYTES);

// ---------------------------------------------------------------------------
// Handles (JIT.md §5)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct PatchSite {
    pub off: u32,
    pub kind: u8,
    pub site: u8,
}

pub struct Handle {
    /// Keeps the method alive (GC root) and maps handle -> method.
    pub method: Value,
    pub code_off: usize,
    pub code_len: usize,
    pub entry_off: u32,
    /// Sorted (bytecode pc -> native offset) pairs: back-edge polls and
    /// any other sanctioned re-entry points (e.g. post-PRIM fallback).
    pub reentry: Box<[(u16, u32)]>,
    /// Dense per-send-site-index native offsets of send continuations.
    /// Boxed slice: stable storage, pointed at by the runtime entry.
    pub retpts: Box<[u32]>,
    pub patch_sites: Box<[PatchSite]>,
    pub live: bool,
}

// ---------------------------------------------------------------------------
// Code cache: one contiguous mmap reservation with guard pages (J7: code
// never moves, unlinked code is reclaimed only by flush-all)
// ---------------------------------------------------------------------------

pub struct CodeCache {
    map_base: *mut u8,
    map_len: usize,
    base: *mut u8,
    len: usize,
    used: usize,
}

// The cache is owned exclusively by the Vm (one OS thread runs Smalltalk).
unsafe impl Send for CodeCache {}

const PAGE: usize = 4096;

impl CodeCache {
    pub fn new(reserve: usize) -> CodeCache {
        let reserve = reserve.div_ceil(PAGE) * PAGE;
        let map_len = reserve + 2 * PAGE;
        let p = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                map_len,
                libc::PROT_NONE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        assert!(p != libc::MAP_FAILED, "code cache mmap failed");
        CodeCache {
            map_base: p as *mut u8,
            map_len,
            base: unsafe { (p as *mut u8).add(PAGE) },
            len: reserve,
            used: 0,
        }
    }

    #[inline]
    pub fn base_addr(&self) -> usize {
        self.base as usize
    }

    pub fn used(&self) -> usize {
        self.used
    }

    fn mprotect(&self, off: usize, len: usize, prot: libc::c_int) {
        let start = off / PAGE * PAGE;
        let end = (off + len).div_ceil(PAGE) * PAGE;
        let r = unsafe {
            libc::mprotect(self.base.add(start) as *mut libc::c_void, end - start, prot)
        };
        assert_eq!(r, 0, "mprotect failed");
    }

    /// Copy `bytes` into the cache (16-byte aligned) and make them
    /// executable. Returns the cache offset, or None when full.
    pub fn install(&mut self, bytes: &[u8]) -> Option<usize> {
        let off = self.used.div_ceil(16) * 16;
        if off + bytes.len() > self.len {
            return None;
        }
        self.mprotect(off, bytes.len(), libc::PROT_READ | libc::PROT_WRITE);
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.base.add(off), bytes.len());
        }
        self.mprotect(off, bytes.len(), libc::PROT_READ | libc::PROT_EXEC);
        self.used = off + bytes.len();
        Some(off)
    }

    /// Patch `bytes` at `off` (W^X flip around the write; J6: VM-time only,
    /// so no compiled instruction is in flight during the flip).
    pub fn patch(&mut self, off: usize, bytes: &[u8]) {
        assert!(off + bytes.len() <= self.used, "patch outside installed code");
        self.mprotect(off, bytes.len(), libc::PROT_READ | libc::PROT_WRITE);
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.base.add(off), bytes.len());
        }
        self.mprotect(off, bytes.len(), libc::PROT_READ | libc::PROT_EXEC);
    }

    /// Flush-all support: drop back to `keep` used bytes (the glue).
    pub fn reset_to(&mut self, keep: usize) {
        if self.used > keep {
            self.mprotect(keep, self.used - keep, libc::PROT_NONE);
        }
        self.used = keep;
    }
}

impl Drop for CodeCache {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.map_base as *mut libc::c_void, self.map_len);
        }
    }
}

// ---------------------------------------------------------------------------
// The entry trampoline (hand-written, fixed) — JIT.md §4/§7.
//
// extern "C" fn st_jit_enter(gbl: *mut GlobalsPage, entry: u64) -> u64
//
// Saves the C callee-saved registers, establishes GBL/LNK, records the
// native SP for the exit glue, and jumps into the cache. Compiled code
// runs with rsp 16-aligned (the sub rsp,8 below), so a `call [LNK+8n]`
// gives the Rust stub a conformant stack. The exit glue (installed in the
// cache) unwinds back through here: it restores rsp from GBL_EXIT_SP and
// pops what we pushed, returning the disposition left in rax.
// ---------------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".globl st_jit_enter",
    "st_jit_enter:",
    "push rbx",
    "push rbp",
    "push r12",
    "push r13",
    "push r14",
    "push r15",
    "sub rsp, 8",
    "mov r14, rdi",
    "mov r13, qword ptr [r14 + 88]", // GBL_LNK
    "mov qword ptr [r14 + 40], rsp", // GBL_EXIT_SP
    "jmp rsi",
);

unsafe extern "C" {
    fn st_jit_enter(gbl: *mut GlobalsPage, entry: u64) -> u64;
}

/// The exit glue, hand-encoded (position-independent), installed in the
/// cache so patched branches can target it (§7 branch-range rule):
///
/// ```text
/// mov rsp, [r14+GBL_EXIT_SP]   ; 49 8B 66 28
/// add rsp, 8                   ; 48 83 C4 08
/// pop r15; pop r14; pop r13    ; 41 5F 41 5E 41 5D
/// pop r12; pop rbp; pop rbx    ; 41 5C 5D 5B
/// ret                          ; C3
/// ```
const EXIT_GLUE: &[u8] = &[
    0x49, 0x8B, 0x66, GBL_EXIT_SP as u8,
    0x48, 0x83, 0xC4, 0x08,
    0x41, 0x5F,
    0x41, 0x5E,
    0x41, 0x5D,
    0x41, 0x5C,
    0x5D,
    0x5B,
    0xC3,
];

/// CALL_INTERP glue (M0 form, no lazy-upgrade check yet): the send
/// template has already written the callee's control words and bumped
/// Process.frameOffset; hand the frame to the interpreter by storing
/// bytecode pc 0 (tagged: 1) and exiting with DISP_EXIT.
///
/// ```text
/// mov rax, [r14+GBL_ACTIVE_PROCESS]        ; 49 8B 46 08
/// mov qword [rax+8+8*PROCESS_PC], 1        ; 48 C7 40 18 01 00 00 00
/// mov eax, DISP_EXIT                       ; B8 01 00 00 00
/// jmp qword [r13+8*LNK_EXIT]               ; 41 FF 65 50
/// ```
const CALL_INTERP_GLUE: &[u8] = &[
    0x49, 0x8B, 0x46, GBL_ACTIVE_PROCESS as u8,
    0x48, 0xC7, 0x40, (8 + 8 * PROCESS_PC) as u8, 0x01, 0x00, 0x00, 0x00,
    0xB8, DISP_EXIT as u8, 0x00, 0x00, 0x00,
    0x41, 0xFF, 0x65, (8 * LNK_EXIT) as u8,
];

// ---------------------------------------------------------------------------
// JitState
// ---------------------------------------------------------------------------

pub struct JitState {
    pub cache: CodeCache,
    pub globals: Box<GlobalsPage>,
    pub linkage: Box<[u64; LNK_COUNT]>,
    pub handles: Vec<Handle>,
    /// Flat native-format mirror of `handles` (indexed by compiled code).
    pub runtime: Vec<RuntimeHandleEntry>,
    /// Compilation request queue (a GC root; drained by the in-image JIT
    /// process via primJITNextRequest).
    pub queue: std::collections::VecDeque<Value>,
    pub threshold: i64,
    pub enabled: bool,
    pub profiling_level: i64,
    pub pending_error: Option<VmError>,
    /// True while the OS thread is below st_jit_enter (a stub is running).
    /// Guards flush-all and suppresses nested native entry (J5).
    pub in_native: bool,
    /// Cache offset where installs start (right after the glue).
    glue_end: usize,
    pub exit_glue_off: usize,
    pub call_interp_off: usize,
}

impl JitState {
    fn handle_of_vmstate(&self, vmstate: i64) -> Option<usize> {
        let h = (vmstate >> VMSTATE_HANDLE_SHIFT) & ((1 << VMSTATE_HANDLE_BITS) - 1);
        if h == 0 { None } else { Some(h as usize - 1) }
    }
}

impl Vm {
    // --- Initialization and global sync ---

    /// Create the JIT state (idempotent). Lazy so interpreter-only VMs and
    /// tests carry no mapping.
    pub fn jit_init(&mut self) {
        if self.jit.is_some() {
            return;
        }
        let mut cache = CodeCache::new(JIT_CACHE_BYTES_DEFAULT);
        let exit_off = cache.install(EXIT_GLUE).expect("glue fits");
        let ci_off = cache.install(CALL_INTERP_GLUE).expect("glue fits");
        let glue_end = cache.used();

        let mut linkage: Box<[u64; LNK_COUNT]> = Box::new([0; LNK_COUNT]);
        linkage[LNK_SEND_MISS] = stub_send_miss as *const () as u64;
        linkage[LNK_SEND_SLOW] = stub_send_slow as *const () as u64;
        linkage[LNK_ALLOC] = stub_alloc as *const () as u64;
        linkage[LNK_BARRIER_REMEMBER] = stub_barrier_remember as *const () as u64;
        linkage[LNK_STACK_GROW] = stub_stack_grow as *const () as u64;
        linkage[LNK_SAFEPOINT] = stub_safepoint as *const () as u64;
        linkage[LNK_NLR] = stub_nlr as *const () as u64;
        linkage[LNK_PRIM_CALL] = stub_prim_call as *const () as u64;
        linkage[LNK_MUST_BE_BOOLEAN] = stub_todo as *const () as u64;
        linkage[LNK_CALL_INTERP] = (cache.base_addr() + ci_off) as u64;
        linkage[LNK_EXIT] = (cache.base_addr() + exit_off) as u64;
        linkage[LNK_RESUME] = stub_resume_at as *const () as u64;

        let globals = Box::new(GlobalsPage {
            vm: 0,
            active_process: 0,
            safepoint_ptr: &self.safepoint.armed as *const _ as u64,
            young_base: 0,
            young_limit: 0,
            exit_sp: 0,
            halt_value: 0,
            nil: self.nil().raw(),
            true_v: self.true_v().raw(),
            false_v: self.false_v().raw(),
            handles: 0,
            lnk: linkage.as_ptr() as u64,
            class_table: 0,
            _pad: [0; 3],
        });

        self.jit = Some(Box::new(JitState {
            cache,
            globals,
            linkage,
            handles: Vec::new(),
            runtime: Vec::new(),
            queue: std::collections::VecDeque::new(),
            threshold: JIT_TIER_THRESHOLD_DEFAULT as i64,
            enabled: true,
            profiling_level: 0,
            pending_error: None,
            in_native: false,
            glue_end,
            exit_glue_off: exit_off,
            call_interp_off: ci_off,
        }));
        self.jit_sync_globals();
    }

    /// Refresh every heap-derived globals-page field. Called at VM-time
    /// whenever the referenced state can have changed: native entry, after
    /// every collection, after a process switch inside a stub.
    pub fn jit_sync_globals(&mut self) {
        let ap = self.active_process.raw();
        let yb = self.heap.young_from.base() as u64;
        let yl = self.heap.young_from.limit() as u64;
        let ct = self.class_table.as_ptr() as u64;
        let Some(jit) = self.jit.as_mut() else { return };
        jit.globals.active_process = ap;
        jit.globals.young_base = yb;
        jit.globals.young_limit = yl;
        jit.globals.handles = jit.runtime.as_ptr() as u64;
        jit.globals.lnk = jit.linkage.as_ptr() as u64;
        jit.globals.class_table = ct;
    }

    // --- vmState accessors (methods and blocks share the slot index) ---

    #[inline]
    pub fn vmstate_of(&self, method: Value) -> i64 {
        self.heap.slot(method.as_ptr(), METHOD_VMSTATE).as_int()
    }

    #[inline]
    pub fn set_vmstate(&mut self, method: Value, v: i64) {
        self.heap
            .set_slot_raw(method.as_ptr(), METHOD_VMSTATE, Value::from_int(v));
    }

    /// The live handle index for a method, if it has compiled code.
    pub fn jit_handle_of(&self, method: Value) -> Option<usize> {
        let jit = self.jit.as_ref()?;
        let idx = jit.handle_of_vmstate(self.vmstate_of(method))?;
        if jit.handles[idx].live { Some(idx) } else { None }
    }

    // --- Tiering: invocation counting and the trip (JIT.md §3) ---

    /// Bump the invocation counter; on crossing the threshold, queue the
    /// method for background compilation and signal jitSemaphore. Runs no
    /// image code (the trip is a ring-buffer append + semaphore signal).
    #[inline]
    pub fn bump_invocation(&mut self, method: Value) {
        let Some(jit) = self.jit.as_ref() else { return };
        if !jit.enabled {
            return;
        }
        let vs = self.vmstate_of(method);
        let counter = vs & ((1 << VMSTATE_COUNTER_BITS) - 1);
        if counter >= (1 << VMSTATE_COUNTER_BITS) - 1 {
            return; // saturated
        }
        let vs = vs + 1;
        self.set_vmstate(method, vs);
        let flags_clear = vs & ((1 << VMSTATE_QUEUED_SHIFT) | (1 << VMSTATE_COMPILED_SHIFT) | (1 << VMSTATE_DNC_SHIFT)) == 0;
        let jit = self.jit.as_ref().unwrap();
        // >= not ==: a request dropped on queue overflow must re-trip on
        // the next activation, or the method stays interpreted forever.
        if counter + 1 >= jit.threshold && flags_clear {
            self.jit_trip(method);
        }
    }

    fn jit_trip(&mut self, method: Value) {
        self.counters.jit_trips += 1;
        let jit = self.jit.as_mut().unwrap();
        if jit.queue.len() >= JIT_QUEUE_CAPACITY {
            // Full: drop the request; the method trips again later.
            self.counters.jit_queue_drops += 1;
            return;
        }
        jit.queue.push_back(method);
        let vs = self.vmstate_of(method) | (1 << VMSTATE_QUEUED_SHIFT);
        self.set_vmstate(method, vs);
        let sem = self.specials[SPECIAL_JIT_SEMAPHORE];
        if sem.is_ptr() && sem != self.nil() {
            self.semaphore_signal_internal(sem);
        }
    }

    /// primJITNextRequest: pop the oldest compilation request (nil when
    /// empty). Clears the queued bit only at install/give-up time, not
    /// here, so a method can't re-trip while being compiled.
    pub fn jit_next_request(&mut self) -> Value {
        let nil = self.nil();
        let Some(jit) = self.jit.as_mut() else { return nil };
        jit.queue.pop_front().unwrap_or(nil)
    }

    // --- Install (primJITInstall:code:maps:) ---

    /// Parse the maps ByteArray (Annex J.4 layout, little-endian u32s):
    /// ```text
    /// [0] entry_off  [1] n_reentry  [2] n_retpts  [3] n_patch
    /// then n_reentry * (pc:u32, off:u32)   -- sorted by pc
    /// then n_retpts * off:u32              -- JIT_RETPOINT_NONE = none
    /// then n_patch  * (off:u32, kind|site<<8:u32)
    /// ```
    pub fn jit_install(
        &mut self,
        method: Value,
        code: &[u8],
        maps: &[u8],
    ) -> Result<usize, String> {
        self.jit_init();

        // Parse and validate the maps against the code length.
        let rd = |i: usize| -> Result<u32, String> {
            maps.get(i * 4..i * 4 + 4)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
                .ok_or_else(|| "maps truncated".to_string())
        };
        let entry_off = rd(0)?;
        let n_reentry = rd(1)? as usize;
        let n_retpts = rd(2)? as usize;
        let n_patch = rd(3)? as usize;
        let expect_words = 4 + 2 * n_reentry + n_retpts + 2 * n_patch;
        if maps.len() != expect_words * 4 {
            return Err(format!(
                "maps length {} != expected {}",
                maps.len(),
                expect_words * 4
            ));
        }
        let code_len = code.len() as u32;
        let check_off = |o: u32| -> Result<(), String> {
            if o >= code_len {
                Err(format!("native offset {o} out of code bounds {code_len}"))
            } else {
                Ok(())
            }
        };
        check_off(entry_off)?;
        let mut reentry = Vec::with_capacity(n_reentry);
        let mut last_pc: i64 = -1;
        for i in 0..n_reentry {
            let pc = rd(4 + 2 * i)?;
            let off = rd(4 + 2 * i + 1)?;
            if pc as i64 <= last_pc {
                return Err("re-entry map not strictly sorted by pc".into());
            }
            if pc as usize >= MAX_METHOD_INSTRUCTIONS {
                return Err("re-entry pc out of range".into());
            }
            last_pc = pc as i64;
            check_off(off)?;
            reentry.push((pc as u16, off));
        }
        let rp_base = 4 + 2 * n_reentry;
        let mut retpts = Vec::with_capacity(n_retpts);
        for i in 0..n_retpts {
            let off = rd(rp_base + i)?;
            if off != JIT_RETPOINT_NONE {
                check_off(off)?;
            }
            retpts.push(off);
        }
        let ps_base = rp_base + n_retpts;
        let mut patch_sites = Vec::with_capacity(n_patch);
        for i in 0..n_patch {
            let off = rd(ps_base + 2 * i)?;
            let ks = rd(ps_base + 2 * i + 1)?;
            let kind = (ks & 0xFF) as u8;
            let site = ((ks >> 8) & 0xFF) as u8;
            if kind != JIT_PATCH_CLASS && kind != JIT_PATCH_TARGET {
                return Err(format!("unknown patch kind {kind}"));
            }
            // All patches rewrite a 4-byte immediate field.
            if off + 4 > code_len {
                return Err("patch site out of code bounds".into());
            }
            patch_sites.push(PatchSite { off, kind, site });
        }

        // Replacing existing code? Unlink the old handle first (J7:
        // unlink != free; the memory stays until flush-all).
        self.jit_unlink_method(method);

        let jit = self.jit.as_mut().unwrap();
        let Some(code_off) = jit.cache.install(code) else {
            return Err("code cache full".into());
        };
        let retpts: Box<[u32]> = retpts.into_boxed_slice();
        let handle_idx = jit.handles.len();
        if handle_idx + 1 >= (1 << VMSTATE_HANDLE_BITS) {
            return Err("handle table full".into());
        }
        jit.runtime.push(RuntimeHandleEntry {
            code: (jit.cache.base_addr() + code_off) as u64,
            retpts: retpts.as_ptr() as u64,
        });
        jit.handles.push(Handle {
            method,
            code_off,
            code_len: code.len(),
            entry_off,
            reentry: reentry.into_boxed_slice(),
            retpts,
            patch_sites: patch_sites.into_boxed_slice(),
            live: true,
        });

        // vmState: keep the counter, set compiled + handle, clear queued.
        let vs = self.vmstate_of(method);
        let counter = vs & ((1 << VMSTATE_COUNTER_BITS) - 1);
        let dnc = vs & (1 << VMSTATE_DNC_SHIFT);
        let new_vs = counter
            | dnc
            | (1 << VMSTATE_COMPILED_SHIFT)
            | (((handle_idx as i64) + 1) << VMSTATE_HANDLE_SHIFT);
        self.set_vmstate(method, new_vs);
        self.counters.jit_installs += 1;
        self.jit_sync_globals(); // runtime vec may have reallocated
        Ok(handle_idx)
    }

    /// Unlink a method's compiled code, if any (method install/replace,
    /// class install). The code is never re-entered — vmState is cleared
    /// and the runtime entry zeroed — but its memory stays intact (J7),
    /// so activations currently below it in native execution are
    /// unaffected (there are none at VM-time deeper than one trampoline,
    /// and return-into is routed to the interpreter by the cleared state).
    pub fn jit_unlink_method(&mut self, method: Value) {
        let Some(jit) = self.jit.as_mut() else { return };
        let vs = self.heap.slot(method.as_ptr(), METHOD_VMSTATE);
        if !vs.is_int() {
            return;
        }
        let Some(idx) = jit.handle_of_vmstate(vs.as_int()) else {
            return;
        };
        jit.handles[idx].live = false;
        jit.runtime[idx] = RuntimeHandleEntry { code: 0, retpts: 0 };
        self.counters.jit_unlinks += 1;
        // Clear compiled + handle bits; keep counter and dnc.
        let v = vs.as_int();
        let counter = v & ((1 << VMSTATE_COUNTER_BITS) - 1);
        let dnc = v & (1 << VMSTATE_DNC_SHIFT);
        self.set_vmstate(method, counter | dnc);
    }

    /// Flush-all (JIT.md §12): unlink every handle, reset the cache to the
    /// glue, clear the queue. Requires that no native activation exists —
    /// guaranteed when not called from below the trampoline (J5); refused
    /// (Err) otherwise so the image retries from a do-not-compile method.
    pub fn jit_flush_all(&mut self) -> Result<(), ()> {
        let Some(jit) = self.jit.as_mut() else {
            return Ok(());
        };
        if jit.in_native {
            return Err(());
        }
        let methods: Vec<Value> = jit.handles.iter().map(|h| h.method).collect();
        let glue_end = jit.glue_end;
        jit.handles.clear();
        jit.runtime.clear();
        jit.queue.clear();
        jit.cache.reset_to(glue_end);
        for m in methods {
            // Clear compiled/queued/handle bits; keep counter + dnc.
            let vs = self.heap.slot(m.as_ptr(), METHOD_VMSTATE);
            if vs.is_int() {
                let v = vs.as_int();
                let counter = v & ((1 << VMSTATE_COUNTER_BITS) - 1);
                let dnc = v & (1 << VMSTATE_DNC_SHIFT);
                self.set_vmstate(m, counter | dnc);
            }
        }
        self.jit_sync_globals();
        Ok(())
    }

    // --- Patch routines (J6: mechanical byte-writers at Annex shapes) ---

    /// Rewrite the 4-byte class-index immediate at a JIT_PATCH_CLASS site.
    pub fn jit_patch_class(&mut self, handle: usize, patch_idx: usize, class_index: u32) {
        let jit = self.jit.as_mut().unwrap();
        let h = &jit.handles[handle];
        let ps = h.patch_sites[patch_idx];
        assert_eq!(ps.kind, JIT_PATCH_CLASS);
        let off = h.code_off + ps.off as usize;
        jit.cache.patch(off, &class_index.to_le_bytes());
    }

    /// Rewrite the rel32 at a JIT_PATCH_TARGET site to reach an absolute
    /// target (which must lie inside the cache — §7 branch-range rule).
    pub fn jit_patch_target(&mut self, handle: usize, patch_idx: usize, target: u64) {
        let jit = self.jit.as_mut().unwrap();
        let h = &jit.handles[handle];
        let ps = h.patch_sites[patch_idx];
        assert_eq!(ps.kind, JIT_PATCH_TARGET);
        let base = jit.cache.base_addr() as u64;
        assert!(
            target >= base && target < base + jit.cache.used() as u64,
            "patched branch target outside the code cache"
        );
        let off = h.code_off + ps.off as usize;
        let site_addr = base + off as u64;
        let rel = (target as i64 - (site_addr as i64 + 4)) as i32;
        jit.cache.patch(off, &rel.to_le_bytes());
    }

    /// Clear every compiled inline-cache site back to the miss path —
    /// the compiled arm of §8's eager invalidation walk. (Class immediate
    /// 0 never matches a real class index, so the site takes SEND_MISS.)
    pub fn jit_clear_compiled_sites(&mut self) {
        let Some(jit) = self.jit.as_ref() else { return };
        let mut work: Vec<(usize, usize)> = Vec::new();
        for (hi, h) in jit.handles.iter().enumerate() {
            if !h.live {
                continue;
            }
            for (pi, ps) in h.patch_sites.iter().enumerate() {
                if ps.kind == JIT_PATCH_CLASS {
                    work.push((hi, pi));
                }
            }
        }
        for (hi, pi) in work {
            self.jit_patch_class(hi, pi, 0);
        }
    }

    // --- Flat-loop transitions (J5) ---

    /// Native entry/re-entry address for the current frame, if any:
    /// pc 0 -> method entry; otherwise the re-entry map. Returns an
    /// absolute address.
    pub fn native_resume_addr(&self, regs: &Regs) -> Option<u64> {
        let jit = self.jit.as_ref()?;
        if jit.in_native {
            return None; // no nested native entry (J5)
        }
        if self.frame_is_unwind_cont(regs) {
            return None; // unwind continuations run interpreted
        }
        let idx = self.jit_handle_of(regs.method)?;
        let h = &jit.handles[idx];
        let off = if regs.pc == 0 {
            h.entry_off
        } else {
            let pc = u16::try_from(regs.pc).ok()?;
            let i = h.reentry.binary_search_by_key(&pc, |&(p, _)| p).ok()?;
            h.reentry[i].1
        };
        Some((jit.cache.base_addr() + h.code_off + off as usize) as u64)
    }

    /// Unguarded resume lookup for use *inside* exiting stubs (SEND_MISS,
    /// PRIM_CALL, NLR): the returned address is jumped to by the stub's
    /// cold call site — a native-to-native jump within the same trampoline
    /// invocation, so the in_native guard does not apply.
    fn stub_resume_addr(&self, regs: &Regs) -> Option<u64> {
        if self.frame_is_unwind_cont(regs) {
            return None;
        }
        let jit = self.jit.as_ref()?;
        let idx = {
            let i = jit.handle_of_vmstate(self.vmstate_of(regs.method))?;
            if !jit.handles[i].live {
                return None;
            }
            i
        };
        let h = &jit.handles[idx];
        let off = if regs.pc == 0 {
            h.entry_off
        } else {
            let pc = u16::try_from(regs.pc).ok()?;
            let i = h.reentry.binary_search_by_key(&pc, |&(p, _)| p).ok()?;
            h.reentry[i].1
        };
        Some((jit.cache.base_addr() + h.code_off + off as usize) as u64)
    }

    /// A frame pushed by the unwinder to run an ensure block: its return
    /// resumes the pending unwind, which only the interpreter's do_return
    /// implements — such frames never enter native code (J4).
    fn frame_is_unwind_cont(&self, regs: &Regs) -> bool {
        let flags = self.heap.slot(regs.stack, regs.frame + FRAME_FLAGS);
        flags.is_int() && flags.as_int() & (FLAG_UNWINDCONT as i64) != 0
    }

    /// Where an exiting stub's cold call site should go next: DISP_HALT
    /// (halt value parked), a native address to jump to, or DISP_EXIT.
    /// The cold sequence is `cmp rax, DISP_ERROR; ja -> jmp rax; jmp exit`.
    fn stub_continuation(&mut self, regs: &mut Regs) -> u64 {
        if let Some(v) = regs.halted.take() {
            self.jit.as_mut().unwrap().globals.halt_value = v.raw();
            // Leave halted state discoverable by the loop: the trampoline
            // returns DISP_HALT through the exit glue.
            return DISP_HALT;
        }
        self.save_regs(regs);
        match self.stub_resume_addr(regs) {
            Some(addr) => addr,
            None => DISP_EXIT,
        }
    }

    /// §8's fill routine, compiled arm: when the interpreter (or a
    /// SEND_MISS underneath compiled code) refills a heap send-site
    /// cache, the caller's compiled inline cache is patched to match —
    /// PATCH_CLASS := the receiver's class index, PATCH_TARGET := the
    /// callee's native entry (or the CALL_INTERP glue). Heap entry and
    /// compiled site change together, at VM-time (J6).
    pub fn jit_patch_send_site(
        &mut self,
        caller_method: Value,
        site: u8,
        class_index: u32,
        callee_method: Value,
    ) {
        let Some(handle) = self.jit_handle_of(caller_method) else {
            return;
        };
        // Primitives that cannot run in the framed convention (perform's
        // re-dispatch, snapshot's dual result) are never patched into:
        // their sites stay on the miss path, where SEND_MISS runs them
        // frameless through the ordinary activation.
        if let Some(n) = self.method_primitive(callee_method) {
            if n == PRIM_PERFORM_WITH_ARGS || n == PRIM_SNAPSHOT {
                return;
            }
        }
        let callee_entry = match self.jit_handle_of(callee_method) {
            Some(ch) => {
                self.counters.jit_sites_direct += 1;
                let jit = self.jit.as_ref().unwrap();
                let h = &jit.handles[ch];
                (jit.cache.base_addr() + h.code_off + h.entry_off as usize) as u64
            }
            None => {
                self.counters.jit_sites_interp += 1;
                let jit = self.jit.as_ref().unwrap();
                (jit.cache.base_addr() + jit.call_interp_off) as u64
            }
        };
        let sites: Vec<(usize, u8)> = {
            let jit = self.jit.as_ref().unwrap();
            jit.handles[handle]
                .patch_sites
                .iter()
                .enumerate()
                .filter(|(_, ps)| ps.site == site)
                .map(|(i, ps)| (i, ps.kind))
                .collect()
        };
        for (i, kind) in sites {
            if kind == JIT_PATCH_CLASS {
                self.jit_patch_class(handle, i, class_index);
            } else {
                self.jit_patch_target(handle, i, callee_entry);
            }
        }
    }

    /// The compiled return point for returning into `method`'s frame at
    /// send site `site` (used by the interpreter's return path).
    pub fn native_return_addr(&self, method: Value, site: u8) -> Option<u64> {
        let jit = self.jit.as_ref()?;
        if jit.in_native || site as u64 == RETINFO_NO_SITE {
            return None;
        }
        let idx = self.jit_handle_of(method)?;
        let h = &jit.handles[idx];
        let off = *h.retpts.get(site as usize)?;
        if off == JIT_RETPOINT_NONE {
            return None;
        }
        Some((jit.cache.base_addr() + h.code_off + off as usize) as u64)
    }

    /// Run native code starting at `entry` until it has nothing left to do
    /// without the interpreter (JIT.md §4). On return, `regs` reflect the
    /// (possibly different) current process; `regs.halted` is set when a
    /// base frame returned.
    pub fn enter_native(&mut self, regs: &mut Regs, mut entry: u64) -> Result<(), VmError> {
        loop {
            // The calling convention requires Process.{frameOffset,pc}
            // current at entry (§4 resume contract holds them current at
            // every loop <-> native boundary).
            //
            // Progress rule: an exit that leaves the process exactly where
            // it entered — same process, frame, and pc — must hand control
            // to the interpreter (that is what an exit-to-interpreter
            // template asks for); resuming natively would re-run the same
            // template forever. Any other exit (switch, handoff, return)
            // may chain straight into the next native episode.
            let before = (self.active_process.raw(), regs.frame, regs.pc);
            self.save_regs(regs);
            // Globals are synced at every mutation point (GC, switch,
            // install, class registration) — no per-entry sync needed.
            self.counters.jit_enters += 1;
            let vm_ptr = self as *mut Vm as u64;
            let jit = self.jit.as_mut().unwrap();
            jit.globals.vm = vm_ptr;
            jit.in_native = true;
            let gbl: *mut GlobalsPage = &mut *jit.globals;
            let disp = unsafe { st_jit_enter(gbl, entry) };
            let jit = self.jit.as_mut().unwrap();
            jit.in_native = false;
            self.counters.jit_exits += 1;
            match disp {
                DISP_HALT => {
                    let raw = self.jit.as_ref().unwrap().globals.halt_value;
                    regs.halted = Some(Value::from_raw(raw));
                    return Ok(());
                }
                DISP_ERROR => {
                    let e = self
                        .jit
                        .as_mut()
                        .unwrap()
                        .pending_error
                        .take()
                        .unwrap_or(VmError::Fatal("JIT error with no pending cause".into()));
                    return Err(e);
                }
                DISP_EXIT => {
                    *regs = self.load_regs();
                    if (self.active_process.raw(), regs.frame, regs.pc) == before {
                        return Ok(());
                    }
                    if let Some(addr) = self.native_resume_addr(regs) {
                        entry = addr;
                        continue;
                    }
                    return Ok(());
                }
                other => return fatal(format!("bad JIT disposition {other}")),
            }
        }
    }

    // --- primJITControl ---

    /// Err carries a primitive failure code (crate::prims::FAIL_*), so the
    /// image can handle refusals (e.g. flush-all from compiled code)
    /// through the ordinary fallback protocol.
    pub fn jit_control(&mut self, op: i64, arg: i64) -> Result<i64, i64> {
        self.jit_init();
        match op {
            JITCTL_GET_THRESHOLD => Ok(self.jit.as_ref().unwrap().threshold),
            JITCTL_SET_THRESHOLD => {
                let jit = self.jit.as_mut().unwrap();
                jit.threshold = arg.max(1);
                Ok(jit.threshold)
            }
            JITCTL_GET_ENABLED => Ok(self.jit.as_ref().unwrap().enabled as i64),
            JITCTL_SET_ENABLED => {
                let jit = self.jit.as_mut().unwrap();
                jit.enabled = arg != 0;
                Ok(jit.enabled as i64)
            }
            JITCTL_FLUSH_ALL => {
                self.jit_flush_all().map_err(|()| crate::prims::FAIL_UNSUPPORTED_CONTEXT)?;
                Ok(0)
            }
            JITCTL_GET_PROFILING_LEVEL => Ok(self.jit.as_ref().unwrap().profiling_level),
            JITCTL_SET_PROFILING_LEVEL => {
                // Level change = flush-all + organic recompile (JIT.md §16).
                self.jit_flush_all().map_err(|()| crate::prims::FAIL_UNSUPPORTED_CONTEXT)?;
                let jit = self.jit.as_mut().unwrap();
                jit.profiling_level = arg.clamp(0, 2);
                Ok(jit.profiling_level)
            }
            _ => Err(crate::prims::FAIL_WRONG_TYPE),
        }
    }

    // --- GC integration (called from gc.rs) ---

    /// Root count bookkeeping for the mark phase: every queue entry and
    /// every handle's method.
    pub fn jit_roots(&self) -> Vec<Value> {
        match self.jit.as_ref() {
            None => Vec::new(),
            Some(jit) => jit
                .queue
                .iter()
                .copied()
                .chain(jit.handles.iter().map(|h| h.method))
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime stubs (Annex J.3). All extern "C", vm pointer first; panics are
// caught and surfaced as DISP_ERROR / null so unwinding never crosses the
// FFI boundary into generated code.
// ---------------------------------------------------------------------------

fn with_vm<R>(vm: *mut Vm, default: R, f: impl FnOnce(&mut Vm) -> R) -> R {
    let vmr = unsafe { &mut *vm };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(vmr))) {
        Ok(r) => r,
        Err(p) => {
            let msg = p
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "stub panic".into());
            let vm = unsafe { &mut *vm };
            if let Some(jit) = vm.jit.as_mut() {
                jit.pending_error = Some(VmError::Fatal(format!("panic in JIT stub: {msg}")));
            }
            default
        }
    }
}

/// ALLOC (allocating): returns the new object address, or 0 with a pending
/// error. May collect; re-derives nothing itself — templates re-derive FP
/// and reload operand slots afterwards (J2/§6).
unsafe extern "C" fn stub_alloc(vm: *mut Vm, class: u64, format: u64, nslots: u64) -> u64 {
    with_vm(vm, 0, |vm| {
        match vm.jit_alloc(class as u32, format, nslots as usize) {
            Ok(v) => v.raw(),
            Err(e) => {
                vm.jit.as_mut().unwrap().pending_error = Some(e);
                0
            }
        }
    })
}

/// BARRIER_REMEMBER (leaf): the SSB append slow path. The young-bounds
/// filter is inline in the template; this only runs for an old object
/// storing its first young referent since the last scavenge.
unsafe extern "C" fn stub_barrier_remember(vm: *mut Vm, obj: u64) -> u64 {
    with_vm(vm, 0, |vm| {
        let obj = obj as usize;
        let h = vm.heap.header(obj);
        if h.gc_bits() & GC_BIT_REMEMBERED == 0 {
            vm.heap
                .set_header(obj, h.with_gc_bits(h.gc_bits() | GC_BIT_REMEMBERED));
            vm.heap.ssb.push(obj);
        }
        0
    })
}

/// STACK_GROW (allocating): grow the active process's stack to hold at
/// least `needed_slots`. Returns the new stack object address (0 = error).
unsafe extern "C" fn stub_stack_grow(vm: *mut Vm, needed_slots: u64) -> u64 {
    with_vm(vm, 0, |vm| match vm.jit_grow_stack(needed_slots as usize) {
        Ok(addr) => addr as u64,
        Err(e) => {
            vm.jit.as_mut().unwrap().pending_error = Some(e);
            0
        }
    })
}

/// SAFEPOINT (exiting): called from the poll slow path with the current
/// bytecode pc as a template immediate. Stores Process.pc (making the
/// process suspendable), services timers/samples/preemption. Returns
/// DISP_CONTINUE, or DISP_EXIT after a process switch.
unsafe extern "C" fn stub_safepoint(vm: *mut Vm, pc: u64) -> u64 {
    with_vm(vm, DISP_ERROR, |vm| {
        let p = vm.active_process.as_ptr();
        vm.heap
            .set_slot_raw(p, PROCESS_PC, Value::from_int(pc as i64));
        vm.jit_service_safepoint()
    })
}

/// Placeholder for stubs not yet implemented (SEND_SLOW, MUST_BE_BOOLEAN —
/// both currently covered by the exit-to-interpreter fallback).
unsafe extern "C" fn stub_todo(vm: *mut Vm, _a: u64, _b: u64, _c: u64) -> u64 {
    with_vm(vm, DISP_ERROR, |vm| {
        if let Some(jit) = vm.jit.as_mut() {
            jit.pending_error = Some(VmError::Fatal("unimplemented JIT stub called".into()));
        }
        DISP_ERROR
    })
}

/// SEND_MISS (exiting): the compiled inline cache's class compare failed.
/// Performs the *whole* send through the interpreter's own do_send —
/// shared lookup, cache refill (which also re-patches this compiled site
/// via jit_patch_send_site), primitives, DNU — then answers where the
/// cold call site goes next: a native address (callee entry, or the
/// caller's continuation after a successful primitive), or an exit
/// disposition. `rd` packs the receiver slot and dest slot; `pc_after`
/// is the bytecode continuation (template immediates).
unsafe extern "C" fn stub_send_miss(vm: *mut Vm, site: u64, rd: u64, pc_after: u64) -> u64 {
    with_vm(vm, DISP_ERROR, |vm| {
        let r = (rd & 0xFF) as u8;
        let dest = ((rd >> 8) & 0xFF) as u8;
        let site = site as u8;
        let mut regs = vm.load_regs();
        regs.pc = pc_after as usize;
        let base = site as usize * SITE_STRIDE;
        let selector = vm.heap.slot(regs.sites, base + SITE_SELECTOR);
        let argc = vm.heap.slot(regs.sites, base + SITE_ARGC).as_int() as usize;
        let static_class = vm.heap.slot(regs.sites, base + SITE_STATIC_CLASS);
        let super_static = if static_class != vm.nil() && static_class.is_ptr() {
            Some(static_class)
        } else {
            None
        };
        // Root the caller method across the send (GC can run under it);
        // used afterwards to re-patch the compiled site even when the
        // *heap* cache already hit (e.g. right after an invalidation
        // cleared the compiled site but the interpreter re-warmed the
        // heap entry).
        vm.temp_roots.push(regs.method);
        let sent = vm.do_send(&mut regs, dest, r, selector, argc, Some(base), super_static, site);
        let caller = vm.temp_roots.pop().unwrap();
        match sent {
            Ok(()) => {
                let sites = vm.heap.slot(caller.as_ptr(), METHOD_SEND_SITES);
                if sites.is_ptr() && sites != vm.nil() {
                    let cls = vm.heap.slot(sites.as_ptr(), base + SITE_CACHE_CLASS);
                    let m = vm.heap.slot(sites.as_ptr(), base + SITE_CACHE_METHOD);
                    if cls.is_int() && cls.as_int() != 0 && m.is_ptr() && m != vm.nil() {
                        vm.jit_patch_send_site(caller, site, cls.as_int() as u32, m);
                    }
                }
                vm.stub_continuation(&mut regs)
            }
            Err(e) => {
                vm.jit.as_mut().unwrap().pending_error = Some(e);
                DISP_ERROR
            }
        }
    })
}

/// PRIM_CALL (exiting): the compiled PRIM template. Runs the current
/// frame's primitive with the interpreter's own machinery (r=0 framed
/// convention). Answers 0 on clean failure (the cold site re-derives FP
/// and falls into the compiled fallback body), else a continuation
/// address or an exit disposition.
unsafe extern "C" fn stub_prim_call(vm: *mut Vm) -> u64 {
    with_vm(vm, DISP_ERROR, |vm| {
        let mut regs = vm.load_regs();
        // Resume pc for any suspension: past the PRIM instruction.
        regs.pc = 1;
        let method = regs.method;
        let Some(n) = vm.method_primitive(method) else {
            vm.jit.as_mut().unwrap().pending_error =
                Some(VmError::Fatal("PRIM_CALL on method without primitive".into()));
            return DISP_ERROR;
        };
        let argc = vm.method_argc(method);
        match vm.run_primitive(n, &mut regs, 0, argc, 0, RETINFO_NO_SITE as u8) {
            Ok(crate::interp::PrimOutcome::Value(v)) => {
                match vm.do_return(&mut regs, v) {
                    Ok(()) => vm.stub_continuation(&mut regs),
                    Err(e) => {
                        vm.jit.as_mut().unwrap().pending_error = Some(e);
                        DISP_ERROR
                    }
                }
            }
            Ok(crate::interp::PrimOutcome::Fail(code)) => {
                vm.put(&regs, (1 + argc) as u8, Value::from_int(code));
                0 // continue in-line: the compiled fallback body
            }
            Ok(crate::interp::PrimOutcome::Control) => vm.stub_continuation(&mut regs),
            Err(e) => {
                vm.jit.as_mut().unwrap().pending_error = Some(e);
                DISP_ERROR
            }
        }
    })
}

/// SEND_SLOW (exiting): the specialized-send fallback (JIT.md §7): the
/// template's slow path hands the operation to the ordinary send path —
/// staged like the interpreter's spec_slow — without leaving the
/// trampoline. `packed` carries dest and up to three operand slots; the
/// operand count is implied by the specializedSelectors index.
unsafe extern "C" fn stub_send_slow(vm: *mut Vm, specsel: u64, packed: u64, pc_after: u64) -> u64 {
    with_vm(vm, DISP_ERROR, |vm| {
        let specsel = specsel as usize;
        let nops: usize = match specsel {
            SPECSEL_AT_PUT => 3,
            SPECSEL_SIZE | SPECSEL_CLASS | SPECSEL_NOT => 1,
            _ => 2,
        };
        let dest = (packed & 0xFF) as u8;
        let slots = [
            ((packed >> 8) & 0xFF) as u8,
            ((packed >> 16) & 0xFF) as u8,
            ((packed >> 24) & 0xFF) as u8,
        ];
        let mut regs = vm.load_regs();
        regs.pc = pc_after as usize;
        vm.counters.spec_fallthrough[specsel] += 1;
        let sels = vm.specials()[SPECIAL_SPECIALIZED_SELECTORS];
        let selector = vm.heap.slot(sels.as_ptr(), specsel);
        let vals: Vec<Value> = slots[..nops].iter().map(|s| vm.get(&regs, *s)).collect();
        match vm.send_staged(&mut regs, dest, selector, &vals) {
            Ok(()) => vm.stub_continuation(&mut regs),
            Err(e) => {
                vm.jit.as_mut().unwrap().pending_error = Some(e);
                DISP_ERROR
            }
        }
    })
}

/// RESUME_AT (exiting): the return template's siteless-return path.
/// Stores the resume pc (making the frame state current) and answers the
/// caller's native re-entry address when its resume pc is mapped —
/// staged specialized-send returns stay inside one trampoline instead of
/// exiting and re-entering.
unsafe extern "C" fn stub_resume_at(vm: *mut Vm, pc: u64) -> u64 {
    with_vm(vm, DISP_ERROR, |vm| {
        let p = vm.active_process.as_ptr();
        vm.heap
            .set_slot_raw(p, PROCESS_PC, Value::from_int(pc as i64));
        let regs = vm.load_regs();
        match vm.stub_resume_addr(&regs) {
            Some(a) => a,
            None => DISP_EXIT,
        }
    })
}

/// NLR (exiting): non-local return. `a` is the value slot; `pc_after`
/// the (unreachable, but staged-send-relevant) bytecode continuation.
unsafe extern "C" fn stub_nlr(vm: *mut Vm, a: u64, pc_after: u64) -> u64 {
    with_vm(vm, DISP_ERROR, |vm| {
        let mut regs = vm.load_regs();
        regs.pc = pc_after as usize;
        match vm.do_nlr(&mut regs, a as u8) {
            Ok(()) => vm.stub_continuation(&mut regs),
            Err(e) => {
                vm.jit.as_mut().unwrap().pending_error = Some(e);
                DISP_ERROR
            }
        }
    })
}

impl Vm {
    /// Allocation for compiled code: no Regs — the frame state is already
    /// current per the calling convention (Process.frameOffset), which is
    /// all the GC needs to scan the running stack.
    fn jit_alloc(&mut self, class: u32, format: u64, nslots: usize) -> Result<Value, VmError> {
        let nil = self.nil();
        let attempt = |heap: &mut crate::heap::Heap| match format {
            FMT_FIXED => heap.alloc_fixed(class, nslots, nil),
            FMT_PTRS => heap.alloc_ptrs(class, nslots, nil),
            _ => heap.alloc_bytes(class, nslots),
        };
        if let Some(a) = attempt(&mut self.heap) {
            return Ok(Value::from_ptr(a));
        }
        self.collect_young()?;
        if let Some(a) = attempt(&mut self.heap) {
            return Ok(Value::from_ptr(a));
        }
        self.collect_old()?;
        if let Some(a) = attempt(&mut self.heap) {
            return Ok(Value::from_ptr(a));
        }
        self.alloc_old_fallback(class, format, nslots)
            .map(Value::from_ptr)
            .ok_or(VmError::OutOfMemory)
    }

    /// Stack growth for compiled code (prologue slow path): same logic as
    /// the interpreter's grow_stack, operating on Process state.
    fn jit_grow_stack(&mut self, needed_slots: usize) -> Result<usize, VmError> {
        let p = self.active_process.as_ptr();
        let stack = self.heap.slot(p, PROCESS_STACK).as_ptr();
        let cur_slots = self.heap.num_slots(stack) as usize;
        let mut new_slots = cur_slots * 2;
        while new_slots < needed_slots {
            new_slots *= 2;
        }
        if new_slots * 8 > self.max_stack_bytes {
            return Err(VmError::StackOverflow);
        }
        let nil = self.nil();
        let new_addr = match self
            .heap
            .alloc_ptrs(CLASS_STACK, new_slots, nil)
            .or_else(|| self.heap.alloc_ptrs_old(CLASS_STACK, new_slots, nil))
        {
            Some(a) => a,
            None => {
                self.collect_old()?;
                self.heap
                    .alloc_ptrs_old(CLASS_STACK, new_slots, nil)
                    .ok_or(VmError::OutOfMemory)?
            }
        };
        // Re-derive: the collection may have moved process and stack.
        let p = self.active_process.as_ptr();
        let stack = self.heap.slot(p, PROCESS_STACK).as_ptr();
        let cur_slots = self.heap.num_slots(stack) as usize;
        for i in 0..cur_slots {
            let v = self.heap.slot(stack, i);
            self.heap.set_slot_raw(new_addr, i, v);
        }
        self.store_slot(p, PROCESS_STACK, Value::from_ptr(new_addr));
        self.stack_grow_count += 1;
        self.jit_sync_globals();
        Ok(new_addr)
    }

    /// The safepoint service for compiled code: mirrors
    /// `service_safepoint` minus the Regs bookkeeping (Process state was
    /// stored by the stub caller). Returns a disposition.
    fn jit_service_safepoint(&mut self) -> u64 {
        use std::sync::atomic::Ordering;
        if (self.safepoint.sample_due.swap(false, Ordering::Relaxed)
            || (self.profiler.sample_every_poll))
            && self.profiler.active
        {
            self.counters.jit_samples_native += 1;
            let p = self.active_process.as_ptr();
            let stack = self.heap.slot(p, PROCESS_STACK);
            let off = self.heap.slot(p, PROCESS_FRAME_OFFSET);
            if stack.is_ptr() && off.is_int() {
                self.take_sample(stack.as_ptr(), off.as_int() as usize, None);
            }
        }
        self.service_timers();
        // Stress mode keeps the flag armed so compiled polls keep firing
        // (the interpreter's per-poll sampling needs no flag).
        let keep_armed = !self.timer_requests.is_empty()
            || (self.profiler.active && self.profiler.sample_every_poll);
        self.safepoint.armed.store(keep_armed, Ordering::Relaxed);
        let cur = self.active_process;
        let cur_prio = self.heap.slot(cur.as_ptr(), PROCESS_PRIORITY).as_int();
        if let Some(hp) = self.runnable_priority_ceiling() {
            if hp > cur_prio {
                self.make_runnable(cur);
                if let Some(next) = self.take_next_runnable() {
                    self.transfer_raw(next);
                    return DISP_EXIT;
                }
            }
        }
        DISP_CONTINUE
    }
}
