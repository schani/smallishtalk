//! The interpreter (SPEC §6, §7, §8, §12, §19): a loop over u32 instructions
//! decoded with shifts and masks, operating on frame slots in the active
//! process's stack object.
//!
//! Cached-register discipline: `Regs` caches the stack address and the
//! current method's bytecodes/literals/send-sites addresses. Every operation
//! that can allocate (and therefore collect) goes through `alloc_gc`, which
//! saves pc/frameOffset to the Process first and refreshes `Regs` afterward
//! if a collection ran.

use crate::treaty::*;
use crate::value::Value;
use crate::vm::{fatal, Vm, VmError};

pub struct Regs {
    /// Stack object address of the active process.
    pub stack: usize,
    /// Body-slot index of the current frame's slot 0.
    pub frame: usize,
    /// Bytecode offset (instruction index) of the *next* instruction.
    pub pc: usize,
    pub method: Value,
    /// Cached addresses of the current method's parts.
    pub code: usize,
    pub lits: usize,
    pub sites: usize,
    /// GC epoch these caches were derived at.
    pub epoch: u64,
    /// Destination slot of the most recent MKCLOSURE (CAPTURE's target).
    pub closure_reg: Option<u8>,
    /// Set when the base frame returns: the process's final value.
    pub halted: Option<Value>,
}

pub enum PrimOutcome {
    /// Primitive succeeded with this result; no frame was pushed.
    Value(Value),
    /// Primitive failed cleanly; activate the fallback body with this code.
    Fail(i64),
    /// Primitive transferred control (block value, process switch, unwind):
    /// regs are already updated.
    Control,
}

#[inline(always)]
fn bslot(regs: &Regs, k: u8) -> usize {
    regs.frame + FRAME_RECEIVER + k as usize
}

impl Vm {
    // --- Register load/save/refresh ---

    pub fn load_regs(&self) -> Regs {
        let p = self.active_process.as_ptr();
        let stack = self.heap.slot(p, PROCESS_STACK).as_ptr();
        let frame = self.heap.slot(p, PROCESS_FRAME_OFFSET).as_int() as usize;
        let pc = self.heap.slot(p, PROCESS_PC).as_int() as usize;
        let mut regs = Regs {
            stack,
            frame,
            pc,
            method: self.nil(),
            code: 0,
            lits: 0,
            sites: 0,
            epoch: self.gc_epoch,
            closure_reg: None,
            halted: None,
        };
        self.reload_code(&mut regs);
        regs
    }

    pub fn save_regs(&mut self, regs: &Regs) {
        let p = self.active_process.as_ptr();
        self.heap
            .set_slot_raw(p, PROCESS_FRAME_OFFSET, Value::from_int(regs.frame as i64));
        self.heap
            .set_slot_raw(p, PROCESS_PC, Value::from_int(regs.pc as i64));
    }

    pub fn reload_code(&self, regs: &mut Regs) {
        regs.method = self.heap.slot(regs.stack, regs.frame + FRAME_METHOD);
        let m = regs.method.as_ptr();
        regs.code = self.heap.slot(m, METHOD_BYTECODES).as_ptr();
        let nil = self.nil();
        let lits = self.heap.slot(m, METHOD_LITERALS);
        regs.lits = if lits != nil { lits.as_ptr() } else { 0 };
        let sites = self.heap.slot(m, METHOD_SEND_SITES);
        regs.sites = if sites != nil { sites.as_ptr() } else { 0 };
    }

    /// Re-derive all cached addresses after a possible collection.
    pub fn refresh_regs(&self, regs: &mut Regs) {
        if regs.epoch != self.gc_epoch {
            let p = self.active_process.as_ptr();
            regs.stack = self.heap.slot(p, PROCESS_STACK).as_ptr();
            self.reload_code(regs);
            regs.epoch = self.gc_epoch;
        }
    }

    // --- Slot access (bytecode numbering: slot 0 = receiver) ---

    #[inline(always)]
    pub fn get(&self, regs: &Regs, k: u8) -> Value {
        self.heap.slot(regs.stack, bslot(regs, k))
    }

    /// Frame-slot stores into the running process's stack are exempt from
    /// the write barrier (the running stack is always a GC root).
    #[inline(always)]
    pub fn put(&mut self, regs: &Regs, k: u8, v: Value) {
        self.heap.set_slot_raw(regs.stack, bslot(regs, k), v);
    }

    // --- Allocation with GC retry ---

    pub fn alloc_gc(
        &mut self,
        regs: &mut Regs,
        class: u32,
        format: u64,
        n: usize,
    ) -> Result<Value, VmError> {
        let nil = self.nil();
        let attempt = |heap: &mut crate::heap::Heap| match format {
            FMT_FIXED => heap.alloc_fixed(class, n, nil),
            FMT_PTRS => heap.alloc_ptrs(class, n, nil),
            _ => heap.alloc_bytes(class, n),
        };
        if let Some(a) = attempt(&mut self.heap) {
            return Ok(Value::from_ptr(a));
        }
        self.save_regs(regs);
        self.collect_young()?;
        self.refresh_regs(regs);
        if let Some(a) = attempt(&mut self.heap) {
            return Ok(Value::from_ptr(a));
        }
        // Still failing: old space (large objects, tenure pressure).
        self.save_regs(regs);
        self.collect_old()?;
        self.refresh_regs(regs);
        attempt(&mut self.heap)
            .map(Value::from_ptr)
            .ok_or(VmError::OutOfMemory)
    }

    // --- The run loop ---

