//! Numbered primitives (SPEC §16). Convention: a primitive either fully
//! succeeds (returns a value or transfers control) or fails cleanly with a
//! code, in which case the method's Smalltalk fallback body runs with the
//! code visible in the reserved frame slot after the arguments.

use crate::interp::{PrimOutcome, Regs};
use crate::treaty::*;
use crate::value::Value;
use crate::vm::{Vm, VmError};

// Failure codes (VM-internal, readable by fallback code).
pub const FAIL_UNKNOWN_PRIM: i64 = 1;
pub const FAIL_WRONG_TYPE: i64 = 2;
pub const FAIL_WRONG_ARGC: i64 = 3;
pub const FAIL_BAD_INDEX: i64 = 4;
pub const FAIL_IMMUTABLE: i64 = 5;
pub const FAIL_OVERFLOW: i64 = 6;
pub const FAIL_UNSUPPORTED_CONTEXT: i64 = 7;
pub const FAIL_NOT_FOUND: i64 = 8;

impl Vm {
    /// Dispatch primitive `n`. Receiver at bytecode slot `r`, args at
    /// `r+1..r+argc`; a successful non-control primitive's value goes to
    /// `dest` (handled by the caller); control primitives push/switch
    /// frames themselves using `r`/`dest`.
    pub fn run_primitive(
        &mut self,
        n: u16,
        regs: &mut Regs,
        r: u8,
        argc: usize,
        dest: u8,
    ) -> Result<PrimOutcome, VmError> {
        match n {
            PRIM_BLOCK_VALUE_0 | PRIM_BLOCK_VALUE_1 | PRIM_BLOCK_VALUE_2
            | PRIM_BLOCK_VALUE_3 | PRIM_BLOCK_VALUE_4 => {
                let want = (n - PRIM_BLOCK_VALUE_0) as usize;
                self.prim_block_value(regs, r, argc, dest, want)
            }
            PRIM_BLOCK_VALUE_ARGS => self.prim_block_value_args(regs, r, dest),
            PRIM_TRANSFER_TO => self.prim_transfer_to(regs, r),
            PRIM_SEMAPHORE_WAIT => self.prim_semaphore_wait(regs, r),
            PRIM_SEMAPHORE_SIGNAL => self.prim_semaphore_signal(regs, r),
            PRIM_YIELD => self.prim_yield(regs, r),
            PRIM_PROCESS_RESUME => self.prim_process_resume(regs, r),
            PRIM_PROCESS_SUSPEND => self.prim_process_suspend(regs, r),
            PRIM_PROCESS_TERMINATE => self.prim_process_terminate(regs, r),
            PRIM_SIGNAL_AT_MS => {
                let sem = self.get(regs, r + 1);
                let ms = self.get(regs, r + 2);
                if !sem.is_ptr()
                    || self.heap.header(sem.as_ptr()).class_index() != CLASS_SEMAPHORE
                    || !ms.is_int()
                {
                    return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
                }
                self.timer_requests.push((ms.as_int(), sem));
                self.safepoint_flag = true;
                Ok(PrimOutcome::Value(self.get(regs, r)))
            }
            PRIM_FIND_HANDLER => self.prim_find_handler(regs, r),
            PRIM_UNWIND_TO => self.prim_unwind_to(regs, r),
            PRIM_HANDLER_INFO => self.prim_handler_info(regs, r),
            PRIM_SET_HANDLER_STATE => self.prim_set_handler_state(regs, r),
            PRIM_SIGNAL_CONTEXT => self.prim_signal_context(regs),

            PRIM_CLASS => Ok(PrimOutcome::Value(self.class_of(self.get(regs, r)))),
            PRIM_IDENTITY_HASH => {
                let recv = self.get(regs, r);
                let h = self.identity_hash_of(recv);
                Ok(PrimOutcome::Value(Value::from_int(h)))
            }
            PRIM_IDENTICAL => {
                let eq = self.get(regs, r) == self.get(regs, r + 1);
                Ok(PrimOutcome::Value(self.bool_v(eq)))
            }
            PRIM_NEW => self.prim_new(regs, r),
            PRIM_NEW_SIZED => self.prim_new_sized(regs, r),
            PRIM_AT => {
                let recv = self.get(regs, r);
                let idx = self.get(regs, r + 1);
                match self.at_value(recv, idx) {
                    Some(v) => Ok(PrimOutcome::Value(v)),
                    None => Ok(PrimOutcome::Fail(FAIL_BAD_INDEX)),
                }
            }
            PRIM_AT_PUT => {
                let recv = self.get(regs, r);
                let idx = self.get(regs, r + 1);
                let val = self.get(regs, r + 2);
                match self.at_put_value(recv, idx, val) {
                    Some(v) => Ok(PrimOutcome::Value(v)),
                    None => Ok(PrimOutcome::Fail(FAIL_BAD_INDEX)),
                }
            }
            PRIM_SIZE => {
                let recv = self.get(regs, r);
                match self.size_value(recv) {
                    Some(v) => Ok(PrimOutcome::Value(v)),
                    None => Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE)),
                }
            }
            PRIM_INST_VAR_AT => {
                let recv = self.get(regs, r);
                let idx = self.get(regs, r + 1);
                if !recv.is_ptr() || !idx.is_int() {
                    return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
                }
                let i = idx.as_int();
                let n = self.heap.num_slots(recv.as_ptr()) as i64;
                if i < 1 || i > n || self.heap.header(recv.as_ptr()).is_bytes() {
                    return Ok(PrimOutcome::Fail(FAIL_BAD_INDEX));
                }
                Ok(PrimOutcome::Value(self.heap.slot(recv.as_ptr(), (i - 1) as usize)))
            }
            PRIM_INST_VAR_AT_PUT => {
                let recv = self.get(regs, r);
                let idx = self.get(regs, r + 1);
                let val = self.get(regs, r + 2);
                if !recv.is_ptr() || !idx.is_int() {
                    return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
                }
                let i = idx.as_int();
                let n = self.heap.num_slots(recv.as_ptr()) as i64;
                if i < 1 || i > n || self.heap.header(recv.as_ptr()).is_bytes() {
                    return Ok(PrimOutcome::Fail(FAIL_BAD_INDEX));
                }
                self.store_slot(recv.as_ptr(), (i - 1) as usize, val);
                Ok(PrimOutcome::Value(val))
            }
            PRIM_PERFORM_WITH_ARGS => self.prim_perform(regs, r, dest),

