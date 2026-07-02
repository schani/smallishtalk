//! Phase 1 GC tests (SPEC §14, §20): hand-built heaps, scavenge, forwarding,
//! SSB drain, tenuring, and collection under a running interpreter.

use smallishtalk::asm::Insn::*;
use smallishtalk::fixture::MethodBuilder;
use smallishtalk::heap::HeapConfig;
use smallishtalk::treaty::*;
use smallishtalk::value::Value;
use smallishtalk::vm::{Vm, VmConfig};

fn int(n: i64) -> Value {
    Value::from_int(n)
}

fn small_vm() -> Vm {
    Vm::bare(VmConfig {
        heap: HeapConfig {
            young_bytes: 64 * 1024,
            // Roomy: grown stacks land in old space and stay until
            // mark-compact (a later phase) reclaims them.
            old_bytes: 32 * 1024 * 1024,
            ..HeapConfig::default()
        },
        ..VmConfig::default()
    })
}

#[test]
fn scavenge_moves_live_and_drops_dead() {
    let mut vm = small_vm();
    let live = vm.make_array(&[int(1), int(2), int(3)]).unwrap();
    let _dead = vm.make_array(&[int(9); 100]).unwrap();
    let used_before = vm.heap.young_from.used_bytes();

    vm.temp_roots.push(live);
    vm.collect_young().unwrap();
    let live2 = vm.temp_roots.pop().unwrap();

    assert_ne!(live, live2, "live object must have moved");
    assert!(vm.heap.is_young(live2.as_ptr()));
    assert_eq!(vm.heap.num_slots(live2.as_ptr()), 3);
    assert_eq!(vm.heap.slot(live2.as_ptr(), 0), int(1));
    assert_eq!(vm.heap.slot(live2.as_ptr(), 2), int(3));
    assert!(
        vm.heap.young_from.used_bytes() < used_before,
        "dead object reclaimed"
    );
}

#[test]
fn forwarding_shares_single_copy() {
    let mut vm = small_vm();
    let obj = vm.make_array(&[int(7)]).unwrap();
    let holder = vm.make_array(&[obj, obj]).unwrap();
    vm.temp_roots.push(holder);
    vm.temp_roots.push(obj);
    vm.collect_young().unwrap();
    let obj2 = vm.temp_roots.pop().unwrap();
    let holder2 = vm.temp_roots.pop().unwrap();
    assert_eq!(vm.heap.slot(holder2.as_ptr(), 0), obj2);
    assert_eq!(vm.heap.slot(holder2.as_ptr(), 1), obj2);
}

#[test]
fn cycles_survive() {
    let mut vm = small_vm();
    let a = vm.make_array(&[vm.nil()]).unwrap();
    let b = vm.make_array(&[a]).unwrap();
    vm.store_slot(a.as_ptr(), 0, b);
    vm.temp_roots.push(a);
    vm.collect_young().unwrap();
    let a2 = vm.temp_roots.pop().unwrap();
    let b2 = vm.heap.slot(a2.as_ptr(), 0);
    assert_eq!(vm.heap.slot(b2.as_ptr(), 0), a2);
}

#[test]
fn barrier_filters_stores() {
    let mut vm = small_vm();
    let nil = vm.nil();
    let old = Value::from_ptr(vm.heap.alloc_ptrs_old(CLASS_ARRAY, 3, nil).unwrap());
    let young = vm.make_array(&[int(5)]).unwrap();
    let young2 = vm.make_array(&[int(6)]).unwrap();

    assert!(vm.heap.ssb.is_empty());
    // int into old: filtered.
    vm.store_slot(old.as_ptr(), 0, int(42));
    assert!(vm.heap.ssb.is_empty());
    // young into young: filtered.
    vm.store_slot(young.as_ptr(), 0, young2);
    assert!(vm.heap.ssb.is_empty());
    // young into old: remembered, once.
    vm.store_slot(old.as_ptr(), 1, young);
    assert_eq!(vm.heap.ssb, vec![old.as_ptr()]);
    vm.store_slot(old.as_ptr(), 2, young2);
    assert_eq!(vm.heap.ssb, vec![old.as_ptr()], "remembered bit set, no duplicate");
}

#[test]
fn scavenge_drains_ssb_and_updates_old_object() {
    let mut vm = small_vm();
    let nil = vm.nil();
    let old = Value::from_ptr(vm.heap.alloc_ptrs_old(CLASS_ARRAY, 1, nil).unwrap());
    let young = vm.make_array(&[int(77)]).unwrap();
    vm.store_slot(old.as_ptr(), 0, young);

    vm.collect_young().unwrap();
    let child = vm.heap.slot(old.as_ptr(), 0);
    assert_ne!(child, young, "slot updated to the copy");
    assert!(vm.heap.is_young(child.as_ptr()));
    assert_eq!(vm.heap.slot(child.as_ptr(), 0), int(77));
    // Still points young: must stay remembered.
    assert_eq!(vm.heap.ssb, vec![old.as_ptr()]);
    assert!(vm.heap.header(old.as_ptr()).gc_bits() & GC_BIT_REMEMBERED != 0);
}

