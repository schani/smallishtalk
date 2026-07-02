//! Phase 1 tests: old-space mark-compact (SPEC §14) — sliding compaction
//! with a forwarding side table, allocation order preserved, suspended
//! process stacks still resolving after moves.

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
            young_bytes: 128 * 1024,
            old_bytes: 8 * 1024 * 1024,
            ..HeapConfig::default()
        },
        ..VmConfig::default()
    })
}

#[test]
fn compaction_reclaims_garbage_and_preserves_order() {
    let mut vm = small_vm();
    let nil = vm.nil();
    // garbage, live A, garbage, live B (A points to B), garbage.
    let _g1 = vm.heap.alloc_ptrs_old(CLASS_ARRAY, 1000, nil).unwrap();
    let a = vm.heap.alloc_ptrs_old(CLASS_ARRAY, 2, nil).unwrap();
    let _g2 = vm.heap.alloc_ptrs_old(CLASS_ARRAY, 1000, nil).unwrap();
    let b = vm.heap.alloc_ptrs_old(CLASS_ARRAY, 1, nil).unwrap();
    vm.heap.set_slot_raw(b, 0, int(77));
    vm.heap.set_slot_raw(a, 0, Value::from_ptr(b));
    vm.heap.set_slot_raw(a, 1, int(5));
    let _g3 = vm.heap.alloc_ptrs_old(CLASS_ARRAY, 1000, nil).unwrap();

    let used_before = vm.heap.old.used_bytes();
    vm.temp_roots.push(Value::from_ptr(a));
    vm.collect_old().unwrap();
    let a2 = vm.temp_roots.pop().unwrap();

    assert!(
        vm.heap.old.used_bytes() < used_before - 3 * 8000,
        "garbage reclaimed"
    );
    assert!(a2.as_ptr() < a, "A slid left");
    let b2 = vm.heap.slot(a2.as_ptr(), 0);
    assert!(b2.is_ptr() && vm.heap.in_old_space(b2.as_ptr()));
    assert!(a2.as_ptr() < b2.as_ptr(), "allocation order preserved");
    assert_eq!(vm.heap.slot(b2.as_ptr(), 0), int(77));
    assert_eq!(vm.heap.slot(a2.as_ptr(), 1), int(5));
}

#[test]
fn nil_true_false_keep_their_addresses() {
    let mut vm = small_vm();
    let nil = vm.nil();
    let t = vm.true_v();
    let f = vm.false_v();
    let _garbage = vm.heap.alloc_ptrs_old(CLASS_ARRAY, 500, nil).unwrap();
    vm.collect_old().unwrap();
    assert_eq!(vm.nil(), nil, "nil is at a fixed address at the start of old space");
    assert_eq!(vm.true_v(), t);
    assert_eq!(vm.false_v(), f);
}

#[test]
fn overflow_sized_objects_walk_and_move_correctly() {
    let mut vm = small_vm();
    let nil = vm.nil();
    let _g = vm.heap.alloc_ptrs_old(CLASS_ARRAY, 2000, nil).unwrap();
    // An object with the overflow size word, sandwiched between live ones.
    let a = vm.heap.alloc_ptrs_old(CLASS_ARRAY, 2, nil).unwrap();
    let big = vm.heap.alloc_ptrs_old(CLASS_ARRAY, 300, nil).unwrap();
    let c = vm.heap.alloc_ptrs_old(CLASS_ARRAY, 1, nil).unwrap();
    vm.heap.set_slot_raw(big, 299, int(9));
    vm.heap.set_slot_raw(a, 0, Value::from_ptr(big));
    vm.heap.set_slot_raw(a, 1, Value::from_ptr(c));
    vm.heap.set_slot_raw(c, 0, int(3));

    vm.temp_roots.push(Value::from_ptr(a));
    vm.collect_old().unwrap();
    let a2 = vm.temp_roots.pop().unwrap();
    let big2 = vm.heap.slot(a2.as_ptr(), 0);
    let c2 = vm.heap.slot(a2.as_ptr(), 1);
    assert_eq!(vm.heap.num_slots(big2.as_ptr()), 300);
    assert_eq!(vm.heap.slot(big2.as_ptr(), 299), int(9));
    assert_eq!(vm.heap.slot(c2.as_ptr(), 0), int(3));
}

#[test]
fn immutable_and_hash_bits_survive_compaction() {
    let mut vm = small_vm();
    let nil = vm.nil();
    let _g = vm.heap.alloc_ptrs_old(CLASS_ARRAY, 1000, nil).unwrap();
    let s = vm.heap.alloc_bytes_old(CLASS_BYTESTRING, 3).unwrap();
    vm.heap.write_bytes(s, b"abc");
    vm.heap.set_immutable(s);
    let sv = Value::from_ptr(s);
    let h = vm.identity_hash_of(sv);

    vm.temp_roots.push(sv);
    vm.collect_old().unwrap();
    let s2 = vm.temp_roots.pop().unwrap();
    assert_ne!(s2, sv, "moved");
    assert!(vm.heap.is_immutable(s2.as_ptr()));
    assert_eq!(vm.identity_hash_of(s2), h);
    assert_eq!(vm.heap.bytes(s2.as_ptr()), b"abc");
}