            PRIM_INT_ADD | PRIM_INT_SUB | PRIM_INT_MUL | PRIM_INT_DIV | PRIM_INT_MOD
            | PRIM_INT_QUO | PRIM_INT_BIT_AND | PRIM_INT_BIT_OR | PRIM_INT_BIT_XOR
            | PRIM_INT_BIT_SHIFT => self.prim_int_binary(n, regs, r),
            PRIM_INT_LT | PRIM_INT_GT | PRIM_INT_LE | PRIM_INT_GE | PRIM_INT_EQ => {
                let x = self.get(regs, r);
                let y = self.get(regs, r + 1);
                if !x.is_int() || !y.is_int() {
                    return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
                }
                let (a, b) = (x.as_int(), y.as_int());
                let res = match n {
                    PRIM_INT_LT => a < b,
                    PRIM_INT_GT => a > b,
                    PRIM_INT_LE => a <= b,
                    PRIM_INT_GE => a >= b,
                    _ => a == b,
                };
                Ok(PrimOutcome::Value(self.bool_v(res)))
            }
            PRIM_INT_AS_FLOAT => {
                let x = self.get(regs, r);
                if !x.is_int() {
                    return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
                }
                let f = x.as_int() as f64;
                let obj = self.alloc_gc(regs, CLASS_FLOAT, FMT_BYTES_BASE, 8)?;
                self.heap.write_bytes(obj.as_ptr(), &f.to_le_bytes());
                Ok(PrimOutcome::Value(obj))
            }

