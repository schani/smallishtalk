//! The VM: heap + exact root set + object-model helpers (SPEC §4, §5, §7).
//!
//! All references are tagged `Value`s or addresses; no Rust reference into
//! the heap survives across an allocation point. Objects held only in Rust
//! locals are invisible to the GC — test fixtures therefore allocate
//! long-lived structure (classes, methods, symbols) in old space, and
//! in-flight multi-step allocations root temporaries in `temp_roots`.

use crate::counters::Counters;
use crate::heap::{Header, Heap, HeapConfig};
use crate::profile::{Profiler, SafepointShared};
use crate::treaty::*;
use crate::value::Value;

#[derive(Debug)]
pub enum VmError {
    OutOfMemory,
    StackOverflow,
    Fatal(String),
}

pub fn fatal<T>(msg: impl Into<String>) -> Result<T, VmError> {
    Err(VmError::Fatal(msg.into()))
}

/// Global lookup cache entry: (classIndex, selector) -> method.
#[derive(Clone, Copy, Default)]
pub struct LookupEntry {
    pub class_index: u32,
    pub selector: Value,
    pub method: Value,
}

impl Default for Value {
    fn default() -> Value {
        Value::from_raw(0)
    }
}

pub struct VmConfig {
    pub heap: HeapConfig,
    pub max_stack_bytes: usize,
}

impl Default for VmConfig {
    fn default() -> VmConfig {
        VmConfig {
            heap: HeapConfig::default(),
            max_stack_bytes: DEFAULT_MAX_STACK_BYTES,
        }
    }
}

pub struct Vm {
    pub heap: Heap,
    /// classIndex -> class object. Entry 0 invalid (nil).
    pub class_table: Vec<Value>,
    pub specials: Vec<Value>,
    /// Bootstrap symbol intern table (image-side SymbolTable arrives later).
    /// A GC root: both the bytes (for lookup) and the heap Values.
    pub symbols: Vec<(Vec<u8>, Value)>,
    /// Handle scope for in-flight primitive/interpreter temporaries.
    pub temp_roots: Vec<Value>,
    pub active_process: Value,
    /// Registry of send-site Arrays for eager inline-cache clearing (§8).
    pub site_registry: Vec<Value>,
    pub lookup_cache: Vec<LookupEntry>,
    pub hash_counter: u32,
    /// Safepoint request flags, shared with the profiler timer thread.
    /// The interpreter's poll is one relaxed atomic load.
    pub safepoint: std::sync::Arc<SafepointShared>,
    /// Incremented by every collection; the interpreter reloads cached
    /// addresses when it observes a change.
    pub gc_epoch: u64,
    pub max_stack_bytes: usize,
    /// Test hook: stdio primitive output is captured here as well.
    pub stdout_capture: Vec<u8>,
    pub scavenge_count: u64,
    pub compact_count: u64,
    pub stack_grow_count: u64,
    pub tenure_threshold: u64,
    /// Open file table: fd = index (§16 file primitives).
    pub files: Vec<Option<std::fs::File>>,
    pub start_instant: std::time::Instant,
    /// Timer requests: (deadline monotonic ms, semaphore) (§13).
    pub timer_requests: Vec<(i64, Value)>,
    /// Corpus snapshot mode (§20 phase 4): after this many sends, write an
    /// image to the path and record where the output capture stood.
    pub snapshot_after_sends: Option<(u64, String)>,
    pub sends_seen: u64,
    pub snapshot_fired_at_capture_len: Option<usize>,
    /// Exact VM counters (profiling plan §3).
    pub counters: Counters,
    /// The sampling profiler (profiling plan §2).
    pub profiler: Profiler,
    /// Host-side UI state: the ARGB present buffer, the (scripted or live)
    /// event queue, and the selected clock (UI.md §3/§4A). Not persisted.
    pub host: crate::host_ui::HostUi,
}