#[test]
fn remembered_bit_clears_when_no_longer_pointing_young() {
    let mut vm = small_vm();
    vm.tenure_threshold = 1; // tenure on first copy
    let nil = vm.nil();
    let old = Value::from_ptr(vm.heap.alloc_ptrs_old(CLASS_ARRAY, 1, nil).unwrap());
    let young = vm.make_array(&[int(5)]).unwrap();
    vm.store_slot(old.as_ptr(), 0, young);

    vm.collect_young().unwrap();
    let child = vm.heap.slot(old.as_ptr(), 0);
    assert!(vm.heap.in_old_space(child.as_ptr()), "child tenured");
    assert!(vm.heap.ssb.is_empty(), "SSB drained");
    assert_eq!(vm.heap.header(old.as_ptr()).gc_bits() & GC_BIT_REMEMBERED, 0);
}

#[test]
fn objects_tenure_after_surviving_n_scavenges() {
    let mut vm = small_vm();
    assert_eq!(vm.tenure_threshold, TENURE_AGE);
    let obj = vm.make_array(&[int(1)]).unwrap();
    vm.temp_roots.push(obj);
    for i in 0..TENURE_AGE {
        let v = *vm.temp_roots.last().unwrap();
        assert!(vm.heap.is_young(v.as_ptr()), "still young before scavenge {i}");
        vm.collect_young().unwrap();
    }
    let v = vm.temp_roots.pop().unwrap();
    assert!(vm.heap.in_old_space(v.as_ptr()), "tenured after {TENURE_AGE} scavenges");
    assert_eq!(vm.heap.slot(v.as_ptr(), 0), int(1));
}

#[test]
fn tenured_object_pointing_young_gets_remembered() {
    let mut vm = small_vm();
    vm.tenure_threshold = 1;
    let child = vm.make_array(&[int(3)]).unwrap();
    let parent = vm.make_array(&[child]).unwrap();
    vm.temp_roots.push(parent);
    // Keep the child young by giving it age 0 and the parent... both get
    // copied; with threshold 1 both tenure. Instead: two-step — tenure the
    // parent first while the child is fresh.
    // Step 1: parent tenures, child tenures too (both survive) — so instead
    // allocate a fresh child after parent tenured.
    vm.collect_young().unwrap();
    let parent = *vm.temp_roots.last().unwrap();
    assert!(vm.heap.in_old_space(parent.as_ptr()));
    let fresh = vm.make_array(&[int(9)]).unwrap();
    vm.store_slot(parent.as_ptr(), 0, fresh);
    assert_eq!(vm.heap.ssb, vec![parent.as_ptr()]);
    vm.tenure_threshold = TENURE_AGE;
    vm.collect_young().unwrap();
    let parent = vm.temp_roots.pop().unwrap();
    let kid = vm.heap.slot(parent.as_ptr(), 0);
    assert!(vm.heap.is_young(kid.as_ptr()));
    assert_eq!(vm.heap.slot(kid.as_ptr(), 0), int(9));
    assert!(vm.heap.header(parent.as_ptr()).gc_bits() & GC_BIT_REMEMBERED != 0);
}

#[test]
fn overflow_sized_object_survives_scavenge() {
    let mut vm = small_vm();
    let nil = vm.nil();
    let big = Value::from_ptr(vm.heap.alloc_ptrs(CLASS_ARRAY, 300, nil).unwrap());
    vm.heap.set_slot_raw(big.as_ptr(), 299, int(123));
    vm.temp_roots.push(big);
    vm.collect_young().unwrap();
    let big2 = vm.temp_roots.pop().unwrap();
    assert_eq!(vm.heap.num_slots(big2.as_ptr()), 300);
    assert_eq!(vm.heap.slot(big2.as_ptr(), 299), int(123));
}

#[test]
fn identity_hash_survives_moving() {
    let mut vm = small_vm();
    let obj = vm.make_array(&[int(1)]).unwrap();
    let h1 = vm.identity_hash_of(obj);
    vm.temp_roots.push(obj);
    vm.collect_young().unwrap();
    let obj2 = vm.temp_roots.pop().unwrap();
    assert_eq!(vm.identity_hash_of(obj2), h1);
}

