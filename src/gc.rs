//! Generational, exact, moving GC (SPEC §14).
//!
//! Young generation: copying scavenge with an explicit work queue,
//! age-based tenuring into old space, and SSB-driven remembered set.
//!
//! Stack objects are scanned as ordinary pointer objects. This relies on a
//! VM-wide invariant: every word of a stack object is a valid tagged value
//! at all times (nil-filled at creation and growth, temps nil-filled at
//! frame push, pops leave previously-valid words in place). Dead frame
//! slots may briefly retain garbage — bounded by stack size — but stale
//! words are updated by every collection, so they never dangle. The spec's
//! "pin the running stack" rule is replaced by its sanctioned alternative:
//! the interpreter re-derives its cached registers after every collection
//! (via `gc_epoch`).

use crate::heap::Header;
use crate::treaty::*;
use crate::value::Value;
use crate::vm::{LookupEntry, Vm, VmError};

fn mask(bits: u32) -> u64 {
    (1u64 << bits) - 1
}

impl Vm {
    /// Scavenge the young generation. Called at safepoints only (allocation
    /// sites; pc/frameOffset already saved to the active process).
    pub fn collect_young(&mut self) -> Result<(), VmError> {
        // GC time attribution (plan §2): a due sample lands here with a
        // pseudo-leaf over the triggering Smalltalk stack (regs were saved
        // before every collection entry point).
        if self.sample_due_here() {
            self.take_sample_from_process("<vm:scavenge>");
        }
        let pause_start = std::time::Instant::now();
        self.gc_epoch += 1;
        self.scavenge_count += 1;

        // The global lookup cache holds bare Values; flushing is simpler
        // and cheaper than forwarding 4096 entries.
        for e in self.lookup_cache.iter_mut() {
            *e = LookupEntry::default();
        }

        let mut work: Vec<usize> = Vec::new();
        let mut tenured: Vec<usize> = Vec::new();

        // --- Roots ---
        macro_rules! fwd_vec {
            ($vec:expr) => {
                for i in 0..$vec.len() {
                    let v = $vec[i];
                    let f = self.forward(v, &mut work, &mut tenured);
                    $vec[i] = f;
                }
            };
        }
        // (Temporarily move the vecs out to satisfy the borrow checker.)
        let mut specials = std::mem::take(&mut self.specials);
        fwd_vec!(specials);
        self.specials = specials;
        let mut class_table = std::mem::take(&mut self.class_table);
        fwd_vec!(class_table);
        self.class_table = class_table;
        let mut temp_roots = std::mem::take(&mut self.temp_roots);
        fwd_vec!(temp_roots);
        self.temp_roots = temp_roots;
        let mut site_registry = std::mem::take(&mut self.site_registry);
        fwd_vec!(site_registry);
        self.site_registry = site_registry;
        let mut symbols = std::mem::take(&mut self.symbols);
        for (_, v) in symbols.iter_mut() {
            *v = self.forward(*v, &mut work, &mut tenured);
        }
        self.symbols = symbols;
        let mut timers = std::mem::take(&mut self.timer_requests);
        for (_, v) in timers.iter_mut() {
            *v = self.forward(*v, &mut work, &mut tenured);
        }
        self.timer_requests = timers;
        // JIT roots: the compilation queue and every handle's method
        // (JIT.md §5 — handles keep compiled methods alive).
        let mut jit = self.jit.take();
        if let Some(j) = jit.as_mut() {
            for v in j.queue.iter_mut() {
                *v = self.forward(*v, &mut work, &mut tenured);
            }
            for h in j.handles.iter_mut() {
                h.method = self.forward(h.method, &mut work, &mut tenured);
            }
        }
        self.jit = jit;
        let ap = self.active_process;
        self.active_process = self.forward(ap, &mut work, &mut tenured);

        // The running process's stack is a root in its own right: stores
        // into it are write-barrier-exempt, so when it lives in old space
        // nothing else guarantees its young referents are found. Scan its
        // contents unconditionally (idempotent if it also arrives via the
        // work queue).
        let mut active_old_stack = None;
        if self.active_process.is_ptr() && self.active_process != self.specials[SPECIAL_NIL] {
            let p = self.active_process.as_ptr();
            let stack = self.heap.slot(p, PROCESS_STACK);
            if stack.is_ptr() && stack != self.specials[SPECIAL_NIL] {
                let f = self.forward(stack, &mut work, &mut tenured);
                if f != stack {
                    self.heap.set_slot_raw(p, PROCESS_STACK, f);
                }
                self.scan_object(f.as_ptr(), &mut work, &mut tenured);
                if self.heap.in_old_space(f.as_ptr()) {
                    active_old_stack = Some(f.as_ptr());
                }
            }
        }

        // --- Remembered set: scan old objects that may point young ---
        let ssb = std::mem::take(&mut self.heap.ssb);
        self.counters.gc_ssb_drained += ssb.len() as u64;
        self.counters.gc_ssb_drained_max = self.counters.gc_ssb_drained_max.max(ssb.len() as u64);
        for &obj in &ssb {
            self.scan_object(obj, &mut work, &mut tenured);
        }

        // --- Transitive closure ---
        while let Some(obj) = work.pop() {
            self.scan_object(obj, &mut work, &mut tenured);
        }

        // --- Flip semispaces ---
        std::mem::swap(&mut self.heap.young_from, &mut self.heap.young_to);
        self.heap.young_to.reset();

        // --- Rebuild the remembered set: drained SSB entries and freshly
        // tenured objects stay/become remembered iff they still point young.
        let mut recheck: Vec<usize> = ssb;
        recheck.extend(tenured);
        recheck.extend(active_old_stack);
        recheck.sort_unstable();
        recheck.dedup();
        for obj in recheck {
            let points_young = self.object_points_young(obj);
            let h = self.heap.header(obj);
            let bits = h.gc_bits();
            if points_young {
                if bits & GC_BIT_REMEMBERED == 0 {
                    self.heap
                        .set_header(obj, h.with_gc_bits(bits | GC_BIT_REMEMBERED));
                }
                self.heap.ssb.push(obj);
            } else if bits & GC_BIT_REMEMBERED != 0 {
                self.heap
                    .set_header(obj, h.with_gc_bits(bits & !GC_BIT_REMEMBERED));
            }
        }
        self.counters.gc_remembered_rebuilt += self.heap.ssb.len() as u64;
        self.counters.scavenge_ns += pause_start.elapsed().as_nanos() as u64;
        self.jit_sync_globals();
        Ok(())
    }