impl Vm {
    pub fn bare(config: VmConfig) -> Vm {
        let mut vm = Vm {
            heap: Heap::new(config.heap),
            class_table: Vec::new(),
            specials: Vec::new(),
            symbols: Vec::new(),
            temp_roots: Vec::new(),
            active_process: Value::from_raw(0),
            site_registry: Vec::new(),
            lookup_cache: vec![LookupEntry::default(); LOOKUP_CACHE_SIZE],
            hash_counter: 0,
            safepoint: SafepointShared::new(),
            gc_epoch: 0,
            max_stack_bytes: config.max_stack_bytes,
            stdout_capture: Vec::new(),
            scavenge_count: 0,
            compact_count: 0,
            stack_grow_count: 0,
            tenure_threshold: TENURE_AGE,
            files: Vec::new(),
            start_instant: std::time::Instant::now(),
            timer_requests: Vec::new(),
            snapshot_after_sends: None,
            sends_seen: 0,
            snapshot_fired_at_capture_len: None,
            counters: Counters::new(),
            profiler: Profiler::default(),
            host: crate::host_ui::HostUi::new(),
        };
        vm.bootstrap();
        vm
    }

    /// A bare VM with a small heap, for unit tests.
    pub fn bare_test() -> Vm {
        Vm::bare(VmConfig {
            heap: HeapConfig {
                young_bytes: 2 * 1024 * 1024,
                old_bytes: 8 * 1024 * 1024,
                ..HeapConfig::default()
            },
            max_stack_bytes: DEFAULT_MAX_STACK_BYTES,
        })
    }

