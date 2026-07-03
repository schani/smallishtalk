//! Exact VM counters (docs/profiling-plan.md §3): two tiers.
//!
//! **Always-on** counters are single u64 adds on paths that are already
//! slow (cache misses, primitive dispatch, GC, unwinding) — unmeasurable
//! against the work they sit on. **Gated** counters (per-opcode histogram,
//! instruction/send counts, inter-poll gap distribution) sit on the hottest
//! paths behind `counters.gate`, a mostly-false branch, and additionally
//! behind the `vm-counters` cargo feature so the cost of the gate itself
//! can be measured by building without it.
//!
//! `SMALLISHTALK_STATS=1` prints the full table to stderr when the VM is
//! dropped, so any run doubles as a measurement.

use crate::treaty::*;
use crate::vm::Vm;

/// Log2 bucket count for the inter-poll gap histogram.
pub const GAP_BUCKETS: usize = 40;

pub struct Counters {
    // --- Always-on: send machinery ---
    pub inline_cache_miss: u64,
    pub global_cache_miss: u64,
    pub dict_walks: u64,
    pub dict_classes_walked: u64,
    pub dnu: u64,
    pub must_be_boolean: u64,
    pub sends_staged: u64,
    /// Specialized-send slow-path entries, per specializedSelectors index.
    pub spec_fallthrough: [u64; SPECSEL_COUNT],
    // --- Always-on: primitives (indexed by primitive number) ---
    pub prim_calls: Box<[u64]>,
    pub prim_fails: Box<[u64]>,
    // --- Always-on: GC ---
    pub scavenge_ns: u64,
    pub compact_ns: u64,
    pub gc_bytes_copied: u64,
    pub gc_bytes_tenured: u64,
    pub gc_ssb_drained: u64,
    pub gc_ssb_drained_max: u64,
    pub gc_remembered_rebuilt: u64,
    // --- Always-on: control events ---
    pub process_switches: u64,
    pub semaphore_blocks: u64,
    pub unwind_runs: u64,
    pub ensure_interceptions: u64,
    pub nlrs: u64,
    pub cache_flushes: u64,
    pub method_installs: u64,
    // --- Always-on: UI work metrics (UI.md §13) ---
    // Deterministic under the virtual clock, so CI can assert budgets on them.
    pub bitblt_calls: u64,
    pub pixels_blitted: u64,
    pub glyphs_drawn: u64,
    pub events_processed: u64,
    pub frames_presented: u64,
    // --- Gated tier ---
    pub gate: bool,
    pub insns: u64,
    pub sends: u64,
    pub opcode_hist: Box<[u64]>,
    /// Instructions executed since the last safepoint poll, and the
    /// distribution thereof (log2 buckets) — this bounds the sampler's
    /// attribution error (plan §2).
    pub gap_current: u64,
    pub gap_max: u64,
    pub gap_hist: [u64; GAP_BUCKETS],
}

impl Counters {
    pub fn new() -> Counters {
        Counters {
            inline_cache_miss: 0,
            global_cache_miss: 0,
            dict_walks: 0,
            dict_classes_walked: 0,
            dnu: 0,
            must_be_boolean: 0,
            sends_staged: 0,
            spec_fallthrough: [0; SPECSEL_COUNT],
            prim_calls: vec![0; PRIM_TABLE_SIZE].into_boxed_slice(),
            prim_fails: vec![0; PRIM_TABLE_SIZE].into_boxed_slice(),
            scavenge_ns: 0,
            compact_ns: 0,
            gc_bytes_copied: 0,
            gc_bytes_tenured: 0,
            gc_ssb_drained: 0,
            gc_ssb_drained_max: 0,
            gc_remembered_rebuilt: 0,
            process_switches: 0,
            semaphore_blocks: 0,
            unwind_runs: 0,
            ensure_interceptions: 0,
            nlrs: 0,
            cache_flushes: 0,
            method_installs: 0,
            bitblt_calls: 0,
            pixels_blitted: 0,
            glyphs_drawn: 0,
            events_processed: 0,
            frames_presented: 0,
            gate: false,
            insns: 0,
            sends: 0,
            opcode_hist: vec![0; 256].into_boxed_slice(),
            gap_current: 0,
            gap_max: 0,
            gap_hist: [0; GAP_BUCKETS],
        }
    }

