//! The sampling profiler (docs/profiling-plan.md §2): statistical stack
//! sampling at safepoints, MessageTally-style, driven by a host timer
//! thread through the existing safepoint poll — zero new hot-path cost.
//!
//! Design constraints honored here:
//!
//! - **No GC interaction.** The sample store holds interned small-integer
//!   ids and Rust strings, never heap `Value`s. Method addresses are only
//!   used as cache keys, invalidated wholesale on every `gc_epoch` change,
//!   so a moving collection can never leave a stale address in play.
//! - **Walkability contract.** At every safepoint the frame chain is
//!   consistent (caller links are SmallIntegers chaining to the base
//!   frame; every stack word is a valid tagged value). The walker relies
//!   on exactly that and nothing more. `sample_every_poll` (stress mode)
//!   forces a sample at every poll so the whole corpus enforces the
//!   contract.
//! - **VM-time attribution.** GC entry points and the primitive
//!   dispatcher check the sample flag at their own boundaries and record
//!   pseudo-leaves (`<vm:scavenge>`, `<vm:compact>`, `<vm:prim:N>`) on top
//!   of the live Smalltalk stack.

use crate::treaty::*;
use crate::value::Value;
use crate::vm::Vm;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Frame-walk depth bound per sample.
pub const MAX_SAMPLE_DEPTH: usize = 64;

/// Shared with the timer thread: the interpreter's safepoint poll reads
/// `armed`; the timer thread sets `sample_due` + `armed` every interval.
pub struct SafepointShared {
    pub armed: AtomicBool,
    pub sample_due: AtomicBool,
}

impl SafepointShared {
    pub fn new() -> Arc<SafepointShared> {
        Arc::new(SafepointShared {
            armed: AtomicBool::new(false),
            sample_due: AtomicBool::new(false),
        })
    }
}

struct ProfTimer {
    stop: Arc<AtomicBool>,
    join: std::thread::JoinHandle<()>,
}

#[derive(Default)]
pub struct Profiler {
    pub active: bool,
    /// Stress/test mode: take a sample at every safepoint poll, GC entry,
    /// and primitive dispatch (deterministic, no timer needed).
    pub sample_every_poll: bool,
    pub interval_ms: u64,
    pub total_samples: u64,
    /// Interned display names; the canonical sample key. Ids are indices.
    names: Vec<String>,
    name_ids: HashMap<String, u32>,
    /// Fast path: method address -> id, valid only for `cache_epoch`.
    addr_cache: HashMap<usize, u32>,
    cache_epoch: u64,
    /// Leaf (self) and anywhere-on-stack (total) tallies, indexed by id.
    pub flat_self: Vec<u64>,
    pub flat_total: Vec<u64>,
    /// Full-path tally (root-first), for the future tree report.
    pub paths: HashMap<Box<[u32]>, u64>,
    timer: Option<ProfTimer>,
}

impl Profiler {
    pub fn reset_store(&mut self) {
        self.total_samples = 0;
        self.names.clear();
        self.name_ids.clear();
        self.addr_cache.clear();
        self.flat_self.clear();
        self.flat_total.clear();
        self.paths.clear();
    }

    pub fn name_of(&self, id: u32) -> &str {
        &self.names[id as usize]
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    fn intern(&mut self, name: String) -> u32 {
        if let Some(&id) = self.name_ids.get(&name) {
            return id;
        }
        let id = self.names.len() as u32;
        self.name_ids.insert(name.clone(), id);
        self.names.push(name);
        self.flat_self.push(0);
        self.flat_total.push(0);
        id
    }
}

/// Human names for the bootstrap (Rust-made) classes, which have no image
/// `name` ivar.
fn treaty_class_name(idx: u32) -> Option<&'static str> {
    Some(match idx {
        CLASS_OBJECT => "Object",
        CLASS_BEHAVIOR => "Behavior",
        CLASS_CLASS => "Class",
        CLASS_METACLASS => "Metaclass",
        CLASS_UNDEFINED_OBJECT => "UndefinedObject",
        CLASS_TRUE => "True",
        CLASS_FALSE => "False",
        CLASS_SMALLINTEGER => "SmallInteger",
        CLASS_FLOAT => "Float",
        CLASS_CHARACTER => "Character",
        CLASS_STRING => "String",
        CLASS_BYTESTRING => "ByteString",
        CLASS_SYMBOL => "Symbol",
        CLASS_ARRAY => "Array",
        CLASS_BYTEARRAY => "ByteArray",
        CLASS_ORDERED_COLLECTION => "OrderedCollection",
        CLASS_ASSOCIATION => "Association",
        CLASS_BLOCKCLOSURE => "BlockClosure",
        CLASS_COMPILEDMETHOD => "CompiledMethod",
        CLASS_COMPILEDBLOCK => "CompiledBlock",
        CLASS_PROCESS => "Process",
        CLASS_SEMAPHORE => "Semaphore",
        CLASS_METHODDICTIONARY => "MethodDictionary",
        CLASS_PROCESSOR_SCHEDULER => "ProcessorScheduler",
        CLASS_MESSAGE => "Message",
        CLASS_SYSTEM_DICTIONARY => "SystemDictionary",
        _ => return None,
    })
}