    /// Create nil/true/false, minimal class objects for every Treaty class,
    /// and the special objects array. nil, true, false are the first three
    /// objects in old space (§1: known fixed addresses).
    fn bootstrap(&mut self) {
        let nil_addr = self
            .heap
            .alloc_fixed_old(CLASS_UNDEFINED_OBJECT, 0, Value::from_raw(0))
            .expect("old space");
        let nil = Value::from_ptr(nil_addr);
        // Fix up: alloc_fixed_old(_, 0, fill) has no slots to fill; nil is done.
        let true_addr = self.heap.alloc_fixed_old(CLASS_TRUE, 0, nil).expect("old space");
        let false_addr = self.heap.alloc_fixed_old(CLASS_FALSE, 0, nil).expect("old space");
        let true_v = Value::from_ptr(true_addr);
        let false_v = Value::from_ptr(false_addr);

        self.specials = vec![nil; SPECIAL_OBJECTS_COUNT];
        self.specials[SPECIAL_TRUE] = true_v;
        self.specials[SPECIAL_FALSE] = false_v;

        // Minimal class objects for the reserved index range. Instance
        // format+named-slot-count per class, superclass wired below.
        self.class_table = vec![nil; FIRST_UNRESERVED_CLASS_INDEX as usize];
        let classes: &[(u32, u64, usize)] = &[
            (CLASS_OBJECT, FMT_FIXED, 0),
            (CLASS_BEHAVIOR, FMT_FIXED, BEHAVIOR_NUM_VM_SLOTS),
            (CLASS_CLASS, FMT_FIXED, BEHAVIOR_NUM_VM_SLOTS),
            (CLASS_METACLASS, FMT_FIXED, BEHAVIOR_NUM_VM_SLOTS),
            (CLASS_UNDEFINED_OBJECT, FMT_FIXED, 0),
            (CLASS_TRUE, FMT_FIXED, 0),
            (CLASS_FALSE, FMT_FIXED, 0),
            (CLASS_SMALLINTEGER, FMT_FIXED, 0),
            (CLASS_FLOAT, FMT_BYTES_BASE, 0),
            (CLASS_CHARACTER, FMT_FIXED, 1),
            (CLASS_STRING, FMT_BYTES_BASE, 0),
            (CLASS_BYTESTRING, FMT_BYTES_BASE, 0),
            (CLASS_SYMBOL, FMT_BYTES_BASE, 0),
            (CLASS_LARGE_POSITIVE_INTEGER, FMT_BYTES_BASE, 0),
            (CLASS_LARGE_NEGATIVE_INTEGER, FMT_BYTES_BASE, 0),
            (CLASS_ARRAY, FMT_PTRS, 0),
            (CLASS_BYTEARRAY, FMT_BYTES_BASE, 0),
            (CLASS_ORDERED_COLLECTION, FMT_FIXED, 2),
            (CLASS_ASSOCIATION, FMT_FIXED, 2),
            (CLASS_BOX, FMT_FIXED, 1),
            (CLASS_BLOCKCLOSURE, FMT_FIXED, CLOSURE_CAPTURED_BASE),
            (CLASS_COMPILEDMETHOD, FMT_FIXED, METHOD_NUM_SLOTS),
            (CLASS_COMPILEDBLOCK, FMT_FIXED, BLOCK_NUM_SLOTS),
            (CLASS_PROCESS, FMT_FIXED, PROCESS_NUM_VM_SLOTS),
            (CLASS_SEMAPHORE, FMT_FIXED, SEMAPHORE_NUM_VM_SLOTS),
            (CLASS_METHODDICTIONARY, FMT_FIXED, MDICT_NUM_VM_SLOTS),
            (CLASS_PROCESSOR_SCHEDULER, FMT_FIXED, SCHEDULER_NUM_VM_SLOTS),
            (CLASS_MESSAGE, FMT_FIXED, 2),
            (CLASS_SYSTEM_DICTIONARY, FMT_FIXED, 0),
            (CLASS_STACK, FMT_PTRS, 0),
            (CLASS_LINKED_LIST, FMT_FIXED, LIST_NUM_VM_SLOTS),
        ];
        for &(idx, format, nslots) in classes {
            let c = self.make_class_object(idx, format, nslots, nil);
            self.class_table[idx as usize] = c;
        }
        // Superclass chains: everything under Object; Symbol under
        // ByteString under String under Object.
        let object = self.class_table[CLASS_OBJECT as usize];
        for &(idx, _, _) in classes {
            if idx != CLASS_OBJECT {
                let c = self.class_table[idx as usize];
                self.heap.set_slot_raw(c.as_ptr(), BEHAVIOR_SUPERCLASS, object);
            }
        }
        let string = self.class_table[CLASS_STRING as usize];
        let bytestring = self.class_table[CLASS_BYTESTRING as usize];
        let symbol = self.class_table[CLASS_SYMBOL as usize];
        self.heap.set_slot_raw(bytestring.as_ptr(), BEHAVIOR_SUPERCLASS, string);
        self.heap.set_slot_raw(symbol.as_ptr(), BEHAVIOR_SUPERCLASS, bytestring);

        // Special objects (A.4).
        self.specials[SPECIAL_SEL_DOES_NOT_UNDERSTAND] = self.intern("doesNotUnderstand:");
        self.specials[SPECIAL_SEL_MUST_BE_BOOLEAN] = self.intern("mustBeBoolean");

        let spec_sels: [&str; SPECSEL_COUNT] = [
            "+", "-", "*", "//", "\\\\", "<", ">", "<=", ">=", "=", "==",
            "at:", "at:put:", "size", "class", "not",
        ];
        let sels = self
            .heap
            .alloc_ptrs_old(CLASS_ARRAY, SPECSEL_COUNT, nil)
            .expect("old space");
        for (i, s) in spec_sels.iter().enumerate() {
            let sym = self.intern(s);
            self.heap.set_slot_raw(sels, i, sym);
        }
        self.specials[SPECIAL_SPECIALIZED_SELECTORS] = Value::from_ptr(sels);

        // Scheduler with empty run queues.
        let queues = self
            .heap
            .alloc_ptrs_old(CLASS_ARRAY, NUM_PRIORITIES, nil)
            .expect("old space");
        for i in 0..NUM_PRIORITIES {
            let q = self
                .heap
                .alloc_fixed_old(CLASS_LINKED_LIST, LIST_NUM_VM_SLOTS, nil)
                .expect("old space");
            self.heap.set_slot_raw(queues, i, Value::from_ptr(q));
        }
        let sched = self
            .heap
            .alloc_fixed_old(CLASS_PROCESSOR_SCHEDULER, SCHEDULER_NUM_VM_SLOTS, nil)
            .expect("old space");
        self.heap.set_slot_raw(sched, SCHEDULER_QUEUES, Value::from_ptr(queues));
        self.specials[SPECIAL_PROCESSOR] = Value::from_ptr(sched);

        for idx in [SPECIAL_LOW_SPACE_SEMAPHORE, SPECIAL_TIMER_SEMAPHORE] {
            let sem = self.make_semaphore_old();
            self.specials[idx] = sem;
        }

        self.active_process = nil;
    }