    /// The context switch (§13): store pc/frameOffset into the outgoing
    /// process, remember its (possibly old) stack, load the target's state.
    pub fn transfer_to(&mut self, regs: &mut Regs, target: Value) {
        self.counters.process_switches += 1;
        let nil = self.nil();
        let cur = self.active_process;
        if cur.is_ptr() && cur != nil {
            let stack = self.heap.slot(cur.as_ptr(), PROCESS_STACK);
            if stack != nil {
                self.save_regs(regs);
                // While running, stores into this stack were barrier-exempt;
                // once it stops being the running stack it must be in the
                // remembered set if it lives in old space.
                let sa = stack.as_ptr();
                if self.heap.in_old_space(sa) {
                    let h = self.heap.header(sa);
                    if h.gc_bits() & GC_BIT_REMEMBERED == 0 {
                        self.heap
                            .set_header(sa, h.with_gc_bits(h.gc_bits() | GC_BIT_REMEMBERED));
                        self.heap.ssb.push(sa);
                    }
                }
            }
        }
        let sched = self.specials()[SPECIAL_PROCESSOR];
        self.store_slot(sched.as_ptr(), SCHEDULER_ACTIVE_PROCESS, target);
        self.active_process = target;
        *regs = self.load_regs();
    }

    /// Run the given process until its base frame returns. Other processes
    /// that finish first are terminated and the scheduler picks the next
    /// runnable one.
    pub fn run(&mut self, process: Value) -> Result<Value, VmError> {
        // The run target is a GC root (collections move it); track it via
        // temp_roots so the halt check compares current addresses.
        self.temp_roots.push(process);
        let target_slot = self.temp_roots.len() - 1;
        let result = self.run_rooted(process, target_slot);
        self.temp_roots.truncate(target_slot);
        result
    }