            PRIM_FLOAT_ADD | PRIM_FLOAT_SUB | PRIM_FLOAT_MUL | PRIM_FLOAT_DIV => {
                let (x, y) = match self.two_floats(regs, r) {
                    Some(p) => p,
                    None => return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE)),
                };
                let f = match n {
                    PRIM_FLOAT_ADD => x + y,
                    PRIM_FLOAT_SUB => x - y,
                    PRIM_FLOAT_MUL => x * y,
                    _ => x / y,
                };
                let obj = self.alloc_gc(regs, CLASS_FLOAT, FMT_BYTES_BASE, 8)?;
                self.heap.write_bytes(obj.as_ptr(), &f.to_le_bytes());
                Ok(PrimOutcome::Value(obj))
            }
            PRIM_FLOAT_LT | PRIM_FLOAT_GT | PRIM_FLOAT_LE | PRIM_FLOAT_GE | PRIM_FLOAT_EQ => {
                let (x, y) = match self.two_floats(regs, r) {
                    Some(p) => p,
                    None => return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE)),
                };
                let res = match n {
                    PRIM_FLOAT_LT => x < y,
                    PRIM_FLOAT_GT => x > y,
                    PRIM_FLOAT_LE => x <= y,
                    PRIM_FLOAT_GE => x >= y,
                    _ => x == y,
                };
                Ok(PrimOutcome::Value(self.bool_v(res)))
            }
            PRIM_FLOAT_TRUNCATED => {
                let x = match self.one_float(regs, r) {
                    Some(f) => f,
                    None => return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE)),
                };
                let t = x.trunc();
                if !t.is_finite()
                    || t > Value::SMALLINT_MAX as f64
                    || t < Value::SMALLINT_MIN as f64
                {
                    return Ok(PrimOutcome::Fail(FAIL_OVERFLOW));
                }
                Ok(PrimOutcome::Value(Value::from_int(t as i64)))
            }
            PRIM_FLOAT_SQRT => {
                let x = match self.one_float(regs, r) {
                    Some(f) => f,
                    None => return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE)),
                };
                let f = x.sqrt();
                let obj = self.alloc_gc(regs, CLASS_FLOAT, FMT_BYTES_BASE, 8)?;
                self.heap.write_bytes(obj.as_ptr(), &f.to_le_bytes());
                Ok(PrimOutcome::Value(obj))
            }

            PRIM_FILE_OPEN => self.prim_file_open(regs, r),
            PRIM_FILE_CLOSE => self.prim_file_close(regs, r),
            PRIM_FILE_READ => self.prim_file_read(regs, r),
            PRIM_FILE_WRITE => self.prim_file_write(regs, r),
            PRIM_FILE_POSITION | PRIM_FILE_SET_POSITION | PRIM_FILE_SIZE => {
                self.prim_file_seek_family(n, regs, r)
            }
            PRIM_FILE_DELETE => {
                let path = self.get(regs, r + 1);
                if !path.is_ptr() || !self.heap.header(path.as_ptr()).is_bytes() {
                    return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
                }
                let p = String::from_utf8_lossy(self.heap.bytes(path.as_ptr())).into_owned();
                match std::fs::remove_file(&p) {
                    Ok(()) => Ok(PrimOutcome::Value(self.nil())),
                    Err(_) => Ok(PrimOutcome::Fail(FAIL_NOT_FOUND)),
                }
            }
            PRIM_STDIO_WRITE => self.prim_stdio_write(regs, r),
            PRIM_STDIO_READ => {
                use std::io::Read;
                let count = self.get(regs, r + 1);
                if !count.is_int() || count.as_int() < 0 {
                    return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
                }
                let mut buf = vec![0u8; count.as_int() as usize];
                let n = std::io::stdin().read(&mut buf).unwrap_or(0);
                let obj = self.alloc_gc(regs, CLASS_BYTEARRAY, FMT_BYTES_BASE, n)?;
                self.heap.write_bytes(obj.as_ptr(), &buf[..n]);
                Ok(PrimOutcome::Value(obj))
            }
            // primNextEvent: the v1 host has no event sources; the queue is
            // always empty (§16 — the UI builds on this later).
            PRIM_NEXT_EVENT => Ok(PrimOutcome::Value(self.nil())),
            PRIM_CLOCK_MONOTONIC_MS => Ok(PrimOutcome::Value(Value::from_int(
                self.start_instant.elapsed().as_millis() as i64,
            ))),
            PRIM_CLOCK_WALL_MS => {
                let ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                Ok(PrimOutcome::Value(Value::from_int(ms)))
            }

            PRIM_REGISTER_CLASS => {
                let class = self.get(regs, r);
                if !class.is_ptr() {
                    return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
                }
                let idx = self.register_class(class);
                Ok(PrimOutcome::Value(Value::from_int(idx as i64)))
            }
            PRIM_METHOD_INSTALL => {
                let class = self.get(regs, r);
                let selector = self.get(regs, r + 1);
                let method = self.get(regs, r + 2);
                if !class.is_ptr() || !selector.is_ptr() || !method.is_ptr() {
                    return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
                }
                // §12: the specialized-send methods on SmallInteger are
                // sealed — the VM ignores overriding installs (the loader
                // path in Rust is exempt; it is how they get installed).
                if class == self.class_table_at(CLASS_SMALLINTEGER) {
                    let sels = self.specials()[SPECIAL_SPECIALIZED_SELECTORS];
                    let n = self.heap.num_slots(sels.as_ptr()) as usize;
                    if (0..n).any(|i| self.heap.slot(sels.as_ptr(), i) == selector) {
                        return Ok(PrimOutcome::Value(method));
                    }
                }
                self.install_method(class, selector, method);
                Ok(PrimOutcome::Value(method))
            }
            PRIM_SNAPSHOT => {
                let Some(path) = self.path_arg(regs, r + 1) else {
                    return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
                };
                // §17: the saved image sees `true` as the snapshot's result
                // (write it into the dest slot before dumping); the
                // original run answers `false` (the normal Value outcome
                // overwrites dest afterwards).
                self.save_regs(regs);
                let t = self.true_v();
                self.put(regs, dest, t);
                // Empty young space: the image is old-space-only.
                let saved_threshold = self.tenure_threshold;
                self.tenure_threshold = 0;
                self.collect_young()?;
                self.collect_young()?;
                self.tenure_threshold = saved_threshold;
                self.refresh_regs(regs);
                match self.write_image(&path) {
                    Ok(()) => Ok(PrimOutcome::Value(self.false_v())),
                    Err(_) => Ok(PrimOutcome::Fail(FAIL_NOT_FOUND)),
                }
            }
            PRIM_FLUSH_CACHES => {
                self.flush_caches();
                Ok(PrimOutcome::Value(self.get(regs, r)))
            }
            PRIM_FRAME_INFO => self.prim_frame_info(regs, r),

            _ => Ok(PrimOutcome::Fail(FAIL_UNKNOWN_PRIM)),
        }
    }

    // --- Object essentials ---

    fn prim_new(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let class = self.get(regs, r);
        if !class.is_ptr()
            || (self.heap.num_slots(class.as_ptr()) as usize) < BEHAVIOR_NUM_VM_SLOTS
        {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        let (format, nslots) = self.class_format_and_slots(class);
        if format != FMT_FIXED {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        let idx = self.heap.slot(class.as_ptr(), BEHAVIOR_CLASS_INDEX).as_int() as u32;
        let obj = self.alloc_gc(regs, idx, FMT_FIXED, nslots)?;
        Ok(PrimOutcome::Value(obj))
    }

    fn prim_new_sized(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let class = self.get(regs, r);
        let size = self.get(regs, r + 1);
        if !class.is_ptr()
            || !size.is_int()
            || size.as_int() < 0
            || (self.heap.num_slots(class.as_ptr()) as usize) < BEHAVIOR_NUM_VM_SLOTS
        {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        let (format, _) = self.class_format_and_slots(class);
        let idx = self.heap.slot(class.as_ptr(), BEHAVIOR_CLASS_INDEX).as_int() as u32;
        let n = size.as_int() as usize;
        let obj = match format {
            FMT_PTRS => self.alloc_gc(regs, idx, FMT_PTRS, n)?,
            f if f >= FMT_BYTES_BASE => self.alloc_gc(regs, idx, FMT_BYTES_BASE, n)?,
            _ => return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE)),
        };
        Ok(PrimOutcome::Value(obj))
    }

    /// Generic 1-indexed at: for pointer- and byte-format objects.
    pub fn at_value(&self, recv: Value, idx: Value) -> Option<Value> {
        if !recv.is_ptr() || !idx.is_int() {
            return None;
        }
        let addr = recv.as_ptr();
        let h = self.heap.header(addr);
        let i = idx.as_int();
        if h.format() == FMT_PTRS {
            let n = self.heap.num_slots(addr) as i64;
            (i >= 1 && i <= n).then(|| self.heap.slot(addr, (i - 1) as usize))
        } else if h.is_bytes() {
            let n = self.heap.byte_size(addr) as i64;
            (i >= 1 && i <= n)
                .then(|| Value::from_int(self.heap.byte(addr, (i - 1) as usize) as i64))
        } else {
            None
        }
    }

    pub fn at_put_value(&mut self, recv: Value, idx: Value, val: Value) -> Option<Value> {
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
            if i >= 1 && i <= n && val.is_int() && (0..=255).contains(&val.as_int()) {
                self.heap.set_byte(addr, (i - 1) as usize, val.as_int() as u8);
                return Some(val);
            }
        }
        None
    }

    pub fn size_value(&self, recv: Value) -> Option<Value> {
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

    /// perform: selector withArguments: array — re-dispatch: spread the
    /// arguments over the staging slots after the receiver and send.
    fn prim_perform(&mut self, regs: &mut Regs, r: u8, dest: u8) -> Result<PrimOutcome, VmError> {
        let selector = self.get(regs, r + 1);
        let args = self.get(regs, r + 2);
        if !selector.is_ptr()
            || !args.is_ptr()
            || self.heap.header(args.as_ptr()).format() != FMT_PTRS
        {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        let n = self.heap.num_slots(args.as_ptr()) as usize;
        if r as usize + 1 + n > 250 {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_ARGC));
        }
        let needed = regs.frame + FRAME_RECEIVER + r as usize + 1 + n;
        if needed > self.heap.num_slots(regs.stack) as usize {
            self.grow_stack(regs, needed)?;
        }
        for i in 0..n {
            let v = self.heap.slot(args.as_ptr(), i);
            self.put(regs, r + 1 + i as u8, v);
        }
        self.do_send(regs, dest, r, selector, n, None, None)?;
        Ok(PrimOutcome::Control)
    }

    /// valueWithArguments: — spread the array, then activate like value:.
    fn prim_block_value_args(
        &mut self,
        regs: &mut Regs,
        r: u8,
        dest: u8,
    ) -> Result<PrimOutcome, VmError> {
        if (r as usize) < FRAME_RECEIVER {
            return Ok(PrimOutcome::Fail(FAIL_UNSUPPORTED_CONTEXT));
        }
        let closure = self.get(regs, r);
        let args = self.get(regs, r + 1);
        if !closure.is_ptr()
            || self.heap.header(closure.as_ptr()).class_index() != CLASS_BLOCKCLOSURE
            || !args.is_ptr()
            || self.heap.header(args.as_ptr()).format() != FMT_PTRS
        {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        let block = self.heap.slot(closure.as_ptr(), CLOSURE_COMPILED_BLOCK);
        let n = self.heap.num_slots(args.as_ptr()) as usize;
        if self.method_argc(block) != n {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_ARGC));
        }
        let needed = regs.frame + FRAME_RECEIVER + r as usize + 1 + n;
        if needed > self.heap.num_slots(regs.stack) as usize {
            self.grow_stack(regs, needed)?;
        }
        for i in 0..n {
            let v = self.heap.slot(args.as_ptr(), i);
            self.put(regs, r + 1 + i as u8, v);
        }
        self.push_frame(regs, block, r, dest, n, None, FLAG_BLOCKCTX)?;
        Ok(PrimOutcome::Control)
    }

    // --- SmallInteger arithmetic ---

    fn prim_int_binary(&mut self, n: u16, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let x = self.get(regs, r);
        let y = self.get(regs, r + 1);
        if !x.is_int() || !y.is_int() {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        let (a, b) = (x.as_int(), y.as_int());
        let res: Option<i64> = match n {
            PRIM_INT_ADD => a.checked_add(b),
            PRIM_INT_SUB => a.checked_sub(b),
            PRIM_INT_MUL => a.checked_mul(b),
            PRIM_INT_DIV => {
                if b == 0 {
                    None
                } else {
                    Some(crate::interp::floored_divmod(a, b).0)
                }
            }
            PRIM_INT_MOD => {
                if b == 0 {
                    None
                } else {
                    Some(crate::interp::floored_divmod(a, b).1)
                }
            }
            PRIM_INT_QUO => {
                if b == 0 {
                    None
                } else {
                    Some(a / b)
                }
            }
            PRIM_INT_BIT_AND => Some(a & b),
            PRIM_INT_BIT_OR => Some(a | b),
            PRIM_INT_BIT_XOR => Some(a ^ b),
            PRIM_INT_BIT_SHIFT => {
                if b >= 0 {
                    if b >= 63 {
                        None
                    } else {
                        let shifted = a.checked_shl(b as u32);
                        // A left shift that loses bits must fail.
                        shifted.filter(|s| (s >> b) == a)
                    }
                } else if -b >= 63 {
                    Some(if a < 0 { -1 } else { 0 })
                } else {
                    Some(a >> (-b))
                }
            }
            _ => unreachable!(),
        };
        match res.and_then(Value::try_from_int) {
            Some(v) => Ok(PrimOutcome::Value(v)),
            None => Ok(PrimOutcome::Fail(FAIL_OVERFLOW)),
        }
    }

    // --- Float helpers ---

    fn float_of(&self, v: Value) -> Option<f64> {
        if v.is_ptr() && self.heap.header(v.as_ptr()).class_index() == CLASS_FLOAT {
            Some(self.float_value(v))
        } else {
            None
        }
    }

    fn one_float(&self, regs: &Regs, r: u8) -> Option<f64> {
        self.float_of(self.get(regs, r))
    }

    fn two_floats(&self, regs: &Regs, r: u8) -> Option<(f64, f64)> {
        Some((
            self.float_of(self.get(regs, r))?,
            self.float_of(self.get(regs, r + 1))?,
        ))
    }

    // --- Files and stdio ---

    fn path_arg(&self, regs: &Regs, k: u8) -> Option<String> {
        let v = self.get(regs, k);
        if v.is_ptr() && self.heap.header(v.as_ptr()).is_bytes() {
            Some(String::from_utf8_lossy(self.heap.bytes(v.as_ptr())).into_owned())
        } else {
            None
        }
    }

    fn fd_arg(&self, regs: &Regs, k: u8) -> Option<usize> {
        let v = self.get(regs, k);
        if v.is_int() && v.as_int() >= 0 && (v.as_int() as usize) < self.files.len() {
            let fd = v.as_int() as usize;
            self.files[fd].as_ref()?;
            Some(fd)
        } else {
            None
        }
    }

    /// fopen: path mode: m — 0 read, 1 write (create/truncate), 2
    /// read-write (create), 3 append (create). Answers the fd.
    fn prim_file_open(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let Some(path) = self.path_arg(regs, r + 1) else {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        };
        let mode = self.get(regs, r + 2);
        if !mode.is_int() {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        use std::fs::OpenOptions;
        let file = match mode.as_int() {
            0 => OpenOptions::new().read(true).open(&path),
            1 => OpenOptions::new().write(true).create(true).truncate(true).open(&path),
            2 => OpenOptions::new().read(true).write(true).create(true).open(&path),
            3 => OpenOptions::new().append(true).create(true).open(&path),
            _ => return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE)),
        };
        match file {
            Ok(f) => {
                let fd = if let Some(free) = self.files.iter().position(Option::is_none) {
                    self.files[free] = Some(f);
                    free
                } else {
                    self.files.push(Some(f));
                    self.files.len() - 1
                };
                Ok(PrimOutcome::Value(Value::from_int(fd as i64)))
            }
            Err(_) => Ok(PrimOutcome::Fail(FAIL_NOT_FOUND)),
        }
    }

    fn prim_file_close(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let Some(fd) = self.fd_arg(regs, r + 1) else {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        };
        self.files[fd] = None;
        Ok(PrimOutcome::Value(self.nil()))
    }

    fn prim_file_read(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        use std::io::Read;
        let Some(fd) = self.fd_arg(regs, r + 1) else {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        };
        let count = self.get(regs, r + 2);
        if !count.is_int() || count.as_int() < 0 {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        let mut buf = vec![0u8; count.as_int() as usize];
        let n = match self.files[fd].as_mut().unwrap().read(&mut buf) {
            Ok(n) => n,
            Err(_) => return Ok(PrimOutcome::Fail(FAIL_NOT_FOUND)),
        };
        let obj = self.alloc_gc(regs, CLASS_BYTEARRAY, FMT_BYTES_BASE, n)?;
        self.heap.write_bytes(obj.as_ptr(), &buf[..n]);
        Ok(PrimOutcome::Value(obj))
    }

    fn prim_file_write(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        use std::io::Write;
        let Some(fd) = self.fd_arg(regs, r + 1) else {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        };
        let bytes = self.get(regs, r + 2);
        if !bytes.is_ptr() || !self.heap.header(bytes.as_ptr()).is_bytes() {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        let data = self.heap.bytes(bytes.as_ptr()).to_vec();
        match self.files[fd].as_mut().unwrap().write_all(&data) {
            Ok(()) => Ok(PrimOutcome::Value(Value::from_int(data.len() as i64))),
            Err(_) => Ok(PrimOutcome::Fail(FAIL_NOT_FOUND)),
        }
    }

    fn prim_file_seek_family(
        &mut self,
        n: u16,
        regs: &mut Regs,
        r: u8,
    ) -> Result<PrimOutcome, VmError> {
        use std::io::{Seek, SeekFrom};
        let Some(fd) = self.fd_arg(regs, r + 1) else {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        };
        let file = self.files[fd].as_mut().unwrap();
        let result = match n {
            PRIM_FILE_POSITION => file.stream_position().map(|p| p as i64),
            PRIM_FILE_SET_POSITION => {
                let pos = self.get(regs, r + 2);
                if !pos.is_int() || pos.as_int() < 0 {
                    return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
                }
                let file = self.files[fd].as_mut().unwrap();
                file.seek(SeekFrom::Start(pos.as_int() as u64)).map(|p| p as i64)
            }
            _ => self.files[fd].as_ref().unwrap().metadata().map(|m| m.len() as i64),
        };
        match result {
            Ok(v) => Ok(PrimOutcome::Value(Value::from_int(v))),
            Err(_) => Ok(PrimOutcome::Fail(FAIL_NOT_FOUND)),
        }
    }

    /// stdioWrite: bytes on: fd (1 = stdout, 2 = stderr). Output is also
    /// captured in `stdout_capture` for tests and the corpus runner.
    fn prim_stdio_write(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        use std::io::Write;
        let bytes = self.get(regs, r + 1);
        let fd = self.get(regs, r + 2);
        if !bytes.is_ptr() || !self.heap.header(bytes.as_ptr()).is_bytes() || !fd.is_int() {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        let data = self.heap.bytes(bytes.as_ptr()).to_vec();
        let ok = match fd.as_int() {
            1 => {
                self.stdout_capture.extend_from_slice(&data);
                std::io::stdout().write_all(&data).is_ok()
            }
            2 => std::io::stderr().write_all(&data).is_ok(),
            _ => return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE)),
        };
        if ok {
            Ok(PrimOutcome::Value(Value::from_int(data.len() as i64)))
        } else {
            Ok(PrimOutcome::Fail(FAIL_NOT_FOUND))
        }
    }

    // --- Processes, semaphores, scheduler (§13) ---

    fn scheduler_queues(&self) -> Value {
        let sched = self.specials()[SPECIAL_PROCESSOR];
        self.heap.slot(sched.as_ptr(), SCHEDULER_QUEUES)
    }

    fn process_priority(&self, p: Value) -> i64 {
        self.heap.slot(p.as_ptr(), PROCESS_PRIORITY).as_int()
    }

    fn is_process(&self, v: Value) -> bool {
        v.is_ptr() && self.heap.header(v.as_ptr()).class_index() == CLASS_PROCESS
    }

    /// Append a process to a queue (run queue or semaphore) linked via
    /// nextLink/myList.
    fn enqueue_process(&mut self, list: Value, head_slot: usize, tail_slot: usize, p: Value) {
        let nil = self.nil();
        let la = list.as_ptr();
        let pa = p.as_ptr();
        self.store_slot(pa, PROCESS_NEXT_LINK, nil);
        self.store_slot(pa, PROCESS_MY_LIST, list);
        let head = self.heap.slot(la, head_slot);
        if head == nil {
            self.store_slot(la, head_slot, p);
        } else {
            let tail = self.heap.slot(la, tail_slot);
            self.store_slot(tail.as_ptr(), PROCESS_NEXT_LINK, p);
        }
        self.store_slot(la, tail_slot, p);
    }

    fn dequeue_process(&mut self, list: Value, head_slot: usize, tail_slot: usize) -> Option<Value> {
        let nil = self.nil();
        let la = list.as_ptr();
        let head = self.heap.slot(la, head_slot);
        if head == nil {
            return None;
        }
        let next = self.heap.slot(head.as_ptr(), PROCESS_NEXT_LINK);
        self.store_slot(la, head_slot, next);
        if next == nil {
            self.store_slot(la, tail_slot, nil);
        }
        self.store_slot(head.as_ptr(), PROCESS_NEXT_LINK, nil);
        self.store_slot(head.as_ptr(), PROCESS_MY_LIST, nil);
        Some(head)
    }

    fn queue_slots_of(&self, list: Value) -> (usize, usize) {
        if self.heap.header(list.as_ptr()).class_index() == CLASS_SEMAPHORE {
            (SEMAPHORE_QUEUE_HEAD, SEMAPHORE_QUEUE_TAIL)
        } else {
            (LIST_HEAD, LIST_TAIL)
        }
    }

    /// Unlink a process from whatever queue currently holds it.
    fn remove_process_from_its_list(&mut self, p: Value) {
        let nil = self.nil();
        let list = self.heap.slot(p.as_ptr(), PROCESS_MY_LIST);
        if list == nil {
            return;
        }
        let (hs, ts) = self.queue_slots_of(list);
        let la = list.as_ptr();
        let mut prev = nil;
        let mut cur = self.heap.slot(la, hs);
        while cur != nil {
            let next = self.heap.slot(cur.as_ptr(), PROCESS_NEXT_LINK);
            if cur == p {
                if prev == nil {
                    self.store_slot(la, hs, next);
                } else {
                    self.store_slot(prev.as_ptr(), PROCESS_NEXT_LINK, next);
                }
                if next == nil {
                    self.store_slot(la, ts, prev);
                }
                break;
            }
            prev = cur;
            cur = next;
        }
        self.store_slot(p.as_ptr(), PROCESS_NEXT_LINK, nil);
        self.store_slot(p.as_ptr(), PROCESS_MY_LIST, nil);
    }

    pub fn make_runnable(&mut self, p: Value) {
        let prio = self.process_priority(p).clamp(1, NUM_PRIORITIES as i64) as usize;
        let queues = self.scheduler_queues();
        let q = self.heap.slot(queues.as_ptr(), prio - 1);
        self.enqueue_process(q, LIST_HEAD, LIST_TAIL, p);
    }

    pub(crate) fn take_next_runnable(&mut self) -> Option<Value> {
        let queues = self.scheduler_queues();
        for prio in (0..NUM_PRIORITIES).rev() {
            let q = self.heap.slot(queues.as_ptr(), prio);
            if let Some(p) = self.dequeue_process(q, LIST_HEAD, LIST_TAIL) {
                return Some(p);
            }
        }
        None
    }

    pub(crate) fn runnable_priority_ceiling(&self) -> Option<i64> {
        let nil = self.nil();
        let queues = self.scheduler_queues();
        for prio in (0..NUM_PRIORITIES).rev() {
            let q = self.heap.slot(queues.as_ptr(), prio);
            if self.heap.slot(q.as_ptr(), LIST_HEAD) != nil {
                return Some(prio as i64 + 1);
            }
        }
        None
    }

    /// Wait until some process is runnable, servicing timers (§13: the VM
    /// never blocks the OS thread while Smalltalk processes are runnable —
    /// and when none are, only a due timer can unblock us).
    pub fn pick_next_or_wait(&mut self) -> Result<Value, VmError> {
        loop {
            if let Some(p) = self.take_next_runnable() {
                return Ok(p);
            }
            if self.timer_requests.is_empty() {
                return Err(VmError::Fatal(
                    "deadlock: no runnable process and no pending timers".into(),
                ));
            }
            if !self.service_timers() {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    /// Fire due timers; true if any semaphore was signaled.
    pub fn service_timers(&mut self) -> bool {
        let now = self.start_instant.elapsed().as_millis() as i64;
        let due: Vec<Value> = {
            let (fire, keep): (Vec<_>, Vec<_>) =
                std::mem::take(&mut self.timer_requests)
                    .into_iter()
                    .partition(|(t, _)| *t <= now);
            self.timer_requests = keep;
            fire.into_iter().map(|(_, s)| s).collect()
        };
        let fired = !due.is_empty();
        for sem in due {
            self.semaphore_signal_internal(sem);
        }
        fired
    }

    /// signal without preemption (timer/event side).
    pub fn semaphore_signal_internal(&mut self, sem: Value) {
        if let Some(p) = self.dequeue_process(sem, SEMAPHORE_QUEUE_HEAD, SEMAPHORE_QUEUE_TAIL) {
            self.make_runnable(p);
        } else {
            let n = self.heap.slot(sem.as_ptr(), SEMAPHORE_EXCESS_SIGNALS).as_int();
            self.heap
                .set_slot_raw(sem.as_ptr(), SEMAPHORE_EXCESS_SIGNALS, Value::from_int(n + 1));
        }
    }

    fn prim_transfer_to(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let nil = self.nil();
        let target = self.get(regs, r);
        if !self.is_process(target) || self.heap.slot(target.as_ptr(), PROCESS_STACK) == nil {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        self.transfer_to(regs, target);
        Ok(PrimOutcome::Control)
    }

    fn prim_semaphore_wait(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let sem = self.get(regs, r);
        if !sem.is_ptr() || self.heap.header(sem.as_ptr()).class_index() != CLASS_SEMAPHORE {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        let n = self.heap.slot(sem.as_ptr(), SEMAPHORE_EXCESS_SIGNALS).as_int();
        if n > 0 {
            self.heap
                .set_slot_raw(sem.as_ptr(), SEMAPHORE_EXCESS_SIGNALS, Value::from_int(n - 1));
            return Ok(PrimOutcome::Value(sem));
        }
        // Block: enqueue on the semaphore, run someone else.
        let cur = self.active_process;
        self.save_regs(regs);
        self.enqueue_process(sem, SEMAPHORE_QUEUE_HEAD, SEMAPHORE_QUEUE_TAIL, cur);
        let next = self.pick_next_or_wait()?;
        self.transfer_to(regs, next);
        Ok(PrimOutcome::Control)
    }

    fn prim_semaphore_signal(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let sem = self.get(regs, r);
        if !sem.is_ptr() || self.heap.header(sem.as_ptr()).class_index() != CLASS_SEMAPHORE {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        if let Some(p) = self.dequeue_process(sem, SEMAPHORE_QUEUE_HEAD, SEMAPHORE_QUEUE_TAIL) {
            let cur = self.active_process;
            if self.process_priority(p) > self.process_priority(cur) {
                // Preempt: current goes runnable, the waiter runs now.
                self.make_runnable(cur);
                self.transfer_to(regs, p);
                return Ok(PrimOutcome::Control);
            }
            self.make_runnable(p);
        } else {
            let n = self.heap.slot(sem.as_ptr(), SEMAPHORE_EXCESS_SIGNALS).as_int();
            self.heap
                .set_slot_raw(sem.as_ptr(), SEMAPHORE_EXCESS_SIGNALS, Value::from_int(n + 1));
        }
        Ok(PrimOutcome::Value(sem))
    }

    fn prim_yield(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let cur = self.active_process;
        self.make_runnable(cur);
        let next = self.pick_next_or_wait()?;
        if next == cur {
            let nil = self.nil();
            self.heap.set_slot_raw(cur.as_ptr(), PROCESS_MY_LIST, nil);
            return Ok(PrimOutcome::Value(self.get(regs, r)));
        }
        self.transfer_to(regs, next);
        Ok(PrimOutcome::Control)
    }

    fn prim_process_resume(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let nil = self.nil();
        let p = self.get(regs, r);
        if !self.is_process(p)
            || p == self.active_process
            || self.heap.slot(p.as_ptr(), PROCESS_STACK) == nil
            || self.heap.slot(p.as_ptr(), PROCESS_MY_LIST) != nil
        {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        let cur = self.active_process;
        if self.process_priority(p) > self.process_priority(cur) {
            self.make_runnable(cur);
            self.transfer_to(regs, p);
            return Ok(PrimOutcome::Control);
        }
        self.make_runnable(p);
        Ok(PrimOutcome::Value(p))
    }

    fn prim_process_suspend(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let p = self.get(regs, r);
        if !self.is_process(p) {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        if p == self.active_process {
            self.save_regs(regs);
            let next = self.pick_next_or_wait()?;
            self.transfer_to(regs, next);
            return Ok(PrimOutcome::Control);
        }
        self.remove_process_from_its_list(p);
        Ok(PrimOutcome::Value(p))
    }

    /// §11 termination: a process only ever unwinds itself. Terminating
    /// self unwinds to the base frame (running ensure blocks) and then
    /// halts; terminating another process pushes the Treaty terminate
    /// trampoline onto *its* stack and makes it runnable, so it performs
    /// the same self-termination in its own context.
    fn prim_process_terminate(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let nil = self.nil();
        let p = self.get(regs, r);
        if !self.is_process(p) {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        if p == self.active_process {
            self.unwind_and_return_from(regs, STACK_FRAMES_BASE, nil)?;
            return Ok(PrimOutcome::Control);
        }
        let stack = self.heap.slot(p.as_ptr(), PROCESS_STACK);
        if stack == nil {
            return Ok(PrimOutcome::Value(p)); // already terminated
        }
        let tramp = self.specials()[SPECIAL_TERMINATE_TRAMPOLINE];
        if !tramp.is_ptr() || tramp == nil {
            return Ok(PrimOutcome::Fail(FAIL_UNSUPPORTED_CONTEXT));
        }
        // Push the trampoline activation on top of the target's frames.
        let sa = stack.as_ptr();
        let t_off = self.heap.slot(p.as_ptr(), PROCESS_FRAME_OFFSET).as_int() as usize;
        let t_pc = self.heap.slot(p.as_ptr(), PROCESS_PC).as_int();
        let t_method = self.heap.slot(sa, t_off + FRAME_METHOD);
        let fs = self.method_frame_slots(t_method);
        let new_off = t_off + FRAME_RECEIVER + fs;
        let tramp_fs = self.method_frame_slots(tramp);
        let needed = new_off + FRAME_RECEIVER + tramp_fs;
        if needed > self.heap.num_slots(sa) as usize {
            // Grow the *target's* stack — sanctioned by the Stack
            // Invariant: it is not the running process.
            let cur_slots = self.heap.num_slots(sa) as usize;
            let mut new_slots = cur_slots * 2;
            while new_slots < needed {
                new_slots *= 2;
            }
            if new_slots * 8 > self.max_stack_bytes {
                return Ok(PrimOutcome::Fail(FAIL_UNSUPPORTED_CONTEXT));
            }
            self.temp_roots.push(p);
            let new_stack = self.alloc_gc(regs, CLASS_STACK, FMT_PTRS, new_slots)?;
            let p2 = self.temp_roots.pop().unwrap();
            let old_stack = self.heap.slot(p2.as_ptr(), PROCESS_STACK);
            for i in 0..cur_slots {
                let v = self.heap.slot(old_stack.as_ptr(), i);
                self.heap.set_slot_raw(new_stack.as_ptr(), i, v);
            }
            self.store_slot(p2.as_ptr(), PROCESS_STACK, new_stack);
            // A non-running stack full of possibly-young referents needs
            // remembering when it lives in old space.
            let nsa = new_stack.as_ptr();
            if self.heap.in_old_space(nsa) {
                let h = self.heap.header(nsa);
                if h.gc_bits() & GC_BIT_REMEMBERED == 0 {
                    self.heap
                        .set_header(nsa, h.with_gc_bits(h.gc_bits() | GC_BIT_REMEMBERED));
                    self.heap.ssb.push(nsa);
                }
            }
            // Re-derive everything the collection may have moved.
            return self.finish_terminate_other(p2, t_off, t_pc);
        }
        self.finish_terminate_other(p, t_off, t_pc)
    }

    /// Write the trampoline frame at the top of the (possibly just-grown)
    /// target stack and make the target runnable. All heap references are
    /// re-read here so it is safe after a collection.
    fn finish_terminate_other(
        &mut self,
        p: Value,
        t_off: usize,
        t_pc: i64,
    ) -> Result<PrimOutcome, VmError> {
        let nil = self.nil();
        let tramp = self.specials()[SPECIAL_TERMINATE_TRAMPOLINE];
        let tramp_fs = self.method_frame_slots(tramp);
        let stack = self.heap.slot(p.as_ptr(), PROCESS_STACK);
        let sa = stack.as_ptr();
        let t_method = self.heap.slot(sa, t_off + FRAME_METHOD);
        let fs = self.method_frame_slots(t_method);
        let new_off = t_off + FRAME_RECEIVER + fs;
        debug_assert!(new_off + FRAME_RECEIVER + tramp_fs <= self.heap.num_slots(sa) as usize);
        let serial = {
            let cur = self.heap.slot(p.as_ptr(), PROCESS_SERIAL_COUNTER).as_int();
            let next = if cur >= (1 << 30) - 1 { 1 } else { cur + 1 };
            self.heap
                .set_slot_raw(p.as_ptr(), PROCESS_SERIAL_COUNTER, Value::from_int(next));
            next
        };
        self.store_slot(sa, new_off + FRAME_CALLER, Value::from_int(t_off as i64));
        self.store_slot(
            sa,
            new_off + FRAME_RETINFO,
            Value::from_int(t_pc << RETINFO_PC_SHIFT),
        );
        self.store_slot(sa, new_off + FRAME_METHOD, tramp);
        self.store_slot(sa, new_off + FRAME_FLAGS, Value::from_int(serial << SERIAL_SHIFT));
        self.store_slot(sa, new_off + FRAME_RECEIVER, p);
        for k in 1..tramp_fs {
            self.store_slot(sa, new_off + FRAME_RECEIVER + k, nil);
        }
        self.heap
            .set_slot_raw(p.as_ptr(), PROCESS_FRAME_OFFSET, Value::from_int(new_off as i64));
        self.heap.set_slot_raw(p.as_ptr(), PROCESS_PC, Value::from_int(0));
        self.remove_process_from_its_list(p);
        self.make_runnable(p);
        Ok(PrimOutcome::Value(p))
    }

    /// primFrameInfo: (process, offset) → Array {method. pc-or-nil.
    /// receiver. slot values...} (§19, debugger support).
    fn prim_frame_info(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let process = self.get(regs, r + 1);
        let off = self.get(regs, r + 2);
        if !process.is_ptr() || !off.is_int() {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        let nil = self.nil();
        let stack = self.heap.slot(process.as_ptr(), PROCESS_STACK);
        if stack == nil {
            return Ok(PrimOutcome::Fail(FAIL_NOT_FOUND));
        }
        let o = off.as_int() as usize;
        let method = self.heap.slot(stack.as_ptr(), o + FRAME_METHOD);
        if !method.is_ptr() || method == nil {
            return Ok(PrimOutcome::Fail(FAIL_NOT_FOUND));
        }
        let fs = self.method_frame_slots(method);
        let arr = self.alloc_gc(regs, CLASS_ARRAY, FMT_PTRS, 2 + fs)?;
        // Re-derive after possible GC.
        let stack = self.heap.slot(process.as_ptr(), PROCESS_STACK);
        let method = self.heap.slot(stack.as_ptr(), o + FRAME_METHOD);
        let saved_off = self.heap.slot(process.as_ptr(), PROCESS_FRAME_OFFSET);
        let pc = if saved_off.is_int() && saved_off.as_int() as usize == o {
            // The suspended top frame: pc is the process's saved pc.
            self.heap.slot(process.as_ptr(), PROCESS_PC)
        } else if saved_off.is_int() {
            // An older frame: its resume pc lives in its callee's
            // returnInfo. Walk the caller chain down from the top.
            let mut f = saved_off.as_int() as usize;
            let mut found = nil;
            while f > o {
                let caller = self.heap.slot(stack.as_ptr(), f + FRAME_CALLER);
                if !caller.is_int() || caller.as_int() == 0 {
                    break;
                }
                if caller.as_int() as usize == o {
                    let ri = self.heap.slot(stack.as_ptr(), f + FRAME_RETINFO);
                    if ri.is_int() {
                        found = Value::from_int(ri.as_int() >> RETINFO_PC_SHIFT);
                    }
                    break;
                }
                f = caller.as_int() as usize;
            }
            found
        } else {
            nil
        };
        let a = arr.as_ptr();
        self.heap.set_slot_raw(a, 0, method);
        self.heap.set_slot_raw(a, 1, pc);
        for k in 0..fs {
            let v = self.heap.slot(stack.as_ptr(), o + FRAME_RECEIVER + k);
            self.heap.set_slot_raw(a, 2 + k, v);
        }
        Ok(PrimOutcome::Value(arr))
    }

    fn frame_flags_at(&self, regs: &Regs, off: usize) -> Value {
        self.heap.slot(regs.stack, off + FRAME_FLAGS)
    }

    fn method_at_frame(&self, regs: &Regs, off: usize) -> Value {
        self.heap.slot(regs.stack, off + FRAME_METHOD)
    }

    fn frame_bslot(&self, regs: &Regs, off: usize, k: usize) -> Value {
        self.heap.slot(regs.stack, off + FRAME_RECEIVER + k)
    }

    /// findHandlerFor: excClass from: offsetOrNil (§11). Walk caller links
    /// starting at the sender (from = nil) or at the caller of the frame at
    /// `from`, returning the nearest *armed* handler frame whose stored
    /// class handles excClass. v1 note: the spec has the image apply
    /// `handles:`; this primitive applies the plain is-kind-of rule
    /// directly (ExceptionSet arrives with the image-side loop).
    fn prim_find_handler(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let nil = self.nil();
        let exc_class = self.get(regs, r + 1);
        let from = self.get(regs, r + 2);
        let mut off = if from.is_int() {
            let caller = self
                .heap
                .slot(regs.stack, from.as_int() as usize + FRAME_CALLER);
            if !caller.is_int() || caller.as_int() == 0 {
                return Ok(PrimOutcome::Value(nil));
            }
            caller.as_int() as usize
        } else {
            regs.frame
        };
        loop {
            let flags = self.frame_flags_at(regs, off);
            if flags.is_int() && flags.as_int() & (FLAG_HANDLER as i64) != 0 {
                let method = self.method_at_frame(regs, off);
                let hsb = self.method_handler_slot_base(method);
                let state = self.frame_bslot(regs, off, hsb + HANDLER_SLOT_STATE);
                let stored = self.frame_bslot(regs, off, hsb + HANDLER_SLOT_CLASS);
                if state.is_int()
                    && state.as_int() == HANDLER_STATE_ARMED
                    && self.class_handles(stored, exc_class)
                {
                    return Ok(PrimOutcome::Value(Value::from_int(off as i64)));
                }
            }
            let caller = self.heap.slot(regs.stack, off + FRAME_CALLER);
            if !caller.is_int() || caller.as_int() == 0 {
                return Ok(PrimOutcome::Value(nil));
            }
            off = caller.as_int() as usize;
        }
    }

    /// stored class C handles exception class E iff E == C or C is on E's
    /// superclass chain.
    fn class_handles(&self, stored: Value, exc_class: Value) -> bool {
        let nil = self.nil();
        let mut c = exc_class;
        while c != nil && c.is_ptr() {
            if c == stored {
                return true;
            }
            c = self.heap.slot(c.as_ptr(), BEHAVIOR_SUPERCLASS);
        }
        false
    }

    /// unwindTo: offset serial: serial return: value — the one unwinder
    /// primitive (§11): pop frames (running pending ensure blocks) down to
    /// the target, then return `value` from it. Fails cleanly when the
    /// target activation is dead.
    fn prim_unwind_to(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let off = self.get(regs, r + 1);
        let serial = self.get(regs, r + 2);
        let value = self.get(regs, r + 3);
        if !off.is_int() || !serial.is_int() {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        let t = off.as_int() as usize;
        let live = t <= regs.frame && {
            let flags = self.frame_flags_at(regs, t);
            flags.is_int() && (flags.as_int() >> SERIAL_SHIFT) == serial.as_int()
        };
        if !live {
            return Ok(PrimOutcome::Fail(FAIL_NOT_FOUND));
        }
        self.unwind_and_return_from(regs, t, value)?;
        Ok(PrimOutcome::Control)
    }

    /// handlerInfoAt: offset → {storedClass. handlerBlock. serial}
    fn prim_handler_info(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let off = self.get(regs, r + 1);
        if !off.is_int() {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        let o = off.as_int() as usize;
        let flags = self.frame_flags_at(regs, o);
        if !flags.is_int() || flags.as_int() & (FLAG_HANDLER as i64) == 0 {
            return Ok(PrimOutcome::Fail(FAIL_NOT_FOUND));
        }
        let arr = self.alloc_gc(regs, CLASS_ARRAY, FMT_PTRS, 3)?;
        // Re-read post-GC.
        let flags = self.frame_flags_at(regs, o);
        let method = self.method_at_frame(regs, o);
        let hsb = self.method_handler_slot_base(method);
        let stored = self.frame_bslot(regs, o, hsb + HANDLER_SLOT_CLASS);
        let block = self.frame_bslot(regs, o, hsb + HANDLER_SLOT_BLOCK);
        let a = arr.as_ptr();
        self.heap.set_slot_raw(a, 0, stored);
        self.heap.set_slot_raw(a, 1, block);
        self.heap
            .set_slot_raw(a, 2, Value::from_int(flags.as_int() >> SERIAL_SHIFT));
        Ok(PrimOutcome::Value(arr))
    }

    /// setHandlerState: offset to: state (armed / in-progress / disarmed).
    fn prim_set_handler_state(&mut self, regs: &mut Regs, r: u8) -> Result<PrimOutcome, VmError> {
        let off = self.get(regs, r + 1);
        let state = self.get(regs, r + 2);
        if !off.is_int() || !state.is_int() {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        let o = off.as_int() as usize;
        let flags = self.frame_flags_at(regs, o);
        if !flags.is_int() || flags.as_int() & (FLAG_HANDLER as i64) == 0 {
            return Ok(PrimOutcome::Fail(FAIL_NOT_FOUND));
        }
        let method = self.method_at_frame(regs, o);
        let hsb = self.method_handler_slot_base(method);
        self.heap.set_slot_raw(
            regs.stack,
            o + FRAME_RECEIVER + hsb + HANDLER_SLOT_STATE,
            state,
        );
        Ok(PrimOutcome::Value(self.get(regs, r)))
    }

    /// signalContext → {senderFrameOffset. senderFrameSerial} — the frame
    /// that sent this primitive's method becomes the exception's signal
    /// frame (resume: unwinds back to it).
    fn prim_signal_context(&mut self, regs: &mut Regs) -> Result<PrimOutcome, VmError> {
        let arr = self.alloc_gc(regs, CLASS_ARRAY, FMT_PTRS, 2)?;
        let flags = self.frame_flags_at(regs, regs.frame);
        let a = arr.as_ptr();
        self.heap
            .set_slot_raw(a, 0, Value::from_int(regs.frame as i64));
        self.heap
            .set_slot_raw(a, 1, Value::from_int(flags.as_int() >> SERIAL_SHIFT));
        Ok(PrimOutcome::Value(arr))
    }

    /// BlockClosure>>value... — activate the block: push a frame whose
    /// method is the CompiledBlock and whose receiver slot holds the
    /// closure itself (§10).
    fn prim_block_value(
        &mut self,
        regs: &mut Regs,
        r: u8,
        argc: usize,
        dest: u8,
        want: usize,
    ) -> Result<PrimOutcome, VmError> {
        if (r as usize) < FRAME_RECEIVER {
            // Entered via OP_PRIM in a directly-run method: there is no
            // send staging area to overlap. Fall back.
            return Ok(PrimOutcome::Fail(FAIL_UNSUPPORTED_CONTEXT));
        }
        let closure = self.get(regs, r);
        if !closure.is_ptr()
            || self.heap.header(closure.as_ptr()).class_index() != CLASS_BLOCKCLOSURE
        {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_TYPE));
        }
        let block = self.heap.slot(closure.as_ptr(), CLOSURE_COMPILED_BLOCK);
        if argc != want || self.method_argc(block) != want {
            return Ok(PrimOutcome::Fail(FAIL_WRONG_ARGC));
        }
        self.push_frame(regs, block, r, dest, argc, None, FLAG_BLOCKCTX)?;
        Ok(PrimOutcome::Control)
    }
}