    fn make_class_object(&mut self, idx: u32, format: u64, nslots: usize, nil: Value) -> Value {
        let mdict = self.make_mdict_old(nil);
        let c = self
            .heap
            .alloc_fixed_old(CLASS_CLASS, BEHAVIOR_NUM_VM_SLOTS, nil)
            .expect("old space");
        self.heap.set_slot_raw(c, BEHAVIOR_SUPERCLASS, nil);
        self.heap.set_slot_raw(c, BEHAVIOR_METHOD_DICTIONARY, mdict);
        self.heap.set_slot_raw(
            c,
            BEHAVIOR_FORMAT_AND_SLOTS,
            Value::from_int(((format as i64) << FORMAT_AND_SLOTS_FORMAT_SHIFT) | nslots as i64),
        );
        self.heap
            .set_slot_raw(c, BEHAVIOR_CLASS_INDEX, Value::from_int(idx as i64));
        Value::from_ptr(c)
    }

    fn make_mdict_old(&mut self, nil: Value) -> Value {
        let keys = self.heap.alloc_ptrs_old(CLASS_ARRAY, 0, nil).expect("old space");
        let values = self.heap.alloc_ptrs_old(CLASS_ARRAY, 0, nil).expect("old space");
        let md = self
            .heap
            .alloc_fixed_old(CLASS_METHODDICTIONARY, MDICT_NUM_VM_SLOTS, nil)
            .expect("old space");
        self.heap.set_slot_raw(md, MDICT_KEYS, Value::from_ptr(keys));
        self.heap.set_slot_raw(md, MDICT_VALUES, Value::from_ptr(values));
        Value::from_ptr(md)
    }

    pub fn make_semaphore_old(&mut self) -> Value {
        let nil = self.nil();
        let sem = self
            .heap
            .alloc_fixed_old(CLASS_SEMAPHORE, SEMAPHORE_NUM_VM_SLOTS, nil)
            .expect("old space");
        self.heap
            .set_slot_raw(sem, SEMAPHORE_EXCESS_SIGNALS, Value::from_int(0));
        Value::from_ptr(sem)
    }

    // --- Well-known values ---

    #[inline(always)]
    pub fn nil(&self) -> Value {
        self.specials[SPECIAL_NIL]
    }

    #[inline(always)]
    pub fn true_v(&self) -> Value {
        self.specials[SPECIAL_TRUE]
    }

    #[inline(always)]
    pub fn false_v(&self) -> Value {
        self.specials[SPECIAL_FALSE]
    }

    pub fn bool_v(&self, b: bool) -> Value {
        if b { self.true_v() } else { self.false_v() }
    }

    pub fn specials(&self) -> &[Value] {
        &self.specials
    }

    pub fn set_special(&mut self, idx: usize, v: Value) {
        self.specials[idx] = v;
    }

    pub fn class_table_at(&self, idx: u32) -> Value {
        self.class_table[idx as usize]
    }

    // --- Symbols ---

    pub fn intern(&mut self, name: &str) -> Value {
        self.intern_bytes(name.as_bytes())
    }

    pub fn intern_bytes(&mut self, bytes: &[u8]) -> Value {
        if let Some((_, v)) = self.symbols.iter().find(|(b, _)| b == bytes) {
            return *v;
        }
        let addr = self
            .heap
            .alloc_bytes_old(CLASS_SYMBOL, bytes.len())
            .expect("old space");
        self.heap.write_bytes(addr, bytes);
        self.heap.set_immutable(addr);
        let v = Value::from_ptr(addr);
        self.symbols.push((bytes.to_vec(), v));
        v
    }

    // --- Write barrier (§14) ---

    #[inline(always)]
    pub fn write_barrier(&mut self, obj: usize, v: Value) {
        if v.is_ptr() && self.heap.is_young(v.as_ptr()) && self.heap.in_old_space(obj) {
            let h = self.heap.header(obj);
            if h.gc_bits() & GC_BIT_REMEMBERED == 0 {
                self.heap
                    .set_header(obj, h.with_gc_bits(h.gc_bits() | GC_BIT_REMEMBERED));
                self.heap.ssb.push(obj);
            }
        }
    }