    fn run_rooted(&mut self, process: Value, target_slot: usize) -> Result<Value, VmError> {
        self.active_process = process;
        let sched = self.specials()[SPECIAL_PROCESSOR];
        self.store_slot(sched.as_ptr(), SCHEDULER_ACTIVE_PROCESS, process);
        let mut regs = self.load_regs();
        loop {
            let word = self.heap.insn_at(regs.code, regs.pc);
            regs.pc += 1;
            let op = (word & 0xFF) as u8;
            let a = ((word >> 8) & 0xFF) as u8;
            let b = ((word >> 16) & 0xFF) as u8;
            let c = (word >> 24) as u8;
            let d16 = (word >> 16) as u16;

            // Gated counter tier (profiling plan §3): one predictable
            // branch on the dispatch path, and only with the feature on.
            #[cfg(feature = "vm-counters")]
            if self.counters.gate {
                self.counters.insns += 1;
                self.counters.opcode_hist[op as usize] += 1;
                self.counters.gap_current += 1;
            }

            match op {
                OP_NOP => {}
                OP_BREAK => return fatal("BREAK executed"),

                // --- Data movement ---
                OP_MOVE => {
                    let v = self.get(&regs, b);
                    self.put(&regs, a, v);
                }
                OP_LOADK => {
                    let v = self.heap.slot(regs.lits, d16 as usize);
                    self.put(&regs, a, v);
                }
                OP_LOADINT => {
                    self.put(&regs, a, Value::from_int(d16 as i16 as i64));
                }
                OP_LOADNIL => {
                    let v = self.nil();
                    self.put(&regs, a, v);
                }
                OP_LOADTRUE => {
                    let v = self.true_v();
                    self.put(&regs, a, v);
                }
                OP_LOADFALSE => {
                    let v = self.false_v();
                    self.put(&regs, a, v);
                }
                OP_LOADSELF => {
                    let v = self.get(&regs, 0);
                    self.put(&regs, a, v);
                }
                OP_GETIVAR => {
                    let recv = self.get(&regs, 0);
                    if !recv.is_ptr() {
                        return fatal("GETIVAR on immediate receiver");
                    }
                    let v = self.heap.slot(recv.as_ptr(), b as usize);
                    self.put(&regs, a, v);
                }
                OP_SETIVAR => {
                    let recv = self.get(&regs, 0);
                    if !recv.is_ptr() {
                        return fatal("SETIVAR on immediate receiver");
                    }
                    let v = self.get(&regs, b);
                    self.store_slot(recv.as_ptr(), a as usize, v);
                }
                OP_GETBOX => {
                    let boxv = self.get(&regs, b);
                    if !boxv.is_ptr() {
                        return fatal("GETBOX on non-box");
                    }
                    let v = self.heap.slot(boxv.as_ptr(), 0);
                    self.put(&regs, a, v);
                }
                OP_SETBOX => {
                    let boxv = self.get(&regs, a);
                    if !boxv.is_ptr() {
                        return fatal("SETBOX on non-box");
                    }
                    let v = self.get(&regs, b);
                    self.store_slot(boxv.as_ptr(), 0, v);
                }
                OP_MKBOX => {
                    let boxv = self.alloc_gc(&mut regs, CLASS_BOX, FMT_FIXED, 1)?;
                    let v = self.get(&regs, b);
                    self.heap.set_slot_raw(boxv.as_ptr(), 0, v);
                    self.put(&regs, a, boxv);
                }

                // --- Control ---
                OP_JUMP => {
                    let off = d16 as i16 as isize;
                    regs.pc = (regs.pc as isize + off) as usize;
                    if off < 0 {
                        self.poll_safepoint(&mut regs)?;
                    }
                }
                OP_JUMPTRUE | OP_JUMPFALSE => {
                    let v = self.get(&regs, a);
                    let want = if op == OP_JUMPTRUE { self.true_v() } else { self.false_v() };
                    let other = if op == OP_JUMPTRUE { self.false_v() } else { self.true_v() };
                    if v == want {
                        let off = d16 as i16 as isize;
                        regs.pc = (regs.pc as isize + off) as usize;
                        if off < 0 {
                            self.poll_safepoint(&mut regs)?;
                        }
                    } else if v != other {
                        // mustBeBoolean: send it, result replaces slot a,
                        // then the jump itself re-executes.
                        self.counters.must_be_boolean += 1;
                        let jump_pc = regs.pc - 1;
                        regs.pc = jump_pc;
                        let sel = self.specials()[SPECIAL_SEL_MUST_BE_BOOLEAN];
                        self.send_staged(&mut regs, a, sel, &[v])?;
                    }
                }
                OP_RET => {
                    let v = self.get(&regs, a);
                    self.do_return(&mut regs, v)?;
                }
                OP_RETSELF => {
                    let v = self.get(&regs, 0);
                    self.do_return(&mut regs, v)?;
                }
                OP_NLR => {
                    self.do_nlr(&mut regs, a)?;
                }
                OP_PRIM => {
                    let method = regs.method;
                    let argc = self.method_argc(method);
                    match self.run_primitive(d16, &mut regs, 0, argc, 0)? {
                        PrimOutcome::Value(v) => {
                            self.do_return(&mut regs, v)?;
                        }
                        PrimOutcome::Fail(code) => {
                            self.put(&regs, (1 + argc) as u8, Value::from_int(code));
                        }
                        PrimOutcome::Control => {}
                    }
                }

                // --- Sends ---
                OP_SEND | OP_SENDSUPER => {
                    self.poll_safepoint(&mut regs)?;
                    if self.snapshot_after_sends.is_some() {
                        self.sends_seen += 1;
                        if let Some((n, path)) = self.snapshot_after_sends.clone() {
                            if self.sends_seen == n {
                                // Snapshot with pc pointing AT this send:
                                // the resumed image re-executes it (the
                                // staging slots are part of the frame).
                                regs.pc -= 1;
                                self.snapshot_now(&mut regs, &path)?;
                                regs.pc += 1;
                            }
                        }
                    }
                    if regs.sites == 0 {
                        return fatal("SEND without send-site table");
                    }
                    let base = c as usize * SITE_STRIDE;
                    let selector = self.heap.slot(regs.sites, base + SITE_SELECTOR);
                    let argc = self.heap.slot(regs.sites, base + SITE_ARGC).as_int() as usize;
                    let super_static = if op == OP_SENDSUPER {
                        Some(self.heap.slot(regs.sites, base + SITE_STATIC_CLASS))
                    } else {
                        None
                    };
                    self.do_send(&mut regs, a, b, selector, argc, Some(base), super_static)?;
                }

                // --- Closures ---
                OP_MKCLOSURE => {
                    self.do_mkclosure(&mut regs, a, d16)?;
                }
                OP_CAPTURE => {
                    let Some(cl_slot) = regs.closure_reg else {
                        return fatal("CAPTURE without preceding MKCLOSURE");
                    };
                    let closure = self.get(&regs, cl_slot);
                    let v = self.get(&regs, b);
                    self.store_slot(closure.as_ptr(), CLOSURE_CAPTURED_BASE + a as usize, v);
                }

                // --- Specialized sends (§12) ---
                OP_ADD | OP_SUB | OP_MUL => {
                    let x = self.get(&regs, b);
                    let y = self.get(&regs, c);
                    let done = if x.is_int() && y.is_int() {
                        let r = match op {
                            OP_ADD => x.as_int().checked_add(y.as_int()),
                            OP_SUB => x.as_int().checked_sub(y.as_int()),
                            _ => x.as_int().checked_mul(y.as_int()),
                        };
                        match r.and_then(Value::try_from_int) {
                            Some(v) => {
                                self.put(&regs, a, v);
                                true
                            }
                            None => false,
                        }
                    } else {
                        false
                    };
                    if !done {
                        let sel_idx = match op {
                            OP_ADD => SPECSEL_PLUS,
                            OP_SUB => SPECSEL_MINUS,
                            _ => SPECSEL_TIMES,
                        };
                        self.spec_slow(&mut regs, sel_idx, a, &[b, c])?;
                    }
                }
                OP_DIV | OP_MOD => {
                    let x = self.get(&regs, b);
                    let y = self.get(&regs, c);
                    let mut done = false;
                    if x.is_int() && y.is_int() && y.as_int() != 0 {
                        let (xi, yi) = (x.as_int(), y.as_int());
                        // Floored semantics; i64::MIN/-1 can't occur (63-bit ints).
                        let (q, r) = floored_divmod(xi, yi);
                        let res = if op == OP_DIV { q } else { r };
                        if let Some(v) = Value::try_from_int(res) {
                            self.put(&regs, a, v);
                            done = true;
                        }
                    }
                    if !done {
                        let sel_idx = if op == OP_DIV { SPECSEL_INT_DIV } else { SPECSEL_MOD };
                        self.spec_slow(&mut regs, sel_idx, a, &[b, c])?;
                    }
                }
                OP_LT | OP_GT | OP_LE | OP_GE | OP_EQNUM => {
                    let x = self.get(&regs, b);
                    let y = self.get(&regs, c);
                    if x.is_int() && y.is_int() {
                        let (xi, yi) = (x.as_int(), y.as_int());
                        let r = match op {
                            OP_LT => xi < yi,
                            OP_GT => xi > yi,
                            OP_LE => xi <= yi,
                            OP_GE => xi >= yi,
                            _ => xi == yi,
                        };
                        let v = self.bool_v(r);
                        self.put(&regs, a, v);
                    } else {
                        let sel_idx = match op {
                            OP_LT => SPECSEL_LT,
                            OP_GT => SPECSEL_GT,
                            OP_LE => SPECSEL_LE,
                            OP_GE => SPECSEL_GE,
                            _ => SPECSEL_EQ,
                        };
                        self.spec_slow(&mut regs, sel_idx, a, &[b, c])?;
                    }
                }
                OP_IDEQ => {
                    // Identity is universal: raw word compare, no slow path.
                    let x = self.get(&regs, b);
                    let y = self.get(&regs, c);
                    let v = self.bool_v(x == y);
                    self.put(&regs, a, v);
                }
                OP_AT => {
                    let recv = self.get(&regs, b);
                    let idx = self.get(&regs, c);
                    match self.at_fast(recv, idx) {
                        Some(v) => self.put(&regs, a, v),
                        None => self.spec_slow(&mut regs, SPECSEL_AT, a, &[b, c])?,
                    }
                }
                OP_ATPUT => {
                    // receiver a=b-field, index c-field, value at slot c+1.
                    let recv = self.get(&regs, b);
                    let idx = self.get(&regs, c);
                    let val = self.get(&regs, c + 1);
                    match self.atput_fast(recv, idx, val) {
                        Some(v) => self.put(&regs, a, v),
                        None => self.spec_slow(&mut regs, SPECSEL_AT_PUT, a, &[b, c, c + 1])?,
                    }
                }
                OP_SIZE => {
                    let recv = self.get(&regs, b);
                    match self.size_fast(recv) {
                        Some(v) => self.put(&regs, a, v),
                        None => self.spec_slow(&mut regs, SPECSEL_SIZE, a, &[b])?,
                    }
                }
                OP_CLASSOF => {
                    let recv = self.get(&regs, b);
                    let v = self.class_of(recv);
                    self.put(&regs, a, v);
                }
                OP_NOT => {
                    let recv = self.get(&regs, b);
                    if recv == self.true_v() {
                        let v = self.false_v();
                        self.put(&regs, a, v);
                    } else if recv == self.false_v() {
                        let v = self.true_v();
                        self.put(&regs, a, v);
                    } else {
                        self.spec_slow(&mut regs, SPECSEL_NOT, a, &[b])?;
                    }
                }

                _ => return fatal(format!("illegal opcode {op:#x} at pc {}", regs.pc - 1)),
            }

            if op != OP_MKCLOSURE {
                regs.closure_reg = if op == OP_CAPTURE { regs.closure_reg } else { None };
            }
            if let Some(v) = regs.halted {
                // The active process's base frame returned: it terminates
                // (§7: terminated = stack is nil).
                let nil = self.nil();
                let cur = self.active_process;
                self.store_slot(cur.as_ptr(), PROCESS_STACK, nil);
                self.heap.set_slot_raw(cur.as_ptr(), PROCESS_MY_LIST, nil);
                if cur == self.temp_roots[target_slot] {
                    return Ok(v);
                }
                let next = self.pick_next_or_wait()?;
                self.transfer_to(&mut regs, next);
                regs.halted = None;
            }
        }
    }

