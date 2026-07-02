//! Counter exactness tests (profiling plan §3, §6): fixed hand-assembled
//! programs with known send/allocation/GC behavior must produce exact
//! counter values — an instrument that can't reproduce a known answer
//! doesn't get to produce unknown ones.

use smallishtalk::asm::Insn::*;
use smallishtalk::fixture::MethodBuilder;
use smallishtalk::heap::HeapConfig;
use smallishtalk::treaty::*;
use smallishtalk::value::Value;
use smallishtalk::vm::{Vm, VmConfig};

fn int(n: i64) -> Value {
    Value::from_int(n)
}

/// A class with an `answer` method returning 42, and a caller that sends
/// #answer to an instance `n` times through ONE send site.
fn n_sends_setup(vm: &mut Vm, n: usize) -> Value {
    let class = vm.new_test_class(FMT_FIXED, 0);
    let sel = vm.intern("answer");
    let answer = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 42 }, Ret { a: 1 }])
        .build(vm);
    vm.install_method(class, sel, answer);
    let obj = vm.make_instance(class).unwrap();
    let mut insns = Vec::new();
    for _ in 0..n {
        insns.push(LoadK { d: 8, k: 0 });
        insns.push(Send { d: 1, r: 8, site: 0 });
    }
    insns.push(Ret { a: 1 });
    MethodBuilder::new(0, 12)
        .insns(insns)
        .literals(vec![obj])
        .site_named(vm, "answer", 0)
        .build(vm)
}

#[cfg(feature = "vm-counters")]
#[test]
fn gated_send_and_insn_counts_are_exact() {
    let mut vm = Vm::bare_test();
    let caller = n_sends_setup(&mut vm, 5);
    vm.counters.gate = true;
    assert_eq!(vm.call(caller, vm.nil(), &[]).unwrap(), int(42));
    // Caller: 5×(LOADK + SEND) + RET = 11; callee: 5×(LOADINT + RET) = 10.
    assert_eq!(vm.counters.insns, 21, "exact instruction count");
    assert_eq!(vm.counters.sends, 5, "exact send count");
    assert_eq!(vm.counters.opcode_hist[OP_SEND as usize], 5);
    assert_eq!(vm.counters.opcode_hist[OP_LOADK as usize], 5);
    assert_eq!(vm.counters.opcode_hist[OP_RET as usize], 6);
}

#[test]
fn inline_cache_misses_once_then_hits() {
    let mut vm = Vm::bare_test();
    let caller = n_sends_setup(&mut vm, 5);
    assert_eq!(vm.call(caller, vm.nil(), &[]).unwrap(), int(42));
    assert_eq!(
        vm.counters.inline_cache_miss, 1,
        "one site: first send misses, four hit"
    );
    assert_eq!(vm.counters.global_cache_miss, 1, "one dictionary walk");
    assert_eq!(vm.counters.dict_walks, 1);
    assert!(vm.counters.dict_classes_walked >= 1);
}

#[test]
fn gate_off_counts_nothing_gated_but_slow_paths_still_count() {
    let mut vm = Vm::bare_test();
    let caller = n_sends_setup(&mut vm, 3);
    assert_eq!(vm.call(caller, vm.nil(), &[]).unwrap(), int(42));
    assert_eq!(vm.counters.insns, 0, "gated tier off by default");
    assert_eq!(vm.counters.sends, 0);
    assert_eq!(vm.counters.inline_cache_miss, 1, "always-on tier still counts");
}

#[test]
fn primitive_calls_and_failures_are_counted() {
    let mut vm = Vm::bare_test();
    let class = vm.new_test_class(FMT_FIXED, 0);
    // A method whose primitive (SmallInteger +) fails on a non-int
    // receiver and falls into the body.
    let sel = vm.intern("badadd:");
    let m = MethodBuilder::new(1, 4)
        .primitive(PRIM_INT_ADD)
        .insns(vec![LoadInt { d: 3, imm: -7 }, Ret { a: 3 }])
        .build(&mut vm);
    vm.install_method(class, sel, m);
    let obj = vm.make_instance(class).unwrap();
    let caller = MethodBuilder::new(0, 12)
        .insns(vec![
            LoadK { d: 8, k: 0 },
            LoadInt { d: 9, imm: 1 },
            Send { d: 1, r: 8, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![obj])
        .site_named(&mut vm, "badadd:", 1)
        .build(&mut vm);
    assert_eq!(vm.call(caller, vm.nil(), &[]).unwrap(), int(-7));
    assert_eq!(vm.counters.prim_calls[PRIM_INT_ADD as usize], 1);
    assert_eq!(vm.counters.prim_fails[PRIM_INT_ADD as usize], 1);
}

#[test]
fn allocation_counters_track_mkbox() {
    let mut vm = Vm::bare_test();
    let before_count = vm.heap.alloc_young_count;
    let before_bytes = vm.heap.alloc_young_bytes;
    let m = MethodBuilder::new(0, 6)
        .insns(vec![
            LoadInt { d: 1, imm: 1 },
            MkBox { d: 2, a: 1 },
            MkBox { d: 3, a: 1 },
            MkBox { d: 4, a: 1 },
            Ret { a: 1 },
        ])
        .build(&mut vm);
    vm.call(m, vm.nil(), &[]).unwrap();
    // Three boxes (1 header + 1 slot = 16 bytes each) plus the process +
    // stack spawned by call().
    assert!(vm.heap.alloc_young_count >= before_count + 3);
    assert!(vm.heap.alloc_young_bytes >= before_bytes + 3 * 16);
}

#[test]
fn gc_pause_and_copy_counters() {
    let mut vm = Vm::bare_test();
    // Keep something young alive via a temp root so the scavenge copies.
    let s = vm.make_string("young survivor").unwrap();
    vm.temp_roots.push(s);
    vm.collect_young().unwrap();
    vm.temp_roots.pop();
    assert_eq!(vm.scavenge_count, 1);
    assert!(vm.counters.scavenge_ns > 0, "pause time recorded");
    assert!(
        vm.counters.gc_bytes_copied >= 24,
        "the survivor was copied ({} bytes)",
        vm.counters.gc_bytes_copied
    );

    vm.collect_old().unwrap();
    assert_eq!(vm.compact_count, 1);
    assert!(vm.counters.compact_ns > 0);
}

#[test]
fn spec_fallthrough_counts_non_int_operands() {
    let mut vm = Vm::bare_test();
    // `'a' + 1` falls through OP_ADD's fast path to a real send of #+ —
    // give String a #+ method so the send lands somewhere.
    let string_class = vm.class_table_at(CLASS_BYTESTRING);
    let sel = vm.intern("+");
    let plus = MethodBuilder::new(1, 3)
        .insns(vec![LoadInt { d: 2, imm: 99 }, Ret { a: 2 }])
        .build(&mut vm);
    vm.install_method(string_class, sel, plus);
    let s = vm.make_string("a").unwrap();
    let m = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 1, k: 0 },
            LoadInt { d: 2, imm: 1 },
            Add { d: 3, a: 1, b: 2 },
            Ret { a: 3 },
        ])
        .literals(vec![s])
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), int(99));
    assert_eq!(vm.counters.spec_fallthrough[SPECSEL_PLUS], 1);
    assert_eq!(vm.counters.sends_staged, 1);
}