impl Vm {
    // --- Start / stop ---

    /// Start sampling at the given interval, resetting the sample store.
    pub fn profiler_start(&mut self, interval_ms: u64) {
        self.profiler_stop();
        self.profiler.reset_store();
        self.profiler.active = true;
        self.profiler.interval_ms = interval_ms.max(1);
        let stop = Arc::new(AtomicBool::new(false));
        let sp = Arc::clone(&self.safepoint);
        let stop2 = Arc::clone(&stop);
        let interval = std::time::Duration::from_millis(self.profiler.interval_ms);
        let join = std::thread::spawn(move || loop {
            std::thread::sleep(interval);
            if stop2.load(Ordering::Relaxed) {
                break;
            }
            sp.sample_due.store(true, Ordering::Relaxed);
            sp.armed.store(true, Ordering::Release);
        });
        self.profiler.timer = Some(ProfTimer { stop, join });
    }

    /// Stop the timer; sampling ceases. The store stays for reporting.
    pub fn profiler_stop(&mut self) {
        self.profiler.active = false;
        if let Some(t) = self.profiler.timer.take() {
            t.stop.store(true, Ordering::Relaxed);
            let _ = t.join.join();
        }
        self.safepoint.sample_due.store(false, Ordering::Relaxed);
    }

    /// Test hook (plan §6): the next safepoint poll takes exactly one
    /// sample, no timer involved.
    pub fn force_sample_at_next_poll(&mut self) {
        self.profiler.active = true;
        self.safepoint.sample_due.store(true, Ordering::Relaxed);
        self.safepoint.armed.store(true, Ordering::Relaxed);
    }

    /// True when a sample is due at a VM boundary (GC entry, primitive
    /// dispatch): consumes the flag.
    #[inline]
    pub(crate) fn sample_due_here(&mut self) -> bool {
        self.profiler.active
            && (self.safepoint.sample_due.swap(false, Ordering::Relaxed)
                || self.profiler.sample_every_poll)
    }

    // --- Taking samples ---

    /// Walk the frame chain starting at (stack, frame) — which must be a
    /// consistent chain per the walkability contract — and record the
    /// sample, with an optional VM pseudo-leaf on top.
    pub fn take_sample(&mut self, stack: usize, frame: usize, pseudo: Option<&str>) {
        let mut path: Vec<u32> = Vec::with_capacity(MAX_SAMPLE_DEPTH + 1);
        if let Some(p) = pseudo {
            let id = self.profiler.intern(p.to_string());
            path.push(id);
        }
        let mut off = frame;
        for _ in 0..MAX_SAMPLE_DEPTH {
            let method = self.heap.slot(stack, off + FRAME_METHOD);
            let id = self.method_sample_id(method);
            path.push(id);
            let caller = self.heap.slot(stack, off + FRAME_CALLER);
            if !caller.is_int() || caller.as_int() == 0 {
                break;
            }
            off = caller.as_int() as usize;
        }
        self.record_sample(path);
    }

    /// Sample from the active process's *saved* state — used at GC entry,
    /// where `save_regs` has already run (every collection is preceded by
    /// one). Falls back to a bare pseudo-leaf when no process is running.
    pub fn take_sample_from_process(&mut self, pseudo: &str) {
        let nil = self.nil();
        let p = self.active_process;
        if p.is_ptr() && p != nil {
            let stack = self.heap.slot(p.as_ptr(), PROCESS_STACK);
            let off = self.heap.slot(p.as_ptr(), PROCESS_FRAME_OFFSET);
            if stack.is_ptr() && stack != nil && off.is_int() {
                self.take_sample(stack.as_ptr(), off.as_int() as usize, Some(pseudo));
                return;
            }
        }
        let id = self.profiler.intern(pseudo.to_string());
        self.record_sample(vec![id]);
    }