    /// Barrier-checked slot store — every GC-visible pointer store except
    /// stores into the running process's own stack goes through here.
    #[inline(always)]
    pub fn store_slot(&mut self, obj: usize, i: usize, v: Value) {
        self.heap.set_slot_raw(obj, i, v);
        self.write_barrier(obj, v);
    }

    // --- Classes and lookup ---

    #[inline(always)]
    pub fn class_index_of(&self, v: Value) -> u32 {
        if v.is_int() {
            CLASS_SMALLINTEGER
        } else {
            self.heap.header(v.as_ptr()).class_index()
        }
    }

    pub fn class_of(&self, v: Value) -> Value {
        self.class_table[self.class_index_of(v) as usize]
    }

    pub fn class_format_and_slots(&self, class: Value) -> (u64, usize) {
        let fas = self.heap.slot(class.as_ptr(), BEHAVIOR_FORMAT_AND_SLOTS).as_int();
        (
            (fas >> FORMAT_AND_SLOTS_FORMAT_SHIFT) as u64,
            (fas as u64 & FORMAT_AND_SLOTS_NSLOTS_MASK) as usize,
        )
    }

    /// Slow-path lookup: walk method dictionaries up the superclass chain.
    pub fn lookup_method(&self, class_index: u32, selector: Value) -> Option<Value> {
        self.lookup_method_counted(class_index, selector).0
    }

    /// Lookup that also reports how many classes the walk visited (for the
    /// dict-walk counters).
    pub fn lookup_method_counted(
        &self,
        class_index: u32,
        selector: Value,
    ) -> (Option<Value>, u64) {
        let nil = self.nil();
        let mut cls = self.class_table[class_index as usize];
        let mut walked = 0u64;
        while cls != nil {
            walked += 1;
            let mdict = self.heap.slot(cls.as_ptr(), BEHAVIOR_METHOD_DICTIONARY);
            if mdict != nil {
                if let Some(m) = self.mdict_lookup(mdict, selector) {
                    return (Some(m), walked);
                }
            }
            cls = self.heap.slot(cls.as_ptr(), BEHAVIOR_SUPERCLASS);
        }
        (None, walked)
    }

    /// Lookup starting *above* the given (static) class — SENDSUPER.
    pub fn lookup_method_above(&self, static_class: Value, selector: Value) -> Option<Value> {
        let nil = self.nil();
        let sup = self.heap.slot(static_class.as_ptr(), BEHAVIOR_SUPERCLASS);
        if sup == nil {
            return None;
        }
        let idx = self.heap.slot(sup.as_ptr(), BEHAVIOR_CLASS_INDEX).as_int() as u32;
        self.lookup_method(idx, selector)
    }

    pub fn mdict_lookup(&self, mdict: Value, selector: Value) -> Option<Value> {
        let keys = self.heap.slot(mdict.as_ptr(), MDICT_KEYS);
        let vals = self.heap.slot(mdict.as_ptr(), MDICT_VALUES);
        let n = self.heap.num_slots(keys.as_ptr()) as usize;
        for i in 0..n {
            if self.heap.slot(keys.as_ptr(), i) == selector {
                return Some(self.heap.slot(vals.as_ptr(), i));
            }
        }
        None
    }

    /// Install a method: grow the dictionary's parallel arrays, stamp
    /// selector/methodClass, flush caches eagerly (§8).
    pub fn install_method(&mut self, class: Value, selector: Value, method: Value) {
        self.counters.method_installs += 1;
        let nil = self.nil();
        let mdict = self.heap.slot(class.as_ptr(), BEHAVIOR_METHOD_DICTIONARY);
        let keys = self.heap.slot(mdict.as_ptr(), MDICT_KEYS);
        let vals = self.heap.slot(mdict.as_ptr(), MDICT_VALUES);
        let n = self.heap.num_slots(keys.as_ptr()) as usize;

        // Overwrite in place if the selector is already present.
        for i in 0..n {
            if self.heap.slot(keys.as_ptr(), i) == selector {
                let va = vals.as_ptr();
                self.store_slot(va, i, method);
                self.stamp_and_flush(method, selector, class);
                return;
            }
        }
        let new_keys = self
            .heap
            .alloc_ptrs_old(CLASS_ARRAY, n + 1, nil)
            .expect("old space");
        let new_vals = self
            .heap
            .alloc_ptrs_old(CLASS_ARRAY, n + 1, nil)
            .expect("old space");
        for i in 0..n {
            let k = self.heap.slot(keys.as_ptr(), i);
            let v = self.heap.slot(vals.as_ptr(), i);
            self.store_slot(new_keys, i, k);
            self.store_slot(new_vals, i, v);
        }
        self.store_slot(new_keys, n, selector);
        self.store_slot(new_vals, n, method);
        let md = mdict.as_ptr();
        self.store_slot(md, MDICT_KEYS, Value::from_ptr(new_keys));
        self.store_slot(md, MDICT_VALUES, Value::from_ptr(new_vals));
        self.stamp_and_flush(method, selector, class);
    }