    // --- Specialized-send fast-path helpers ---

    fn at_fast(&self, recv: Value, idx: Value) -> Option<Value> {
        if !recv.is_ptr() || !idx.is_int() {
            return None;
        }
        let addr = recv.as_ptr();
        let h = self.heap.header(addr);
        let i = idx.as_int();
        if h.format() == FMT_PTRS {
            let n = self.heap.num_slots(addr) as i64;
            if i >= 1 && i <= n {
                return Some(self.heap.slot(addr, (i - 1) as usize));
            }
        } else if h.is_bytes() {
            let n = self.heap.byte_size(addr) as i64;
            if i >= 1 && i <= n {
                return Some(Value::from_int(self.heap.byte(addr, (i - 1) as usize) as i64));
            }
        }
        None
    }

    fn atput_fast(&mut self, recv: Value, idx: Value, val: Value) -> Option<Value> {
        if !recv.is_ptr() || !idx.is_int() {
            return None;
        }
        let addr = recv.as_ptr();
        if self.heap.is_immutable(addr) {
            return None;
        }
        let h = self.heap.header(addr);
        let i = idx.as_int();
        if h.format() == FMT_PTRS {
            let n = self.heap.num_slots(addr) as i64;
            if i >= 1 && i <= n {
                self.store_slot(addr, (i - 1) as usize, val);
                return Some(val);
            }
        } else if h.is_bytes() {
            let n = self.heap.byte_size(addr) as i64;
            if i >= 1 && i <= n && val.is_int() {
                let byte = val.as_int();
                if (0..=255).contains(&byte) {
                    self.heap.set_byte(addr, (i - 1) as usize, byte as u8);
                    return Some(val);
                }
            }
        }
        None
    }

    fn size_fast(&self, recv: Value) -> Option<Value> {
        if !recv.is_ptr() {
            return None;
        }
        let addr = recv.as_ptr();
        let h = self.heap.header(addr);
        if h.format() == FMT_PTRS {
            Some(Value::from_int(self.heap.num_slots(addr) as i64))
        } else if h.is_bytes() {
            Some(Value::from_int(self.heap.byte_size(addr) as i64))
        } else {
            None
        }
    }

    /// Slow path of a specialized send: stage receiver+args contiguously in
    /// the free area above the current frame, then dispatch as a real send
    /// of the corresponding selector (§12).
    fn spec_slow(
        &mut self,
        regs: &mut Regs,
        specsel: usize,
        dest: u8,
        operand_slots: &[u8],
    ) -> Result<(), VmError> {
        self.counters.spec_fallthrough[specsel] += 1;
        let sels = self.specials()[SPECIAL_SPECIALIZED_SELECTORS];
        let selector = self.heap.slot(sels.as_ptr(), specsel);
        let vals: Vec<Value> = operand_slots.iter().map(|s| self.get(regs, *s)).collect();
        self.send_staged(regs, dest, selector, &vals)
    }

    /// Stage values (receiver first) contiguously above the current frame's
    /// slots and dispatch as an ordinary send.
    fn send_staged(
        &mut self,
        regs: &mut Regs,
        dest: u8,
        selector: Value,
        recv_and_args: &[Value],
    ) -> Result<(), VmError> {
        self.counters.sends_staged += 1;
        let fs = self.method_frame_slots(regs.method);
        // The callee's four control words land in bytecode slots r-4..r-1,
        // which must not overlap the frame's live slots: stage a full gap
        // above them.
        let r = fs + FRAME_RECEIVER;
        if r + recv_and_args.len() > 250 {
            return fatal("staging area exceeds frame-slot limit");
        }
        let needed = regs.frame + FRAME_RECEIVER + r + recv_and_args.len();
        if needed > self.heap.num_slots(regs.stack) as usize {
            self.grow_stack(regs, needed)?;
        }
        for (i, v) in recv_and_args.iter().enumerate() {
            self.put(regs, (r + i) as u8, *v);
        }
        self.do_send(
            regs,
            dest,
            r as u8,
            selector,
            recv_and_args.len() - 1,
            None,
            None,
        )
    }

    // --- Sends ---