    /// Zero every numeric counter; the gate setting is preserved.
    pub fn reset(&mut self) {
        let gate = self.gate;
        *self = Counters::new();
        self.gate = gate;
    }

    /// Record a completed inter-poll instruction gap (gated tier).
    #[inline]
    pub fn record_gap(&mut self) {
        let g = self.gap_current;
        self.gap_current = 0;
        if g > self.gap_max {
            self.gap_max = g;
        }
        let bucket = (64 - g.leading_zeros() as usize).min(GAP_BUCKETS - 1);
        self.gap_hist[bucket] += 1;
    }
}

impl Default for Counters {
    fn default() -> Counters {
        Counters::new()
    }
}

pub fn opcode_name(op: u8) -> Option<&'static str> {
    Some(match op {
        OP_NOP => "NOP",
        OP_BREAK => "BREAK",
        OP_MOVE => "MOVE",
        OP_LOADK => "LOADK",
        OP_LOADINT => "LOADINT",
        OP_LOADNIL => "LOADNIL",
        OP_LOADTRUE => "LOADTRUE",
        OP_LOADFALSE => "LOADFALSE",
        OP_LOADSELF => "LOADSELF",
        OP_GETIVAR => "GETIVAR",
        OP_SETIVAR => "SETIVAR",
        OP_GETBOX => "GETBOX",
        OP_SETBOX => "SETBOX",
        OP_MKBOX => "MKBOX",
        OP_SEND => "SEND",
        OP_SENDSUPER => "SENDSUPER",
        OP_RET => "RET",
        OP_RETSELF => "RETSELF",
        OP_NLR => "NLR",
        OP_PRIM => "PRIM",
        OP_MKCLOSURE => "MKCLOSURE",
        OP_CAPTURE => "CAPTURE",
        OP_JUMP => "JUMP",
        OP_JUMPTRUE => "JUMPTRUE",
        OP_JUMPFALSE => "JUMPFALSE",
        OP_ADD => "ADD",
        OP_SUB => "SUB",
        OP_MUL => "MUL",
        OP_DIV => "DIV",
        OP_MOD => "MOD",
        OP_LT => "LT",
        OP_GT => "GT",
        OP_LE => "LE",
        OP_GE => "GE",
        OP_EQNUM => "EQNUM",
        OP_AT => "AT",
        OP_ATPUT => "ATPUT",
        OP_SIZE => "SIZE",
        OP_CLASSOF => "CLASSOF",
        OP_NOT => "NOT",
        OP_IDEQ => "IDEQ",
        _ => return None,
    })
}

const SPECSEL_NAMES: [&str; SPECSEL_COUNT] = [
    "+", "-", "*", "//", "\\\\", "<", ">", "<=", ">=", "=", "==",
    "at:", "at:put:", "size", "class", "not",
];