    fn record_sample(&mut self, path: Vec<u32>) {
        let p = &mut self.profiler;
        p.total_samples += 1;
        if let Some(&leaf) = path.first() {
            p.flat_self[leaf as usize] += 1;
        }
        let mut uniq = path.clone();
        uniq.sort_unstable();
        uniq.dedup();
        for id in uniq {
            p.flat_total[id as usize] += 1;
        }
        let mut root_first = path;
        root_first.reverse();
        *p.paths.entry(root_first.into_boxed_slice()).or_insert(0) += 1;
    }

    // --- Symbolization ---

    /// Resolve a frame's method to its interned sample id: a hash hit on
    /// the (address, epoch) cache in steady state; the display name is
    /// built once per method per GC epoch (plan §2).
    fn method_sample_id(&mut self, method: Value) -> u32 {
        if self.profiler.cache_epoch != self.gc_epoch {
            self.profiler.addr_cache.clear();
            self.profiler.cache_epoch = self.gc_epoch;
        }
        if method.is_ptr() {
            if let Some(&id) = self.profiler.addr_cache.get(&method.as_ptr()) {
                return id;
            }
        }
        let name = self.method_display_name(method, 0);
        let id = self.profiler.intern(name);
        if method.is_ptr() {
            self.profiler.addr_cache.insert(method.as_ptr(), id);
        }
        id
    }

    fn method_display_name(&self, method: Value, depth: usize) -> String {
        let nil = self.nil();
        if !method.is_ptr() || method == nil || depth > 4 {
            return "<invalid-frame>".to_string();
        }
        let h = self.heap.header(method.as_ptr());
        match h.class_index() {
            CLASS_COMPILEDBLOCK => {
                let outer = self.heap.slot(method.as_ptr(), BLOCK_OUTER_METHOD);
                format!("[] in {}", self.method_display_name(outer, depth + 1))
            }
            CLASS_COMPILEDMETHOD => {
                let sel = self.heap.slot(method.as_ptr(), METHOD_SELECTOR);
                let cls = self.heap.slot(method.as_ptr(), METHOD_CLASS);
                let sel_s = if sel.is_ptr()
                    && sel != nil
                    && self.heap.header(sel.as_ptr()).is_bytes()
                {
                    String::from_utf8_lossy(self.heap.bytes(sel.as_ptr())).into_owned()
                } else {
                    "doIt".to_string()
                };
                format!("{}>>{}", self.class_display_name(cls), sel_s)
            }
            _ => "<invalid-frame>".to_string(),
        }
    }

    /// A class's display name: the image `name` ivar (first slot after the
    /// four VM slots) when present, else the Treaty name for bootstrap
    /// classes, else the class index.
    pub fn class_display_name(&self, cls: Value) -> String {
        let nil = self.nil();
        if !cls.is_ptr() || cls == nil {
            return "?".to_string();
        }
        let h = self.heap.header(cls.as_ptr());
        if h.is_bytes() {
            return "?".to_string();
        }
        let n = self.heap.num_slots(cls.as_ptr()) as usize;
        if n > BEHAVIOR_NUM_VM_SLOTS {
            // The image stores the name (a Symbol) in the first slot after
            // the VM slots; metaclasses are already named 'Foo class'.
            let name = self.heap.slot(cls.as_ptr(), BEHAVIOR_NUM_VM_SLOTS);
            if name.is_ptr() && name != nil && self.heap.header(name.as_ptr()).is_bytes() {
                return String::from_utf8_lossy(self.heap.bytes(name.as_ptr())).into_owned();
            }
        }
        if n > BEHAVIOR_CLASS_INDEX {
            let idx = self.heap.slot(cls.as_ptr(), BEHAVIOR_CLASS_INDEX);
            if idx.is_int() {
                let i = idx.as_int() as u32;
                return treaty_class_name(i)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("class#{i}"));
            }
        }
        "?".to_string()
    }

    // --- Reporting ---

    /// Flat report rows `(name, selfSamples, totalSamples)`, sorted by
    /// self-samples descending (the Smalltalk side prints them verbatim).
    pub fn profiler_report_rows(&self) -> Vec<(String, u64, u64)> {
        let p = &self.profiler;
        let mut rows: Vec<(String, u64, u64)> = p
            .names
            .iter()
            .enumerate()
            .filter(|(i, _)| p.flat_self[*i] != 0 || p.flat_total[*i] != 0)
            .map(|(i, n)| (n.clone(), p.flat_self[i], p.flat_total[i]))
            .collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)).then(a.0.cmp(&b.0)));
        rows
    }
}