    /// The full send: lookup (through caches when a site is given),
    /// primitive check, activation, DNU.
    pub fn do_send(
        &mut self,
        regs: &mut Regs,
        dest: u8,
        r: u8,
        selector: Value,
        argc: usize,
        site_base: Option<usize>,
        super_static: Option<Value>,
    ) -> Result<(), VmError> {
        let receiver = self.get(regs, r);
        let class_index = self.class_index_of(receiver);

        #[cfg(feature = "vm-counters")]
        if self.counters.gate {
            self.counters.sends += 1;
        }

        // Inline cache (only for plain sends; SENDSUPER's target is static
        // per receiver-class anyway and shares the same fields).
        if let Some(base) = site_base {
            let cached = self.heap.slot(regs.sites, base + SITE_CACHE_CLASS).as_int();
            if cached == class_index as i64 {
                let method = self.heap.slot(regs.sites, base + SITE_CACHE_METHOD);
                return self.activate(regs, method, r, dest, argc);
            }
            self.counters.inline_cache_miss += 1;
        }

        let looked_up = match super_static {
            Some(static_class) => self.lookup_method_above(static_class, selector),
            None => self.lookup_cached(class_index, selector),
        };

        match looked_up {
            Some(method) => {
                if let Some(base) = site_base {
                    // Refill the inline cache.
                    let sites = regs.sites;
                    self.heap
                        .set_slot_raw(sites, base + SITE_CACHE_CLASS, Value::from_int(class_index as i64));
                    self.store_slot(sites, base + SITE_CACHE_METHOD, method);
                }
                self.activate(regs, method, r, dest, argc)
            }
            None => self.does_not_understand(regs, dest, r, selector, argc),
        }
    }

    /// Global lookup cache in front of the dictionary walk (§8 step 3-4).
    fn lookup_cached(&mut self, class_index: u32, selector: Value) -> Option<Value> {
        let h = ((class_index as u64).wrapping_mul(31).wrapping_add(selector.raw() >> 3)
            as usize)
            % LOOKUP_CACHE_SIZE;
        let e = &self.lookup_cache[h];
        if e.class_index == class_index && e.selector == selector {
            return Some(e.method);
        }
        self.counters.global_cache_miss += 1;
        self.counters.dict_walks += 1;
        let (m, walked) = self.lookup_method_counted(class_index, selector);
        self.counters.dict_classes_walked += walked;
        let m = m?;
        self.lookup_cache[h] = crate::vm::LookupEntry {
            class_index,
            selector,
            method: m,
        };
        Some(m)
    }

    fn does_not_understand(
        &mut self,
        regs: &mut Regs,
        dest: u8,
        r: u8,
        selector: Value,
        argc: usize,
    ) -> Result<(), VmError> {
        self.counters.dnu += 1;
        // Build the Message object (allocated on this slow path only).
        let args_arr = self.alloc_gc(regs, CLASS_ARRAY, FMT_PTRS, argc)?;
        for i in 0..argc {
            let v = self.get(regs, r + 1 + i as u8);
            self.heap.set_slot_raw(args_arr.as_ptr(), i, v);
        }
        self.temp_roots.push(args_arr);
        let msg = self.alloc_gc(regs, CLASS_MESSAGE, FMT_FIXED, 2);
        let args_arr = self.temp_roots.pop().unwrap();
        let msg = msg?;
        self.heap.set_slot_raw(msg.as_ptr(), 0, selector);
        self.heap.set_slot_raw(msg.as_ptr(), 1, args_arr);

        let receiver = self.get(regs, r);
        let class_index = self.class_index_of(receiver);
        let dnu_sel = self.specials()[SPECIAL_SEL_DOES_NOT_UNDERSTAND];
        let Some(method) = self.lookup_cached(class_index, dnu_sel) else {
            let name = if selector.is_ptr() {
                String::from_utf8_lossy(self.heap.bytes(selector.as_ptr())).into_owned()
            } else {
                format!("{selector:?}")
            };
            return fatal(format!(
                "doesNotUnderstand: #{name} (and no doesNotUnderstand: handler; \
                 receiver {receiver:?}, class index {class_index}, \
                 scavenges {})",
                self.scavenge_count
            ));
        };
        self.put(regs, r + 1, msg);
        self.activate(regs, method, r, dest, 1)
    }

    /// Activation (§8): run the primitive if the method has one, else (or on
    /// primitive failure) push a frame.
    fn activate(
        &mut self,
        regs: &mut Regs,
        method: Value,
        r: u8,
        dest: u8,
        argc: usize,
    ) -> Result<(), VmError> {
        if let Some(n) = self.method_primitive(method) {
            match self.run_primitive(n, regs, r, argc, dest)? {
                PrimOutcome::Value(v) => {
                    self.put(regs, dest, v);
                    return Ok(());
                }
                PrimOutcome::Control => return Ok(()),
                PrimOutcome::Fail(code) => {
                    return self.push_frame(regs, method, r, dest, argc, Some(code), 0);
                }
            }
        }
        self.push_frame(regs, method, r, dest, argc, None, 0)
    }