    fn stamp_and_flush(&mut self, method: Value, selector: Value, class: Value) {
        let m = method.as_ptr();
        self.store_slot(m, METHOD_SELECTOR, selector);
        self.store_slot(m, METHOD_CLASS, class);
        // §8: every CompiledMethod registers its send-site table at
        // installation, so eager clearing reaches it.
        let nil = self.nil();
        let sites = self.heap.slot(m, METHOD_SEND_SITES);
        if sites.is_ptr() && sites != nil && !self.site_registry.contains(&sites) {
            self.site_registry.push(sites);
        }
        self.flush_caches();
    }

    /// Eager invalidation (§8): empty the global lookup cache and clear
    /// every registered send-site's inline cache.
    pub fn flush_caches(&mut self) {
        self.counters.cache_flushes += 1;
        for e in self.lookup_cache.iter_mut() {
            *e = LookupEntry::default();
        }
        let nil = self.nil();
        for i in 0..self.site_registry.len() {
            let sites = self.site_registry[i];
            let addr = sites.as_ptr();
            let n = self.heap.num_slots(addr) as usize / SITE_STRIDE;
            for s in 0..n {
                self.heap
                    .set_slot_raw(addr, s * SITE_STRIDE + SITE_CACHE_CLASS, Value::from_int(0));
                self.heap.set_slot_raw(addr, s * SITE_STRIDE + SITE_CACHE_METHOD, nil);
            }
        }
    }

    // --- Class construction (fixture / registerClass:) ---

    pub fn register_class(&mut self, class: Value) -> u32 {
        let idx = self.class_table.len() as u32;
        assert!((idx as u64) < (1 << HDR_CLASS_BITS));
        self.class_table.push(class);
        self.heap
            .set_slot_raw(class.as_ptr(), BEHAVIOR_CLASS_INDEX, Value::from_int(idx as i64));
        self.flush_caches();
        idx
    }

    pub fn new_test_class(&mut self, format: u64, nslots: usize) -> Value {
        let sup = self.class_table[CLASS_OBJECT as usize];
        self.new_test_subclass(sup, format, nslots)
    }

    pub fn new_test_subclass(&mut self, superclass: Value, format: u64, nslots: usize) -> Value {
        let nil = self.nil();
        let c = self.make_class_object(0, format, nslots, nil);
        self.heap
            .set_slot_raw(c.as_ptr(), BEHAVIOR_SUPERCLASS, superclass);
        self.register_class(c);
        c
    }

    // --- Instantiation ---

    pub fn make_instance(&mut self, class: Value) -> Result<Value, VmError> {
        let (format, nslots) = self.class_format_and_slots(class);
        let idx = self.heap.slot(class.as_ptr(), BEHAVIOR_CLASS_INDEX).as_int() as u32;
        let nil = self.nil();
        match format {
            FMT_FIXED => self
                .heap
                .alloc_fixed(idx, nslots, nil)
                .map(Value::from_ptr)
                .ok_or(VmError::OutOfMemory),
            _ => fatal("make_instance on non-fixed class; use make_instance_sized"),
        }
    }

    pub fn make_instance_sized(&mut self, class: Value, size: usize) -> Result<Value, VmError> {
        let (format, _) = self.class_format_and_slots(class);
        let idx = self.heap.slot(class.as_ptr(), BEHAVIOR_CLASS_INDEX).as_int() as u32;
        let nil = self.nil();
        let addr = match format {
            FMT_PTRS => self.heap.alloc_ptrs(idx, size, nil),
            f if f >= FMT_BYTES_BASE => self.heap.alloc_bytes(idx, size),
            _ => return fatal("make_instance_sized on fixed-format class"),
        };
        addr.map(Value::from_ptr).ok_or(VmError::OutOfMemory)
    }