    /// Copy a from-space object (or return the existing forward), leaving a
    /// forwarding word (new address | 1) in the old header slot. Never
    /// fails: to-space is as large as from-space, and tenure targets fall
    /// back to to-space when old space is full.
    fn forward(&mut self, v: Value, work: &mut Vec<usize>, tenured: &mut Vec<usize>) -> Value {
        if !v.is_ptr() {
            return v;
        }
        let addr = v.as_ptr();
        if !self.heap.young_from.contains(addr) {
            return v;
        }
        let hword = self.heap.word(addr);
        if hword & 1 == 1 {
            return Value::from_ptr((hword & !1) as usize);
        }
        let header = Header::from_raw(hword);
        let has_over = header.num_slots_field() == HDR_NSLOTS_OVERFLOW;
        let nslots = if has_over {
            self.heap.word(addr - 8) as usize
        } else {
            header.num_slots_field() as usize
        };
        let extra = has_over as usize;
        let total_words = extra + 1 + nslots;

        let age = (header.gc_bits() >> GC_AGE_SHIFT) & mask(GC_AGE_BITS);
        let new_age = (age + 1).min(mask(GC_AGE_BITS));
        let wants_tenure = new_age >= self.tenure_threshold;

        let (start, is_old) = if wants_tenure {
            match self.heap.old.alloc_words(total_words) {
                Some(s) => (s, true),
                None => (
                    self.heap
                        .young_to
                        .alloc_words(total_words)
                        .expect("to-space can hold all survivors"),
                    false,
                ),
            }
        } else {
            (
                self.heap
                    .young_to
                    .alloc_words(total_words)
                    .expect("to-space can hold all survivors"),
                false,
            )
        };

        let src = addr - extra * 8;
        unsafe {
            std::ptr::copy_nonoverlapping(src as *const u64, start as *mut u64, total_words);
        }
        self.counters.gc_bytes_copied += (total_words * 8) as u64;
        if is_old {
            self.counters.gc_bytes_tenured += (total_words * 8) as u64;
        }
        let new_obj = start + extra * 8;
        // Fresh gcBits: keep immutable/pinned, stamp the age, clear
        // mark/remembered (the rebuild pass re-remembers as needed).
        let keep = header.gc_bits() & (GC_BIT_IMMUTABLE | GC_BIT_PINNED);
        let new_header = header.with_gc_bits(keep | (new_age << GC_AGE_SHIFT));
        self.heap.set_header(new_obj, new_header);
        self.heap.set_word(addr, (new_obj as u64) | 1);

        work.push(new_obj);
        if is_old {
            tenured.push(new_obj);
        }
        Value::from_ptr(new_obj)
    }