    /// Push a frame for `method` whose receiver already sits at bytecode
    /// slot `r` of the current frame (overlapping calling convention, §7).
    pub fn push_frame(
        &mut self,
        regs: &mut Regs,
        method: Value,
        r: u8,
        dest: u8,
        argc: usize,
        prim_fail: Option<i64>,
        extra_flags: u64,
    ) -> Result<(), VmError> {
        debug_assert!(r as usize >= FRAME_RECEIVER, "send staged below slot 4");
        let fs = self.method_frame_slots(method);
        let new_off = regs.frame + r as usize;
        let needed = new_off + FRAME_RECEIVER + fs;
        if needed > self.heap.num_slots(regs.stack) as usize {
            self.grow_stack(regs, needed)?;
        }
        let serial = self.bump_serial();
        // Handler/ensure frame bits come from the method header (§11).
        let mh_flags = (self.method_header_bits(method) >> MH_FLAGS_SHIFT) as u64;
        let mut fflags = extra_flags;
        if mh_flags & MH_FLAG_IS_HANDLER != 0 {
            fflags |= FLAG_HANDLER;
        }
        if mh_flags & MH_FLAG_IS_ENSURE != 0 {
            fflags |= FLAG_ENSURE;
        }
        let st = regs.stack;
        self.heap
            .set_slot_raw(st, new_off + FRAME_CALLER, Value::from_int(regs.frame as i64));
        self.heap.set_slot_raw(
            st,
            new_off + FRAME_RETINFO,
            Value::from_int((dest as i64) | ((regs.pc as i64) << RETINFO_PC_SHIFT)),
        );
        self.heap.set_slot_raw(st, new_off + FRAME_METHOD, method);
        self.heap.set_slot_raw(
            st,
            new_off + FRAME_FLAGS,
            Value::from_int((fflags as i64) | ((serial as i64) << SERIAL_SHIFT)),
        );
        // Receiver and args are already in place; nil the temps and scratch
        // so the frame's live extent contains only valid tagged words.
        let nil = self.nil();
        for k in (1 + argc)..fs {
            self.heap
                .set_slot_raw(st, new_off + FRAME_RECEIVER + k, nil);
        }
        regs.frame = new_off;
        self.reload_code(regs);
        // On primitive failure, enter past the PRIM instruction and expose
        // the failure code in the reserved slot after the arguments.
        if let Some(code) = prim_fail {
            self.put(regs, (1 + argc) as u8, Value::from_int(code));
            let first = self.heap.insn_at(regs.code, 0);
            regs.pc = if (first & 0xFF) as u8 == OP_PRIM { 1 } else { 0 };
        } else {
            regs.pc = 0;
        }
        Ok(())
    }

    fn bump_serial(&mut self) -> u32 {
        let p = self.active_process.as_ptr();
        let cur = self.heap.slot(p, PROCESS_SERIAL_COUNTER).as_int() as u32;
        // Serials live in bits 32.. of a 63-bit SmallInteger, so wrap at
        // 2^30 (the spec accepts wraparound).
        let next = if cur >= (1 << 30) - 1 { 1 } else { cur + 1 };
        self.heap
            .set_slot_raw(p, PROCESS_SERIAL_COUNTER, Value::from_int(next as i64));
        next
    }

    /// Sets regs.halted when the base frame returns (process finished).
    pub fn do_return(&mut self, regs: &mut Regs, value: Value) -> Result<(), VmError> {
        let flags = self.heap.slot(regs.stack, regs.frame + FRAME_FLAGS).as_int();
        if flags & (FLAG_UNWINDCONT as i64) != 0 {
            return self.continue_unwind_after_ensure(regs, value);
        }
        let caller = self
            .heap
            .slot(regs.stack, regs.frame + FRAME_CALLER)
            .as_int();
        if caller == 0 {
            regs.halted = Some(value);
            return Ok(());
        }
        let ri = self
            .heap
            .slot(regs.stack, regs.frame + FRAME_RETINFO)
            .as_int();
        let dest = (ri & ((1 << RETINFO_DEST_BITS) - 1)) as u8;
        self.nil_frame(regs);
        regs.pc = (ri >> RETINFO_PC_SHIFT) as usize;
        regs.frame = caller as usize;
        self.reload_code(regs);
        self.put(regs, dest, value);
        Ok(())
    }

    /// Nil out the current frame's region so popped frames pin no garbage
    /// (the GC scans whole stack objects; dead slots must not retain or
    /// resurrect objects).
    pub fn nil_frame(&mut self, regs: &Regs) {
        let fs = self.method_frame_slots(regs.method);
        let nil = self.nil();
        for i in 0..FRAME_RECEIVER + fs {
            self.heap.set_slot_raw(regs.stack, regs.frame + i, nil);
        }
    }

    // --- Stack growth (§7) ---