    pub fn make_array(&mut self, items: &[Value]) -> Result<Value, VmError> {
        let nil = self.nil();
        let addr = self
            .heap
            .alloc_ptrs(CLASS_ARRAY, items.len(), nil)
            .ok_or(VmError::OutOfMemory)?;
        for (i, v) in items.iter().enumerate() {
            self.heap.set_slot_raw(addr, i, *v);
        }
        Ok(Value::from_ptr(addr))
    }

    pub fn make_string(&mut self, s: &str) -> Result<Value, VmError> {
        let addr = self
            .heap
            .alloc_bytes(CLASS_BYTESTRING, s.len())
            .ok_or(VmError::OutOfMemory)?;
        self.heap.write_bytes(addr, s.as_bytes());
        Ok(Value::from_ptr(addr))
    }

    pub fn make_float(&mut self, f: f64) -> Result<Value, VmError> {
        let addr = self
            .heap
            .alloc_bytes(CLASS_FLOAT, 8)
            .ok_or(VmError::OutOfMemory)?;
        self.heap.write_bytes(addr, &f.to_le_bytes());
        Ok(Value::from_ptr(addr))
    }

    pub fn float_value(&self, v: Value) -> f64 {
        f64::from_le_bytes(self.heap.bytes(v.as_ptr()).try_into().expect("8-byte float"))
    }

    // --- CompiledMethod field access ---

    pub fn method_header_bits(&self, method: Value) -> i64 {
        self.heap.slot(method.as_ptr(), METHOD_HEADER).as_int()
    }

    /// Bytecode-visible slot count (receiver + args + temps + scratch).
    /// The frame's total footprint is FRAME_RECEIVER + this.
    pub fn method_frame_slots(&self, method: Value) -> usize {
        ((self.method_header_bits(method) >> MH_FRAME_SLOTS_SHIFT) & 0xFF) as usize
    }

    pub fn method_argc(&self, method: Value) -> usize {
        ((self.method_header_bits(method) >> MH_ARGC_SHIFT) & 0xF) as usize
    }

    pub fn method_primitive(&self, method: Value) -> Option<u16> {
        let bits = self.method_header_bits(method);
        if (bits >> MH_HAS_PRIMITIVE_SHIFT) & 1 != 0 {
            Some(((bits >> MH_PRIMITIVE_SHIFT) & 0xFFF) as u16)
        } else {
            None
        }
    }

    pub fn method_handler_slot_base(&self, method: Value) -> usize {
        ((self.method_header_bits(method) >> MH_HANDLER_SLOT_BASE_SHIFT) & 0xFF) as usize
    }

    pub fn pack_method_header(
        frame_slots: usize,
        argc: usize,
        primitive: Option<u16>,
        handler_slot_base: usize,
        flags: u64,
    ) -> i64 {
        assert!(frame_slots <= MAX_FRAME_SLOTS && argc <= 15 && handler_slot_base <= 255);
        let (prim, has) = match primitive {
            Some(n) => {
                assert!((n as usize) < PRIM_TABLE_SIZE);
                (n as i64, 1i64)
            }
            None => (0, 0),
        };
        (frame_slots as i64)
            | ((argc as i64) << MH_ARGC_SHIFT)
            | (prim << MH_PRIMITIVE_SHIFT)
            | (has << MH_HAS_PRIMITIVE_SHIFT)
            | ((handler_slot_base as i64) << MH_HANDLER_SLOT_BASE_SHIFT)
            | ((flags as i64) << MH_FLAGS_SHIFT)
    }

    // --- Processes and stacks (§7) ---

    /// Allocation for API-level (non-interpreter) callers: collects on
    /// young exhaustion. In-flight Values must be rooted in temp_roots.
    fn alloc_api(&mut self, class: u32, format: u64, n: usize) -> Result<usize, VmError> {
        let nil = self.nil();
        let attempt = |heap: &mut Heap| match format {
            FMT_FIXED => heap.alloc_fixed(class, n, nil),
            FMT_PTRS => heap.alloc_ptrs(class, n, nil),
            _ => heap.alloc_bytes(class, n),
        };
        if let Some(a) = attempt(&mut self.heap) {
            return Ok(a);
        }
        self.collect_young()?;
        if let Some(a) = attempt(&mut self.heap) {
            return Ok(a);
        }
        self.collect_old()?;
        attempt(&mut self.heap).ok_or(VmError::OutOfMemory)
    }