impl Vm {
    /// Every counter as a (name, value) row — the one table behind both the
    /// `SMALLISHTALK_STATS` dump and `primVmCounters`. Indexed counters
    /// (per-primitive, per-opcode) contribute only their nonzero rows.
    pub fn counter_rows(&self) -> Vec<(String, u64)> {
        fn r(rows: &mut Vec<(String, u64)>, name: &str, v: u64) {
            rows.push((name.to_string(), v));
        }
        let c = &self.counters;
        let mut rows: Vec<(String, u64)> = Vec::new();

        r(&mut rows, "send.inline_cache_miss", c.inline_cache_miss);
        r(&mut rows, "send.global_cache_miss", c.global_cache_miss);
        r(&mut rows, "send.dict_walks", c.dict_walks);
        r(&mut rows, "send.dict_classes_walked", c.dict_classes_walked);
        r(&mut rows, "send.dnu", c.dnu);
        r(&mut rows, "send.must_be_boolean", c.must_be_boolean);
        r(&mut rows, "send.staged", c.sends_staged);
        for (i, v) in c.spec_fallthrough.iter().enumerate() {
            if *v != 0 {
                rows.push((format!("specsend.fallthrough.{}", SPECSEL_NAMES[i]), *v));
            }
        }
        for (n, v) in c.prim_calls.iter().enumerate() {
            if *v != 0 {
                rows.push((format!("prim.calls.{n}"), *v));
            }
        }
        for (n, v) in c.prim_fails.iter().enumerate() {
            if *v != 0 {
                rows.push((format!("prim.fails.{n}"), *v));
            }
        }
        let h = &self.heap;
        rows.push(("alloc.young.count".into(), h.alloc_young_count));
        rows.push(("alloc.young.bytes".into(), h.alloc_young_bytes));
        rows.push(("alloc.old.count".into(), h.alloc_old_count));
        rows.push(("alloc.old.bytes".into(), h.alloc_old_bytes));
        rows.push(("alloc.large.count".into(), h.alloc_large_count));
        r(&mut rows, "gc.scavenge.count", self.scavenge_count);
        r(&mut rows, "gc.scavenge.ns", c.scavenge_ns);
        r(&mut rows, "gc.compact.count", self.compact_count);
        r(&mut rows, "gc.compact.ns", c.compact_ns);
        r(&mut rows, "gc.bytes_copied", c.gc_bytes_copied);
        r(&mut rows, "gc.bytes_tenured", c.gc_bytes_tenured);
        r(&mut rows, "gc.ssb_drained.total", c.gc_ssb_drained);
        r(&mut rows, "gc.ssb_drained.max", c.gc_ssb_drained_max);
        r(&mut rows, "gc.remembered_rebuilt", c.gc_remembered_rebuilt);
        r(&mut rows, "stack.growths", self.stack_grow_count);
        r(&mut rows, "process.switches", c.process_switches);
        r(&mut rows, "semaphore.blocks", c.semaphore_blocks);
        r(&mut rows, "unwind.runs", c.unwind_runs);
        r(&mut rows, "ensure.interceptions", c.ensure_interceptions);
        r(&mut rows, "nlr.count", c.nlrs);
        r(&mut rows, "cache.flushes", c.cache_flushes);
        r(&mut rows, "method.installs", c.method_installs);

        r(&mut rows, "ui.bitblt_calls", c.bitblt_calls);
        r(&mut rows, "ui.pixels_blitted", c.pixels_blitted);
        r(&mut rows, "ui.glyphs_drawn", c.glyphs_drawn);
        r(&mut rows, "ui.events_processed", c.events_processed);
        r(&mut rows, "ui.frames_presented", c.frames_presented);

        r(&mut rows, "gated.enabled", c.gate as u64);
        r(&mut rows, "insn.count", c.insns);
        r(&mut rows, "send.count", c.sends);
        for (op, v) in c.opcode_hist.iter().enumerate() {
            if *v != 0 {
                let name = opcode_name(op as u8).unwrap_or("ILLEGAL");
                rows.push((format!("opcode.{name}"), *v));
            }
        }
        r(&mut rows, "poll.gap_max", c.gap_max);
        for (i, v) in c.gap_hist.iter().enumerate() {
            if *v != 0 {
                // Bucket i holds gaps in [2^(i-1), 2^i); bucket 0 is gap 0.
                rows.push((format!("poll.gap_le_{}", if i == 0 { 0 } else { 1u64 << i }), *v));
            }
        }
        rows
    }

    pub fn format_stats(&self) -> String {
        let mut s = String::from("--- smallishtalk VM counters ---\n");
        let rows = self.counter_rows();
        let width = rows.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
        for (name, v) in rows {
            s.push_str(&format!("{name:width$}  {v}\n"));
        }
        s
    }

    pub fn reset_counters(&mut self) {
        self.counters.reset();
        self.heap.alloc_young_count = 0;
        self.heap.alloc_young_bytes = 0;
        self.heap.alloc_old_count = 0;
        self.heap.alloc_old_bytes = 0;
        self.heap.alloc_large_count = 0;
        self.scavenge_count = 0;
        self.compact_count = 0;
        self.stack_grow_count = 0;
    }
}

impl Drop for Vm {
    fn drop(&mut self) {
        // Stop (and join) the profiler timer thread if one is running.
        self.profiler_stop();
        if std::env::var_os("SMALLISHTALK_STATS").is_some_and(|v| v == "1") {
            eprint!("{}", self.format_stats());
        }
    }
}