    /// Leaf path: must not itself trigger GC. Young space if possible,
    /// old space directly otherwise.
    pub fn grow_stack(&mut self, regs: &mut Regs, needed_slots: usize) -> Result<(), VmError> {
        let cur_slots = self.heap.num_slots(regs.stack) as usize;
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
                // Not a leaf anymore, but preferable to dying: compact old
                // space and retry once.
                self.save_regs(regs);
                self.collect_old()?;
                self.refresh_regs(regs);
                self.heap
                    .alloc_ptrs_old(CLASS_STACK, new_slots, nil)
                    .ok_or(VmError::OutOfMemory)?
            }
        };
        for i in 0..cur_slots {
            let v = self.heap.slot(regs.stack, i);
            self.heap.set_slot_raw(new_addr, i, v);
        }
        let process = self.active_process;
        let new_stack = Value::from_ptr(new_addr);
        self.store_slot(process.as_ptr(), PROCESS_STACK, new_stack);
        regs.stack = new_addr;
        self.stack_grow_count += 1;
        Ok(())
    }

    // --- Safepoints (§13) ---

    pub fn poll_safepoint(&mut self, regs: &mut Regs) -> Result<(), VmError> {
        #[cfg(feature = "vm-counters")]
        if self.counters.gate {
            self.counters.record_gap();
        }
        // Stress/test mode: a sample at *every* poll (walkability contract
        // enforcement, plan §2). Never set in production.
        if self.profiler.sample_every_poll && self.profiler.active {
            self.save_regs(regs);
            self.take_sample(regs.stack, regs.frame, None);
        }
        if self.safepoint.armed.load(std::sync::atomic::Ordering::Relaxed) {
            self.save_regs(regs);
            self.service_safepoint(regs)?;
            self.refresh_regs(regs);
        }
        Ok(())
    }

    /// Timer expiry, profiler samples, and preemption checks (§13). The
    /// flag stays armed while timers are pending (the poll is the v1 tick
    /// source); the profiler timer thread re-arms it on its own.
    fn service_safepoint(&mut self, regs: &mut Regs) -> Result<(), VmError> {
        use std::sync::atomic::Ordering;
        if self.safepoint.sample_due.swap(false, Ordering::Relaxed) && self.profiler.active {
            self.take_sample(regs.stack, regs.frame, None);
        }
        self.service_timers();
        self.safepoint
            .armed
            .store(!self.timer_requests.is_empty(), Ordering::Relaxed);
        let cur = self.active_process;
        let cur_prio = self.heap.slot(cur.as_ptr(), PROCESS_PRIORITY).as_int();
        if let Some(hp) = self.runnable_priority_ceiling() {
            if hp > cur_prio {
                self.make_runnable(cur);
                if let Some(next) = self.take_next_runnable() {
                    self.transfer_to(regs, next);
                }
            }
        }
        Ok(())
    }

    // --- Closures (§10) ---

    fn do_mkclosure(&mut self, regs: &mut Regs, d: u8, b: u16) -> Result<(), VmError> {
        let block = self.heap.slot(regs.lits, b as usize);
        let info = self.heap.slot(block.as_ptr(), BLOCK_INFO).as_int();
        let ncap = (info & ((1 << BI_NUM_CAPTURED_BITS) - 1)) as usize;
        let has_nlr = (info >> BI_HAS_NLR_SHIFT) & 1 != 0;

        let closure = self.alloc_gc(
            regs,
            CLASS_BLOCKCLOSURE,
            FMT_FIXED,
            CLOSURE_CAPTURED_BASE + ncap,
        )?;
        let block = self.heap.slot(regs.lits, b as usize); // re-read post-GC
        let ca = closure.as_ptr();
        self.heap.set_slot_raw(ca, CLOSURE_COMPILED_BLOCK, block);
        if has_nlr {
            let flags = self
                .heap
                .slot(regs.stack, regs.frame + FRAME_FLAGS)
                .as_int();
            if flags & (FLAG_BLOCKCTX as i64) != 0 {
                // Created inside a block activation: the home is the
                // enclosing closure's home, not this activation.
                let encl = self.get(regs, 0);
                for i in [CLOSURE_HOME_PROCESS, CLOSURE_HOME_OFFSET, CLOSURE_HOME_SERIAL] {
                    let v = self.heap.slot(encl.as_ptr(), i);
                    self.heap.set_slot_raw(ca, i, v);
                }
            } else {
                let ap = self.active_process;
                self.heap.set_slot_raw(ca, CLOSURE_HOME_PROCESS, ap);
                self.heap
                    .set_slot_raw(ca, CLOSURE_HOME_OFFSET, Value::from_int(regs.frame as i64));
                self.heap.set_slot_raw(
                    ca,
                    CLOSURE_HOME_SERIAL,
                    Value::from_int(flags >> SERIAL_SHIFT),
                );
            }
        } else {
            let nil = self.nil();
            self.heap.set_slot_raw(ca, CLOSURE_HOME_PROCESS, nil);
            self.heap
                .set_slot_raw(ca, CLOSURE_HOME_OFFSET, Value::from_int(0));
            self.heap
                .set_slot_raw(ca, CLOSURE_HOME_SERIAL, Value::from_int(0));
        }
        self.put(regs, d, closure);
        regs.closure_reg = Some(d);
        Ok(())
    }

    // --- Non-local return (§10) ---

    fn do_nlr(&mut self, regs: &mut Regs, a: u8) -> Result<(), VmError> {
        self.counters.nlrs += 1;
        let value = self.get(regs, a);
        let closure = self.get(regs, 0); // block activation receiver
        let ca = closure.as_ptr();
        let nil = self.nil();
        let hproc = self.heap.slot(ca, CLOSURE_HOME_PROCESS);
        let hoff = self.heap.slot(ca, CLOSURE_HOME_OFFSET);
        let hser = self.heap.slot(ca, CLOSURE_HOME_SERIAL);

        let alive = hproc == self.active_process
            && hproc != nil
            && self.heap.slot(hproc.as_ptr(), PROCESS_STACK) != nil
            && hoff.is_int()
            && (hoff.as_int() as usize) <= regs.frame
            && {
                let flags = self
                    .heap
                    .slot(regs.stack, hoff.as_int() as usize + FRAME_FLAGS);
                flags.is_int() && (flags.as_int() >> SERIAL_SHIFT) == hser.as_int()
            };
        if !alive {
            // BlockCannotReturn — in-image via a cannotReturn: send. The
            // block activation is abandoned: the send replaces it, so the
            // handler's result becomes the block's return value (there is
            // no instruction after NLR to resume at).
            let sel = self.intern("cannotReturn:");
            let caller = self
                .heap
                .slot(regs.stack, regs.frame + FRAME_CALLER)
                .as_int();
            if caller == 0 {
                return fatal("NLR with dead home from a base frame");
            }
            let ri = self
                .heap
                .slot(regs.stack, regs.frame + FRAME_RETINFO)
                .as_int();
            let dest = (ri & ((1 << RETINFO_DEST_BITS) - 1)) as u8;
            self.nil_frame(regs);
            regs.frame = caller as usize;
            regs.pc = (ri >> RETINFO_PC_SHIFT) as usize;
            self.reload_code(regs);
            self.send_staged(regs, dest, sel, &[closure, value])?;
            return Ok(());
        }
        self.unwind_and_return_from(regs, hoff.as_int() as usize, value)
    }

    /// The shared unwinder (§11): pop frames from the top down to `target`,
    /// then return `value` from `target` to its caller as if it executed
    /// RET. Ensure-marked frames intercept the unwind (exceptions phase).
    pub fn unwind_and_return_from(
        &mut self,
        regs: &mut Regs,
        target: usize,
        value: Value,
    ) -> Result<(), VmError> {
        self.counters.unwind_runs += 1;
        loop {
            if regs.frame == target {
                return self.do_return(regs, value);
            }
            if regs.frame < target {
                return fatal("unwind target not on the frame chain");
            }
            let flags = self
                .heap
                .slot(regs.stack, regs.frame + FRAME_FLAGS)
                .as_int();
            if flags & (FLAG_ENSURE as i64) != 0 {
                let hsb = self.method_handler_slot_base(regs.method);
                let blockv = self.get(regs, (hsb + ENSURE_SLOT_BLOCK) as u8);
                if blockv.is_ptr()
                    && self.heap.header(blockv.as_ptr()).class_index() == CLASS_BLOCKCLOSURE
                {
                    return self.run_ensure_block_then_continue(regs, target, value, hsb, blockv);
                }
                // Block already ran (slot nil'd) or never installed: pop
                // this frame like any other.
            }
            let caller = self
                .heap
                .slot(regs.stack, regs.frame + FRAME_CALLER)
                .as_int();
            if caller == 0 {
                return fatal("unwind past base frame");
            }
            self.nil_frame(regs);
            regs.frame = caller as usize;
            self.reload_code(regs);
        }
    }

    /// §11's ensure interception: instead of popping past an ensure frame,
    /// stash the pending unwind (target, serial, value) in the ensure
    /// frame's reserved slots, mark its block as consumed, and activate the
    /// block on top of it with the unwind-continuation flag. When that
    /// activation returns, `continue_unwind_after_ensure` resumes the
    /// unwind. Unwinding state lives entirely in frames — re-entrant and
    /// abandonable (an NLR out of the ensure block simply pops the marker).
    fn run_ensure_block_then_continue(
        &mut self,
        regs: &mut Regs,
        target: usize,
        value: Value,
        hsb: usize,
        blockv: Value,
    ) -> Result<(), VmError> {
        self.counters.ensure_interceptions += 1;
        let nil = self.nil();
        let target_serial = self
            .heap
            .slot(regs.stack, target + FRAME_FLAGS)
            .as_int()
            >> SERIAL_SHIFT;
        self.put(regs, (hsb + ENSURE_SLOT_BLOCK) as u8, nil);
        self.put(
            regs,
            (hsb + ENSURE_SLOT_PENDING_TARGET) as u8,
            Value::from_int(target as i64),
        );
        self.put(
            regs,
            (hsb + ENSURE_SLOT_PENDING_SERIAL) as u8,
            Value::from_int(target_serial),
        );
        self.put(regs, (hsb + ENSURE_SLOT_PENDING_VALUE) as u8, value);

        let block = self.heap.slot(blockv.as_ptr(), CLOSURE_COMPILED_BLOCK);
        if self.method_argc(block) != 0 {
            return fatal("ensure block must take no arguments");
        }
        // Stage the closure above the ensure frame and activate it (full
        // control-word gap above the live slots). The dest slot is
        // irrelevant: the marker return path ignores it.
        let fs = self.method_frame_slots(regs.method);
        let r = fs + FRAME_RECEIVER;
        let needed = regs.frame + FRAME_RECEIVER + r + 1;
        if needed > self.heap.num_slots(regs.stack) as usize {
            self.grow_stack(regs, needed)?;
        }
        self.put(regs, r as u8, blockv);
        self.push_frame(
            regs,
            block,
            r as u8,
            0,
            0,
            None,
            FLAG_BLOCKCTX | FLAG_UNWINDCONT,
        )
    }

    /// RET from an unwind-continuation frame: the caller is the ensure
    /// frame holding the pending unwind; resume it.
    fn continue_unwind_after_ensure(
        &mut self,
        regs: &mut Regs,
        _ensure_block_result: Value,
    ) -> Result<(), VmError> {
        let e_off = self
            .heap
            .slot(regs.stack, regs.frame + FRAME_CALLER)
            .as_int() as usize;
        self.nil_frame(regs);
        regs.frame = e_off;
        self.reload_code(regs);
        let hsb = self.method_handler_slot_base(regs.method);
        let target = self.get(regs, (hsb + ENSURE_SLOT_PENDING_TARGET) as u8);
        let serial = self.get(regs, (hsb + ENSURE_SLOT_PENDING_SERIAL) as u8);
        let value = self.get(regs, (hsb + ENSURE_SLOT_PENDING_VALUE) as u8);
        if !target.is_int() {
            return fatal("corrupt pending unwind in ensure frame");
        }
        let t = target.as_int() as usize;
        let live = t <= regs.frame && {
            let flags = self.heap.slot(regs.stack, t + FRAME_FLAGS);
            flags.is_int() && (flags.as_int() >> SERIAL_SHIFT) == serial.as_int()
        };
        if !live {
            return fatal("unwind target died while ensure block ran");
        }
        self.unwind_and_return_from(regs, t, value)
    }

}