    pub fn spawn_process(
        &mut self,
        method: Value,
        receiver: Value,
        args: &[Value],
    ) -> Result<Value, VmError> {
        let fs = self.method_frame_slots(method);
        assert_eq!(self.method_argc(method), args.len(), "argc mismatch");
        let stack_slots = INITIAL_STACK_BYTES / 8;
        assert!(STACK_FRAMES_BASE + FRAME_RECEIVER + fs <= stack_slots);

        // Root everything: either allocation below may collect.
        let base = self.temp_roots.len();
        self.temp_roots.push(method);
        self.temp_roots.push(receiver);
        self.temp_roots.extend_from_slice(args);

        let proc_addr = self.alloc_api(CLASS_PROCESS, FMT_FIXED, PROCESS_NUM_VM_SLOTS)?;
        self.temp_roots.push(Value::from_ptr(proc_addr));
        let stack_addr = self.alloc_api(CLASS_STACK, FMT_PTRS, stack_slots)?;

        let process = self.temp_roots.pop().unwrap();
        let proc_addr = process.as_ptr();
        let rooted: Vec<Value> = self.temp_roots.split_off(base);
        let method = rooted[0];
        let receiver = rooted[1];
        let args = &rooted[2..];
        let stack = Value::from_ptr(stack_addr);

        self.store_slot(stack_addr, STACK_OWNER, process);
        self.store_slot(proc_addr, PROCESS_STACK, stack);
        self.heap
            .set_slot_raw(proc_addr, PROCESS_FRAME_OFFSET, Value::from_int(STACK_FRAMES_BASE as i64));
        self.heap.set_slot_raw(proc_addr, PROCESS_PC, Value::from_int(0));
        self.heap
            .set_slot_raw(proc_addr, PROCESS_PRIORITY, Value::from_int(4));
        self.heap
            .set_slot_raw(proc_addr, PROCESS_SERIAL_COUNTER, Value::from_int(1));

        // The poised base frame.
        let f = STACK_FRAMES_BASE;
        self.heap
            .set_slot_raw(stack_addr, f + FRAME_CALLER, Value::from_int(0));
        self.heap
            .set_slot_raw(stack_addr, f + FRAME_RETINFO, Value::from_int(0));
        self.store_slot(stack_addr, f + FRAME_METHOD, method);
        self.heap
            .set_slot_raw(stack_addr, f + FRAME_FLAGS, Value::from_int(1 << SERIAL_SHIFT));
        self.store_slot(stack_addr, f + FRAME_RECEIVER, receiver);
        for (i, a) in args.iter().enumerate() {
            self.store_slot(stack_addr, f + FRAME_RECEIVER + 1 + i, *a);
        }
        Ok(process)
    }

    /// Convenience for tests: spawn a process for `method` and run it to
    /// completion of its base frame.
    pub fn call(&mut self, method: Value, receiver: Value, args: &[Value]) -> Result<Value, VmError> {
        let p = self.spawn_process(method, receiver, args)?;
        self.run(p)
    }

    // --- identityHash (§2) ---

    pub fn next_identity_hash(&mut self) -> u32 {
        loop {
            self.hash_counter = (self.hash_counter + 1) & ((1 << HDR_HASH_BITS) - 1);
            if self.hash_counter != 0 {
                return self.hash_counter;
            }
        }
    }

    pub fn identity_hash_of(&mut self, v: Value) -> i64 {
        if v.is_int() {
            return v.as_int();
        }
        let addr = v.as_ptr();
        let h = self.heap.header(addr);
        if h.hash() == 0 {
            let newh = self.next_identity_hash();
            self.heap.set_header(addr, h.with_hash(newh));
            newh as i64
        } else {
            h.hash() as i64
        }
    }

    /// Header word of an object — convenience for tests.
    pub fn header_of(&self, v: Value) -> Header {
        self.heap.header(v.as_ptr())
    }

}