/// The payoff test: a program whose allocations exceed young space many
/// times over runs to completion — collections happen under the running
/// interpreter, the stack and process move, cached registers re-derive.
#[test]
fn interpreter_survives_scavenges() {
    let mut vm = small_vm();
    // sum := 0. i := arg. [i > 0] whileTrue: [
    //     box := Box new: i (via MKBOX). sum := sum + box value. i := i - 1].
    // ^sum   — allocates one Box per iteration.
    let m = MethodBuilder::new(1, 8)
        .insns(vec![
            LoadInt { d: 2, imm: 0 },  // sum
            LoadInt { d: 3, imm: 0 },
            LoadInt { d: 4, imm: 1 },
            // loop:
            Gt { d: 5, a: 1, b: 3 },
            JumpFalse { a: 5, off: 5 },
            MkBox { d: 6, a: 1 },      // box := Box(i)
            GetBox { d: 7, a: 6 },
            Add { d: 2, a: 2, b: 7 },  // sum += box.value
            Sub { d: 1, a: 1, b: 4 },
            Jump { off: -7 },
            Ret { a: 2 },
        ])
        .build(&mut vm);
    // 20000 boxes * 16 bytes ≈ 320KB > 64KB young space.
    let result = vm.call(m, vm.nil(), &[int(20000)]).unwrap();
    assert_eq!(result, int(20000 * 20001 / 2));
    assert!(vm.scavenge_count > 0, "collections actually happened");
}

/// Same but with sends in flight, so frames above and below the allocation
/// site live across collections.
#[test]
fn interpreter_survives_scavenges_with_deep_frames() {
    let mut vm = small_vm();
    let class = vm.new_test_class(FMT_FIXED, 0);
    let sel = vm.intern("go:");
    // go: n  n <= 0 ifTrue: [^0]. box := MKBOX n. ^(self go: n - 1) + box value
    // The box (slot 2) must survive the send, so the send stages at r=7:
    // the callee's control words land in slots 3..6, below the receiver.
    let go = MethodBuilder::new(1, 10)
        .insns(vec![
            LoadInt { d: 2, imm: 0 },
            Le { d: 3, a: 1, b: 2 },
            JumpFalse { a: 3, off: 2 },
            LoadInt { d: 4, imm: 0 },
            Ret { a: 4 },
            MkBox { d: 2, a: 1 },
            LoadSelf { d: 7 },
            LoadInt { d: 4, imm: 1 },
            Sub { d: 8, a: 1, b: 4 },
            Send { d: 3, r: 7, site: 0 },
            GetBox { d: 6, a: 2 },
            Add { d: 3, a: 3, b: 6 },
            Ret { a: 3 },
        ])
        .site_named(&mut vm, "go:", 1)
        .build(&mut vm);
    vm.install_method(class, sel, go);
    let obj = vm.make_instance(class).unwrap();
    let caller = MethodBuilder::new(0, 7)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            LoadInt { d: 5, imm: 500 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![obj])
        .site_named(&mut vm, "go:", 1)
        .build(&mut vm);
    // Run several times to churn through young space repeatedly.
    let mut total = 0i64;
    for _ in 0..30 {
        let r = vm.call(caller, vm.nil(), &[]).unwrap();
        total += r.as_int();
    }
    assert_eq!(total, 30 * (500 * 501 / 2));
    assert!(vm.scavenge_count > 0);
}

#[test]
fn diag_deep_frames_memory() {
    let mut vm = small_vm();
    let class = vm.new_test_class(FMT_FIXED, 0);
    let sel = vm.intern("go:");
    let go = MethodBuilder::new(1, 10)
        .insns(vec![
            LoadInt { d: 2, imm: 0 },
            Le { d: 3, a: 1, b: 2 },
            JumpFalse { a: 3, off: 2 },
            LoadInt { d: 4, imm: 0 },
            Ret { a: 4 },
            MkBox { d: 2, a: 1 },
            LoadSelf { d: 7 },
            LoadInt { d: 4, imm: 1 },
            Sub { d: 8, a: 1, b: 4 },
            Send { d: 3, r: 7, site: 0 },
            GetBox { d: 6, a: 2 },
            Add { d: 3, a: 3, b: 6 },
            Ret { a: 3 },
        ])
        .site_named(&mut vm, "go:", 1)
        .build(&mut vm);
    vm.install_method(class, sel, go);
    let obj = vm.make_instance(class).unwrap();
    let caller = MethodBuilder::new(0, 7)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            LoadInt { d: 5, imm: 500 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![obj])
        .site_named(&mut vm, "go:", 1)
        .build(&mut vm);
    for i in 0..30 {
        let r = vm.call(caller, vm.nil(), &[]);
        eprintln!(
            "iter {i}: result {:?}, young {}K, old {}K, ssb {}, scav {}",
            r.as_ref().map(|v| v.as_int()),
            vm.heap.young_from.used_bytes() / 1024,
            vm.heap.old.used_bytes() / 1024,
            vm.heap.ssb.len(),
            vm.scavenge_count
        );
        if r.is_err() { break; }
    }
}