    /// Forward every pointer slot of a fixed/pointer-format object.
    fn scan_object(&mut self, obj: usize, work: &mut Vec<usize>, tenured: &mut Vec<usize>) {
        let h = self.heap.header(obj);
        if h.is_bytes() {
            return;
        }
        let n = self.heap.num_slots(obj) as usize;
        for i in 0..n {
            let v = self.heap.slot(obj, i);
            let f = self.forward(v, work, tenured);
            if f != v {
                self.heap.set_slot_raw(obj, i, f);
            }
        }
    }

    /// Old-generation mark-compact (§14): Lisp-2 sliding with a forwarding
    /// side table. Scavenges first (young is fresh afterwards), then marks
    /// the full reachable graph, slides marked old objects left preserving
    /// allocation order, and rewrites every pointer through the table.
    /// nil/true/false are the first (always live) old objects, so their
    /// addresses never change.
    pub fn collect_old(&mut self) -> Result<(), VmError> {
        if self.sample_due_here() {
            self.take_sample_from_process("<vm:compact>");
        }
        self.collect_young()?;
        // The pause clock starts after the scavenge — that time is already
        // in scavenge_ns, so scavenge_ns + compact_ns is total GC time.
        let pause_start = std::time::Instant::now();
        self.gc_epoch += 1;
        self.compact_count += 1;
        for e in self.lookup_cache.iter_mut() {
            *e = LookupEntry::default();
        }

        // --- Mark (both generations get mark bits; only old moves) ---
        let mut stack: Vec<usize> = Vec::new();
        let roots: Vec<Value> = self
            .specials
            .iter()
            .chain(self.class_table.iter())
            .chain(self.temp_roots.iter())
            .chain(self.site_registry.iter())
            .copied()
            .chain(self.symbols.iter().map(|(_, v)| *v))
            .chain(self.timer_requests.iter().map(|(_, v)| *v))
            .chain(self.jit_roots())
            .chain(std::iter::once(self.active_process))
            .collect();
        for v in &roots {
            self.mark_value(*v, &mut stack);
        }
        while let Some(obj) = stack.pop() {
            let h = self.heap.header(obj);
            if h.is_bytes() {
                continue;
            }
            let n = self.heap.num_slots(obj) as usize;
            for i in 0..n {
                let v = self.heap.slot(obj, i);
                self.mark_value(v, &mut stack);
            }
        }

        // --- Compute forwarding: slide marked old objects left ---
        let old_base = self.heap.old.base();
        let old_top = self.heap.old.top();
        let mut fwd: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        let mut live: Vec<usize> = Vec::new();
        self.heap.walk_region(old_base, old_top, |obj| live.push(obj));
        // Corruption tripwire (debug builds): every walked object must have
        // a sane header before we trust its footprint for the slide.
        #[cfg(debug_assertions)]
        for &obj in &live {
            let h = self.heap.header(obj);
            let fp = self.heap.footprint(obj);
            assert!(
                h.class_index() != 0
                    && (h.class_index() as usize) < self.class_table.len()
                    && fp < self.heap.old.limit() - self.heap.old.base(),
                "corrupt old-space object at {obj:#x}: header {:#x} footprint {fp}",
                h.raw()
            );
        }
        // footprint() already includes the overflow word, so the cursor
        // advances by exactly footprint — adding `extra` again (the old
        // bug) drifted the cursor 8 bytes right per live overflow-header
        // object, eventually sliding objects *forward* over neighbours
        // they had not yet been read from. The move geometry is captured
        // here, before any slide can clobber an old header.
        let mut next = old_base;
        // (old storage start, new storage start, words, new obj)
        let mut moves: Vec<(usize, usize, usize, usize)> = Vec::new();
        for &obj in &live {
            let h = self.heap.header(obj);
            if h.gc_bits() & GC_BIT_MARK == 0 {
                continue;
            }
            let start = self.heap.storage_start(obj);
            let extra = obj - start;
            let new_obj = next + extra;
            let words = self.heap.footprint(obj) / 8;
            fwd.insert(obj, new_obj);
            moves.push((start, next, words, new_obj));
            next += words * 8;
        }

        // --- Fixup: roots, then every live object's slots ---
        macro_rules! fix {
            ($v:expr) => {{
                let v: Value = $v;
                if v.is_ptr() {
                    if let Some(&n) = fwd.get(&v.as_ptr()) {
                        Value::from_ptr(n)
                    } else {
                        v
                    }
                } else {
                    v
                }
            }};
        }
        for i in 0..self.specials.len() {
            self.specials[i] = fix!(self.specials[i]);
        }
        for i in 0..self.class_table.len() {
            self.class_table[i] = fix!(self.class_table[i]);
        }
        for i in 0..self.temp_roots.len() {
            self.temp_roots[i] = fix!(self.temp_roots[i]);
        }
        for i in 0..self.site_registry.len() {
            self.site_registry[i] = fix!(self.site_registry[i]);
        }
        for i in 0..self.symbols.len() {
            self.symbols[i].1 = fix!(self.symbols[i].1);
        }
        for i in 0..self.timer_requests.len() {
            self.timer_requests[i].1 = fix!(self.timer_requests[i].1);
        }
        if let Some(jit) = self.jit.as_mut() {
            for v in jit.queue.iter_mut() {
                *v = fix!(*v);
            }
            for h in jit.handles.iter_mut() {
                h.method = fix!(h.method);
            }
        }
        self.active_process = fix!(self.active_process);

        let fix_slots = |heap: &mut crate::heap::Heap, obj: usize| {
            let h = heap.header(obj);
            if h.gc_bits() & GC_BIT_MARK == 0 || h.is_bytes() {
                return;
            }
            let n = heap.num_slots(obj) as usize;
            for i in 0..n {
                let v = heap.slot(obj, i);
                if v.is_ptr() {
                    if let Some(&nw) = fwd.get(&v.as_ptr()) {
                        heap.set_slot_raw(obj, i, Value::from_ptr(nw));
                    }
                }
            }
        };
        // Live (marked) young objects may point at moved old objects.
        let yb = self.heap.young_from.base();
        let yt = self.heap.young_from.top();
        let mut young_objs: Vec<usize> = Vec::new();
        self.heap.walk_region(yb, yt, |o| young_objs.push(o));
        for &o in &young_objs {
            fix_slots(&mut self.heap, o);
        }
        for &o in &live {
            fix_slots(&mut self.heap, o);
        }

        // --- Move (ascending: sliding left, memmove-safe), clear marks,
        // rebuild the remembered set. Geometry was precomputed above: the
        // old header must not be re-read here, an earlier slide may have
        // overwritten it. ---
        self.heap.ssb.clear();
        for (start, new_start, words, new_obj) in &moves {
            unsafe {
                std::ptr::copy(*start as *const u64, *new_start as *mut u64, *words);
            }
            let h = self.heap.header(*new_obj);
            let mut bits = h.gc_bits() & !(GC_BIT_MARK | GC_BIT_REMEMBERED);
            if self.object_points_young(*new_obj) {
                bits |= GC_BIT_REMEMBERED;
                self.heap.ssb.push(*new_obj);
            }
            self.heap.set_header(*new_obj, h.with_gc_bits(bits));
        }
        // Clear mark bits on young objects.
        for &o in &young_objs {
            let h = self.heap.header(o);
            if h.gc_bits() & GC_BIT_MARK != 0 {
                self.heap.set_header(o, h.with_gc_bits(h.gc_bits() & !GC_BIT_MARK));
            }
        }
        // New allocation top; zero the freed tail so future walks stop.
        let new_top_words = (next - old_base) / 8;
        let old_alloc_words = self.heap.old.alloc;
        self.heap.old.alloc = new_top_words;
        unsafe {
            std::ptr::write_bytes(
                next as *mut u64,
                0,
                old_alloc_words - new_top_words,
            );
        }
        self.counters.compact_ns += pause_start.elapsed().as_nanos() as u64;
        self.jit_sync_globals();
        Ok(())
    }

    fn mark_value(&mut self, v: Value, stack: &mut Vec<usize>) {
        if !v.is_ptr() {
            return;
        }
        let a = v.as_ptr();
        if !self.heap.is_young(a) && !self.heap.in_old_space(a) {
            return;
        }
        let h = self.heap.header(a);
        if h.gc_bits() & GC_BIT_MARK != 0 {
            return;
        }
        self.heap.set_header(a, h.with_gc_bits(h.gc_bits() | GC_BIT_MARK));
        stack.push(a);
    }

    fn object_points_young(&self, obj: usize) -> bool {
        let h = self.heap.header(obj);
        if h.is_bytes() {
            return false;
        }
        let n = self.heap.num_slots(obj) as usize;
        (0..n).any(|i| {
            let v = self.heap.slot(obj, i);
            v.is_ptr() && self.heap.is_young(v.as_ptr())
        })
    }
}