#[test]
fn remembered_set_is_rebuilt_after_compaction() {
    let mut vm = small_vm();
    let nil = vm.nil();
    let _g = vm.heap.alloc_ptrs_old(CLASS_ARRAY, 1000, nil).unwrap();
    let old = vm.heap.alloc_ptrs_old(CLASS_ARRAY, 1, nil).unwrap();
    let young = vm.make_array(&[int(5)]).unwrap();
    vm.store_slot(old, 0, young);
    assert_eq!(vm.heap.ssb, vec![old]);

    vm.temp_roots.push(Value::from_ptr(old));
    vm.collect_old().unwrap();
    let old2 = vm.temp_roots.pop().unwrap();
    assert_ne!(old2.as_ptr(), old, "slid left");
    let child = vm.heap.slot(old2.as_ptr(), 0);
    assert!(vm.heap.is_young(child.as_ptr()));
    assert_eq!(vm.heap.slot(child.as_ptr(), 0), int(5));
    assert!(
        vm.heap.ssb.contains(&old2.as_ptr()),
        "remembered set tracks the moved object"
    );
    assert!(vm.heap.header(old2.as_ptr()).gc_bits() & GC_BIT_REMEMBERED != 0);
}

#[test]
fn suspended_process_stack_moves_and_still_runs() {
    let mut vm = small_vm();
    let nil = vm.nil();
    // Garbage first, so compaction actually moves the stack.
    let _g = vm.heap.alloc_ptrs_old(CLASS_ARRAY, 4000, nil).unwrap();

    // A poised process whose method computes 6 * 7.
    let m = MethodBuilder::new(2, 4)
        .insns(vec![Mul { d: 3, a: 1, b: 2 }, Ret { a: 3 }])
        .build(&mut vm);
    let process = vm.spawn_process(m, vm.nil(), &[int(6), int(7)]).unwrap();

    // Tenure the process and its stack into old space.
    vm.tenure_threshold = 1;
    vm.temp_roots.push(process);
    vm.collect_young().unwrap();
    let process = *vm.temp_roots.last().unwrap();
    assert!(vm.heap.in_old_space(process.as_ptr()));
    let stack = vm.heap.slot(process.as_ptr(), PROCESS_STACK);
    assert!(vm.heap.in_old_space(stack.as_ptr()));

    // Compact: both slide left; frames are position-independent.
    vm.collect_old().unwrap();
    let process = vm.temp_roots.pop().unwrap();
    let stack2 = vm.heap.slot(process.as_ptr(), PROCESS_STACK);
    assert_ne!(stack2, stack, "stack object moved by compaction");
    assert_eq!(vm.run(process).unwrap(), int(42));
}

#[test]
fn old_space_exhaustion_triggers_compaction_automatically() {
    let mut vm = Vm::bare(VmConfig {
        heap: HeapConfig {
            young_bytes: 64 * 1024,
            old_bytes: 1024 * 1024,
            large_object_bytes: 32 * 1024,
        },
        ..VmConfig::default()
    });
    // Allocate a 100KB ByteArray (large → old space) per iteration, 100
    // times: ~10MB of old allocation through a 1MB old space only works if
    // exhaustion triggers mark-compact.
    let obj_cls = vm.class_table_at(CLASS_OBJECT);
    let installer = MethodBuilder::new(1, 4)
        .primitive(PRIM_NEW_SIZED)
        .insns(vec![Ret { a: 2 }])
        .build(&mut vm);
    let sel = vm.intern("new:");
    vm.install_method(obj_cls, sel, installer);

    let ba_cls = vm.class_table_at(CLASS_BYTEARRAY);
    // i := 100. [i > 0] whileTrue: [ByteArray new: 100000. i := i - 1]. ^0
    let m = MethodBuilder::new(0, 10)
        .insns(vec![
            LoadInt { d: 1, imm: 100 },
            LoadInt { d: 2, imm: 0 },
            LoadInt { d: 3, imm: 1 },
            // loop:
            Gt { d: 4, a: 1, b: 2 },
            JumpFalse { a: 4, off: 5 },
            LoadK { d: 5, k: 0 },
            LoadK { d: 6, k: 1 },
            Send { d: 4, r: 5, site: 0 },
            Sub { d: 1, a: 1, b: 3 },
            Jump { off: -7 },
            Ret { a: 2 },
        ])
        .literals(vec![ba_cls, int(100_000)])
        .site_named(&mut vm, "new:", 1)
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), int(0));
    assert!(vm.compact_count > 0, "compactions actually happened");
}
