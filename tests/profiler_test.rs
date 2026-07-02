//! Sampling-profiler tests (profiling plan §6): deterministic sampling via
//! the force-sample hook and the sample-every-poll stress mode, pseudo-frame
//! attribution for GC and primitives, the timer integration, and the
//! truth-source 90/10 cross-validation.

use smallishtalk::asm::Insn::*;
use smallishtalk::fixture::MethodBuilder;
use smallishtalk::heap::HeapConfig;
use smallishtalk::treaty::*;
use smallishtalk::value::Value;
use smallishtalk::vm::{Vm, VmConfig};

fn int(n: i64) -> Value {
    Value::from_int(n)
}

/// A method that runs `n` iterations of a counted loop — each backward
/// jump is a safepoint poll.
fn loop_method(n: i16) -> MethodBuilder {
    MethodBuilder::new(0, 6).insns(vec![
        LoadInt { d: 1, imm: 0 },
        LoadInt { d: 2, imm: n },
        LoadInt { d: 3, imm: 1 },
        Lt { d: 4, a: 1, b: 2 },
        JumpFalse { a: 4, off: 2 },
        Add { d: 1, a: 1, b: 3 },
        Jump { off: -4 },
        Ret { a: 1 },
    ])
}

/// Install `sel` on a fresh class and return a caller that sends it to an
/// instance.
fn install_and_call_two(vm: &mut Vm, a_iters: i16, b_iters: i16) -> Value {
    let class = vm.new_test_class(FMT_FIXED, 0);
    let alpha = loop_method(a_iters).build(vm);
    let beta = loop_method(b_iters).build(vm);
    let sel_a = vm.intern("alphaSpin");
    let sel_b = vm.intern("betaSpin");
    vm.install_method(class, sel_a, alpha);
    vm.install_method(class, sel_b, beta);
    let obj = vm.make_instance(class).unwrap();
    MethodBuilder::new(0, 12)
        .insns(vec![
            LoadK { d: 8, k: 0 },
            Send { d: 1, r: 8, site: 0 },
            LoadK { d: 8, k: 0 },
            Send { d: 1, r: 8, site: 1 },
            Ret { a: 1 },
        ])
        .literals(vec![obj])
        .site_named(vm, "alphaSpin", 0)
        .site_named(vm, "betaSpin", 0)
        .build(vm)
}

fn rows_by_name(vm: &Vm) -> std::collections::HashMap<String, (u64, u64)> {
    vm.profiler_report_rows()
        .into_iter()
        .map(|(n, s, t)| (n, (s, t)))
        .collect()
}

#[test]
fn force_sample_records_one_sample_with_the_full_stack() {
    let mut vm = Vm::bare_test();
    let caller = install_and_call_two(&mut vm, 50, 50);
    vm.force_sample_at_next_poll();
    assert_eq!(vm.call(caller, vm.nil(), &[]).unwrap(), int(50));
    assert_eq!(vm.profiler.total_samples, 1, "exactly one forced sample");
    let rows = rows_by_name(&vm);
    // The first poll is the first send (leaf = the caller method itself);
    // the caller must appear with self == total == 1.
    let sum_self: u64 = rows.values().map(|(s, _)| *s).sum();
    assert_eq!(sum_self, 1);
}

#[test]
fn stress_mode_samples_split_ninety_ten() {
    // Truth-source test (plan §5): a workload spending 90/10 of its polls
    // in two methods must tally 90/10 — with sample-every-poll this is
    // exact, not statistical.
    let mut vm = Vm::bare_test();
    let caller = install_and_call_two(&mut vm, 90, 10);
    vm.profiler.active = true;
    vm.profiler.sample_every_poll = true;
    assert_eq!(vm.call(caller, vm.nil(), &[]).unwrap(), int(10));
    vm.profiler.active = false;
    let rows = rows_by_name(&vm);
    let alpha = rows
        .iter()
        .find(|(n, _)| n.contains("alphaSpin"))
        .map(|(_, v)| *v)
        .expect("alphaSpin sampled");
    let beta = rows
        .iter()
        .find(|(n, _)| n.contains("betaSpin"))
        .map(|(_, v)| *v)
        .expect("betaSpin sampled");
    assert_eq!(alpha.0, 90, "one leaf sample per alpha backward jump");
    assert_eq!(beta.0, 10);
    // Self samples across all rows equal total samples.
    let sum_self: u64 = rows.values().map(|(s, _)| *s).sum();
    assert_eq!(sum_self, vm.profiler.total_samples);
    // The caller is on-stack for every alpha/beta sample.
    let caller_total = rows
        .iter()
        .find(|(n, _)| n.ends_with(">>doIt"))
        .map(|(_, v)| v.1)
        .expect("caller on stack");
    assert!(caller_total >= 100);
}