impl Vm {
    /// Corpus snapshot mode: dump the image mid-run and keep going (§20
    /// phase 4 — the uninterrupted run doubles as the reference output).
    fn snapshot_now(&mut self, regs: &mut Regs, path: &str) -> Result<(), VmError> {
        self.save_regs(regs);
        let saved = self.tenure_threshold;
        self.tenure_threshold = 0;
        self.collect_young()?;
        self.collect_young()?;
        self.tenure_threshold = saved;
        self.refresh_regs(regs);
        self.write_image(path)?;
        self.snapshot_fired_at_capture_len = Some(self.stdout_capture.len());
        Ok(())
    }

    /// Run until no process is runnable (image semantics: any process's
    /// base-frame return terminates it; the scheduler picks the next; a
    /// fully idle system exits cleanly).
    pub fn run_until_idle(&mut self, process: Value) -> Result<(), VmError> {
        let mut current = process;
        loop {
            self.run(current)?;
            match self.take_next_runnable() {
                Some(next) => current = next,
                None => {
                    if self.timer_requests.is_empty() {
                        return Ok(());
                    }
                    match self.pick_next_or_wait() {
                        Ok(next) => current = next,
                        Err(_) => return Ok(()),
                    }
                }
            }
        }
    }
}

/// Floored division and modulo (Smalltalk // and \\).
pub fn floored_divmod(x: i64, y: i64) -> (i64, i64) {
    let q = x / y;
    let r = x % y;
    if r != 0 && ((r < 0) != (y < 0)) {
        (q - 1, r + y)
    } else {
        (q, r)
    }
}