#[test]
fn dnu_counter() {
    let mut vm = Vm::bare_test();
    // Install a doesNotUnderstand: handler on Object that answers nil.
    let object = vm.class_table_at(CLASS_OBJECT);
    let dnu_sel = vm.intern("doesNotUnderstand:");
    let handler = MethodBuilder::new(1, 3)
        .insns(vec![LoadNil { d: 2 }, Ret { a: 2 }])
        .build(&mut vm);
    vm.install_method(object, dnu_sel, handler);
    let caller = MethodBuilder::new(0, 12)
        .insns(vec![
            LoadInt { d: 8, imm: 7 },
            Send { d: 1, r: 8, site: 0 },
            Ret { a: 1 },
        ])
        .site_named(&mut vm, "noSuchSelector", 0)
        .build(&mut vm);
    assert_eq!(vm.call(caller, vm.nil(), &[]).unwrap(), vm.nil());
    assert_eq!(vm.counters.dnu, 1);
}

#[cfg(feature = "vm-counters")]
#[test]
fn gap_distribution_recorded_under_gate() {
    let mut vm = Vm::bare_test();
    // A counted loop: backward jumps poll, so gaps get recorded.
    let m = MethodBuilder::new(0, 6)
        .insns(vec![
            LoadInt { d: 1, imm: 0 },
            LoadInt { d: 2, imm: 100 },
            LoadInt { d: 3, imm: 1 },
            // loop: i < 100?
            Lt { d: 4, a: 1, b: 2 },
            JumpFalse { a: 4, off: 2 },
            Add { d: 1, a: 1, b: 3 },
            Jump { off: -4 },
            Ret { a: 1 },
        ])
        .build(&mut vm);
    vm.counters.gate = true;
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), int(100));
    assert!(vm.counters.gap_max >= 3, "loop body is ≥3 insns between polls");
    let total_gaps: u64 = vm.counters.gap_hist.iter().sum();
    assert!(total_gaps >= 100, "one gap per backward jump");
}

#[test]
fn counter_rows_and_reset() {
    let mut vm = Vm::bare_test();
    let caller = n_sends_setup(&mut vm, 2);
    vm.counters.gate = true;
    vm.call(caller, vm.nil(), &[]).unwrap();
    let rows = vm.counter_rows();
    let get = |name: &str| {
        rows.iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("row {name} missing"))
            .1
    };
    #[cfg(feature = "vm-counters")]
    assert_eq!(get("send.count"), 2);
    assert_eq!(get("send.inline_cache_miss"), 1);
    assert!(get("alloc.young.count") > 0);
    assert!(get("method.installs") >= 1);
    // Reset zeroes the numbers but keeps the gate.
    vm.reset_counters();
    assert!(vm.counters.gate);
    assert_eq!(vm.counters.sends, 0);
    assert_eq!(vm.heap.alloc_young_count, 0);
    assert_eq!(vm.counters.inline_cache_miss, 0);
}

#[test]
fn gc_stress_run_keeps_counters_consistent() {
    // Tiny young space: the loop churns boxes, forcing scavenges; the GC
    // counters must move together (count, pause time, bytes).
    let mut vm = Vm::bare(VmConfig {
        heap: HeapConfig {
            young_bytes: 16 * 1024,
            old_bytes: 8 * 1024 * 1024,
            ..HeapConfig::default()
        },
        max_stack_bytes: DEFAULT_MAX_STACK_BYTES,
    });
    let m = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadInt { d: 1, imm: 0 },
            LoadInt { d: 2, imm: 5000 },
            LoadInt { d: 3, imm: 1 },
            Lt { d: 4, a: 1, b: 2 },
            JumpFalse { a: 4, off: 3 },
            MkBox { d: 5, a: 1 },
            Add { d: 1, a: 1, b: 3 },
            Jump { off: -5 },
            Ret { a: 1 },
        ])
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), int(5000));
    assert!(vm.scavenge_count > 0, "churn must have scavenged");
    assert!(vm.counters.scavenge_ns > 0);
    assert!(vm.heap.alloc_young_count >= 5000);
}