#[test]
fn block_frames_render_as_brackets_in() {
    let mut vm = Vm::bare_test();
    // BlockClosure>>value via its primitive.
    let bc = vm.class_table_at(CLASS_BLOCKCLOSURE);
    let value_m = MethodBuilder::new(0, 3)
        .primitive(PRIM_BLOCK_VALUE_0)
        .insns(vec![LoadInt { d: 1, imm: -77 }, Ret { a: 1 }])
        .build(&mut vm);
    let value_sel = vm.intern("value");
    vm.install_method(bc, value_sel, value_m);

    // outer method: [ ...loop... ] value — the block loops, so samples
    // land inside the block activation.
    let class = vm.new_test_class(FMT_FIXED, 0);
    let outer_placeholder = MethodBuilder::new(0, 6)
        .insns(vec![
            MkClosure { d: 4, b: 0 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .site_named(&mut vm, "value", 0);
    // Build the block with a real outer so the name renders "[] in ...".
    // (Fixture ordering: outer needs the block as a literal, so install a
    // stamped outer first, then reuse it as the block's home.)
    let outer_stub = MethodBuilder::new(0, 2)
        .insns(vec![RetSelf])
        .build(&mut vm);
    let sel = vm.intern("runBlock");
    vm.install_method(class, sel, outer_stub);
    let blk = loop_method(20).build_block(&mut vm, outer_stub, 0, false);
    let outer = outer_placeholder.literals(vec![blk]).build(&mut vm);
    vm.install_method(class, sel, outer);

    let obj = vm.make_instance(class).unwrap();
    let caller = MethodBuilder::new(0, 12)
        .insns(vec![
            LoadK { d: 8, k: 0 },
            Send { d: 1, r: 8, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![obj])
        .site_named(&mut vm, "runBlock", 0)
        .build(&mut vm);

    vm.profiler.active = true;
    vm.profiler.sample_every_poll = true;
    vm.call(caller, vm.nil(), &[]).unwrap();
    vm.profiler.active = false;
    let rows = rows_by_name(&vm);
    let block_row = rows.iter().find(|(n, _)| n.starts_with("[] in "));
    assert!(
        block_row.is_some(),
        "block frames must render as '[] in ...': {:?}",
        rows.keys().collect::<Vec<_>>()
    );
    assert!(block_row.unwrap().1 .0 >= 20, "block loop polls sampled");
}

#[test]
fn gc_pseudo_frames_appear_under_allocation_churn() {
    let mut vm = Vm::bare(VmConfig {
        heap: HeapConfig {
            young_bytes: 16 * 1024,
            old_bytes: 8 * 1024 * 1024,
            ..HeapConfig::default()
        },
        max_stack_bytes: DEFAULT_MAX_STACK_BYTES,
    });
    let class = vm.new_test_class(FMT_FIXED, 0);
    let churn = MethodBuilder::new(0, 8)
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
    let sel = vm.intern("churn");
    vm.install_method(class, sel, churn);
    let obj = vm.make_instance(class).unwrap();
    let caller = MethodBuilder::new(0, 12)
        .insns(vec![
            LoadK { d: 8, k: 0 },
            Send { d: 1, r: 8, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![obj])
        .site_named(&mut vm, "churn", 0)
        .build(&mut vm);
    vm.profiler.active = true;
    vm.profiler.sample_every_poll = true;
    assert_eq!(vm.call(caller, vm.nil(), &[]).unwrap(), int(5000));
    vm.profiler.active = false;
    assert!(vm.scavenge_count > 0, "churn must scavenge");
    let rows = rows_by_name(&vm);
    let (s, t) = rows
        .get("<vm:scavenge>")
        .copied()
        .expect("scavenge pseudo-frame recorded");
    assert!(s > 0 && t > 0);
    // Attribution: the churn method is on-stack under the scavenge leaf,
    // i.e. some recorded path contains both ids.
    let names = vm.profiler.names();
    let scavenge_id = names.iter().position(|n| n == "<vm:scavenge>").unwrap() as u32;
    let churn_id = names
        .iter()
        .position(|n| n.contains(">>churn"))
        .unwrap() as u32;
    let attributed = vm
        .profiler
        .paths
        .keys()
        .any(|p| p.contains(&scavenge_id) && p.contains(&churn_id));
    assert!(attributed, "GC samples carry the triggering Smalltalk stack");
}

#[test]
fn primitive_pseudo_frames_appear() {
    let mut vm = Vm::bare_test();
    let class = vm.new_test_class(FMT_FIXED, 0);
    let hash_m = MethodBuilder::new(0, 3)
        .primitive(PRIM_IDENTITY_HASH)
        .insns(vec![LoadInt { d: 1, imm: -1 }, Ret { a: 1 }])
        .build(&mut vm);
    let sel = vm.intern("hash");
    vm.install_method(class, sel, hash_m);
    let obj = vm.make_instance(class).unwrap();
    let caller = MethodBuilder::new(0, 12)
        .insns(vec![
            LoadK { d: 8, k: 0 },
            Send { d: 1, r: 8, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![obj])
        .site_named(&mut vm, "hash", 0)
        .build(&mut vm);
    vm.profiler.active = true;
    vm.profiler.sample_every_poll = true;
    vm.call(caller, vm.nil(), &[]).unwrap();
    vm.profiler.active = false;
    let rows = rows_by_name(&vm);
    let key = format!("<vm:prim:{PRIM_IDENTITY_HASH}>");
    assert!(
        rows.contains_key(&key),
        "prim pseudo-frame recorded: {:?}",
        rows.keys().collect::<Vec<_>>()
    );
}

#[test]
fn timer_thread_samples_a_spinning_loop_and_stops_cleanly() {
    let mut vm = Vm::bare_test();
    // ~3M iterations: long enough to catch ≥1 sample at 1 ms in any build.
    let spin = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadInt { d: 1, imm: 0 },
            LoadInt { d: 2, imm: 3000 },
            LoadInt { d: 3, imm: 1 },
            LoadInt { d: 5, imm: 0 },
            LoadInt { d: 6, imm: 1000 },
            // outer: i < 3000?
            Lt { d: 4, a: 1, b: 2 },
            JumpFalse { a: 4, off: 7 },
            // inner: j < 1000?
            LoadInt { d: 5, imm: 0 },
            Lt { d: 7, a: 5, b: 6 },
            JumpFalse { a: 7, off: 2 },
            Add { d: 5, a: 5, b: 3 },
            Jump { off: -4 },
            Add { d: 1, a: 1, b: 3 },
            Jump { off: -9 },
            Ret { a: 1 },
        ])
        .build(&mut vm);
    vm.profiler_start(1);
    assert_eq!(vm.call(spin, vm.nil(), &[]).unwrap(), int(3000));
    vm.profiler_stop();
    assert!(
        vm.profiler.total_samples > 0,
        "a 1ms timer must catch a multi-million-insn loop"
    );
    let sum_self: u64 = vm.profiler.flat_self.iter().sum();
    assert_eq!(sum_self, vm.profiler.total_samples);
    // Stopping again is idempotent; restarting resets the store.
    vm.profiler_stop();
    vm.profiler_start(1);
    assert_eq!(vm.profiler.total_samples, 0);
    vm.profiler_stop();
}

#[test]
fn profiler_off_changes_nothing() {
    // Determinism guard: with the profiler never started, a run must
    // record no samples and leave no store behind.
    let mut vm = Vm::bare_test();
    let caller = install_and_call_two(&mut vm, 50, 50);
    assert_eq!(vm.call(caller, vm.nil(), &[]).unwrap(), int(50));
    assert_eq!(vm.profiler.total_samples, 0);
    assert!(vm.profiler_report_rows().is_empty());
}
