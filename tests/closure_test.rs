//! Phase 1 tests: closures, capture (copy and box), block activation via
//! the value primitives, and non-local return incl. dead-frame detection
//! (SPEC §10, §20).

use smallishtalk::asm::Insn::*;
use smallishtalk::fixture::MethodBuilder;
use smallishtalk::treaty::*;
use smallishtalk::value::Value;
use smallishtalk::vm::Vm;

fn int(n: i64) -> Value {
    Value::from_int(n)
}

/// Install BlockClosure>>value, value:, and cannotReturn: — primitives with
/// marker-returning fallback bodies so tests can observe failure paths.
fn setup(vm: &mut Vm) {
    let bc = vm.class_table_at(CLASS_BLOCKCLOSURE);
    for (name, prim, argc) in [
        ("value", PRIM_BLOCK_VALUE_0, 0u8),
        ("value:", PRIM_BLOCK_VALUE_1, 1),
        ("value:value:", PRIM_BLOCK_VALUE_2, 2),
    ] {
        let d = argc + 1;
        let m = MethodBuilder::new(argc, argc + 3)
            .primitive(prim)
            .insns(vec![LoadInt { d: d + 1, imm: -77 }, Ret { a: d + 1 }])
            .build(vm);
        let sel = vm.intern(name);
        vm.install_method(bc, sel, m);
    }
    // cannotReturn: value — the in-image BlockCannotReturn signal stand-in.
    let m = MethodBuilder::new(1, 3)
        .insns(vec![LoadInt { d: 2, imm: -99 }, Ret { a: 2 }])
        .build(vm);
    let sel = vm.intern("cannotReturn:");
    vm.install_method(bc, sel, m);
}

#[test]
fn simple_block_value() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let nil = vm.nil();
    // [42] value
    let blk = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 42 }, Ret { a: 1 }])
        .build_block(&mut vm, nil, 0, false);
    let m = MethodBuilder::new(0, 6)
        .insns(vec![
            MkClosure { d: 4, b: 0 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![blk])
        .site_named(&mut vm, "value", 0)
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), int(42));
}

#[test]
fn block_with_argument() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let nil = vm.nil();
    // [:x | x + x] value: 21
    let blk = MethodBuilder::new(1, 3)
        .insns(vec![Add { d: 2, a: 1, b: 1 }, Ret { a: 2 }])
        .build_block(&mut vm, nil, 0, false);
    let m = MethodBuilder::new(0, 7)
        .insns(vec![
            MkClosure { d: 4, b: 0 },
            LoadInt { d: 5, imm: 21 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![blk])
        .site_named(&mut vm, "value:", 1)
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), int(42));
}

#[test]
fn wrong_argument_count_fails_primitive() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let nil = vm.nil();
    // Send #value (0 args) to a 1-arg block: primitive fails, fallback
    // body returns the -77 marker.
    let blk = MethodBuilder::new(1, 3)
        .insns(vec![Ret { a: 1 }])
        .build_block(&mut vm, nil, 0, false);
    let m = MethodBuilder::new(0, 6)
        .insns(vec![
            MkClosure { d: 4, b: 0 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![blk])
        .site_named(&mut vm, "value", 0)
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), int(-77));
}

#[test]
fn captured_value_is_copied() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let nil = vm.nil();
    // temp := 33. [temp] value — read-only capture, copied at MKCLOSURE.
    let blk = MethodBuilder::new(0, 2)
        .insns(vec![
            GetIvar { d: 1, i: CLOSURE_CAPTURED_BASE as u8 },
            Ret { a: 1 },
        ])
        .build_block(&mut vm, nil, 1, false);
    let m = MethodBuilder::new(0, 6)
        .insns(vec![
            LoadInt { d: 2, imm: 33 },
            MkClosure { d: 4, b: 0 },
            Capture { c: 0, a: 2 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![blk])
        .site_named(&mut vm, "value", 0)
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), int(33));
}

#[test]
fn boxed_capture_shares_mutation() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let nil = vm.nil();
    // v := Box(5). [v := 77] value. ^v  — mutated capture goes through a Box.
    let blk = MethodBuilder::new(0, 3)
        .insns(vec![
            GetIvar { d: 1, i: CLOSURE_CAPTURED_BASE as u8 }, // the box
            LoadInt { d: 2, imm: 77 },
            SetBox { a: 1, b: 2 },
            Ret { a: 2 },
        ])
        .build_block(&mut vm, nil, 1, false);
    // The box (slot 2) is read after the send, so the closure is staged at
    // r=8: the callee's control words land in slots 4..7.
    let m = MethodBuilder::new(0, 9)
        .insns(vec![
            LoadInt { d: 3, imm: 5 },
            MkBox { d: 2, a: 3 },
            MkClosure { d: 8, b: 0 },
            Capture { c: 0, a: 2 },
            Send { d: 1, r: 8, site: 0 },
            GetBox { d: 3, a: 2 },
            Ret { a: 3 },
        ])
        .literals(vec![blk])
        .site_named(&mut vm, "value", 0)
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), int(77));
}

#[test]
fn box_written_by_method_read_by_block() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let nil = vm.nil();
    // v := Box(1). v := 9 (via box). ^[v] value
    let blk = MethodBuilder::new(0, 3)
        .insns(vec![
            GetIvar { d: 1, i: CLOSURE_CAPTURED_BASE as u8 },
            GetBox { d: 2, a: 1 },
            Ret { a: 2 },
        ])
        .build_block(&mut vm, nil, 1, false);
    let m = MethodBuilder::new(0, 7)
        .insns(vec![
            LoadInt { d: 3, imm: 1 },
            MkBox { d: 2, a: 3 },
            MkClosure { d: 4, b: 0 },
            Capture { c: 0, a: 2 },
            LoadInt { d: 3, imm: 9 },
            SetBox { a: 2, b: 3 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![blk])
        .site_named(&mut vm, "value", 0)
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), int(9));
}

#[test]
fn non_local_return_unwinds_to_home_caller() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let nil = vm.nil();
    let class = vm.new_test_class(FMT_FIXED, 0);

    // block: [^42] — NLR
    let blk = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 42 }, Nlr { a: 1 }])
        .build_block(&mut vm, nil, 0, true);

    // h: aBlock — evaluates the block; if the block returned normally,
    // answers -1 (must NOT happen on the NLR path).
    let h = MethodBuilder::new(1, 7)
        .insns(vec![
            Move { d: 4, a: 1 },
            Send { d: 2, r: 4, site: 0 },
            LoadInt { d: 3, imm: -1 },
            Ret { a: 3 },
        ])
        .site_named(&mut vm, "value", 0)
        .build(&mut vm);
    let h_sel = vm.intern("h:");
    vm.install_method(class, h_sel, h);

    // m — self h: [^42]. ^-5   (the ^-5 must be skipped: NLR returns 42
    // from m itself.)
    let m = MethodBuilder::new(0, 8)
        .insns(vec![
            MkClosure { d: 5, b: 0 },
            LoadSelf { d: 4 },
            Send { d: 1, r: 4, site: 0 },
            LoadInt { d: 2, imm: -5 },
            Ret { a: 2 },
        ])
        .literals(vec![blk])
        .site_named(&mut vm, "h:", 1)
        .build(&mut vm);

    let obj = vm.make_instance(class).unwrap();
    assert_eq!(vm.call(m, obj, &[]).unwrap(), int(42));
}

#[test]
fn nlr_to_dead_frame_in_same_process_hits_cannot_return() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let nil = vm.nil();
    let class = vm.new_test_class(FMT_FIXED, 0);

    // mk — ^[^42]  (returns the NLR block; its home dies with mk's frame)
    let blk = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 42 }, Nlr { a: 1 }])
        .build_block(&mut vm, nil, 0, true);
    let mk = MethodBuilder::new(0, 6)
        .insns(vec![MkClosure { d: 4, b: 0 }, Ret { a: 4 }])
        .literals(vec![blk])
        .build(&mut vm);
    let mk_sel = vm.intern("mk");
    vm.install_method(class, mk_sel, mk);

    // m2 — c := self mk. ^c value
    // The value activation lands at the same stack offset mk's frame used,
    // so this exercises the serial check specifically.
    let m2 = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadSelf { d: 4 },
            Send { d: 2, r: 4, site: 0 },
            Move { d: 4, a: 2 },
            Send { d: 1, r: 4, site: 1 },
            Ret { a: 1 },
        ])
        .site_named(&mut vm, "mk", 0)
        .site_named(&mut vm, "value", 0)
        .build(&mut vm);

    let obj = vm.make_instance(class).unwrap();
    // cannotReturn: answers -99; the value send answers that.
    assert_eq!(vm.call(m2, obj, &[]).unwrap(), int(-99));
}

#[test]
fn nlr_from_another_process_is_an_error() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let nil = vm.nil();
    let class = vm.new_test_class(FMT_FIXED, 0);

    // Process 1: mk returns the closure (home = P1's dead frame anyway,
    // but the process check fires first — home process != active).
    let blk = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 42 }, Nlr { a: 1 }])
        .build_block(&mut vm, nil, 0, true);
    let mk = MethodBuilder::new(0, 6)
        .insns(vec![MkClosure { d: 4, b: 0 }, Ret { a: 4 }])
        .literals(vec![blk])
        .build(&mut vm);
    let obj = vm.make_instance(class).unwrap();
    let closure = vm.call(mk, obj, &[]).unwrap();

    // Process 2: evaluate the closure.
    let invoker = MethodBuilder::new(0, 6)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![closure])
        .site_named(&mut vm, "value", 0)
        .build(&mut vm);
    assert_eq!(vm.call(invoker, vm.nil(), &[]).unwrap(), int(-99));
}

#[test]
fn blocks_without_nlr_have_nil_home() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let nil = vm.nil();
    let blk = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 1 }, Ret { a: 1 }])
        .build_block(&mut vm, nil, 0, false);
    let m = MethodBuilder::new(0, 6)
        .insns(vec![MkClosure { d: 4, b: 0 }, Ret { a: 4 }])
        .literals(vec![blk])
        .build(&mut vm);
    let closure = vm.call(m, vm.nil(), &[]).unwrap();
    let c = closure.as_ptr();
    assert_eq!(vm.heap.header(c).class_index(), CLASS_BLOCKCLOSURE);
    assert_eq!(vm.heap.slot(c, CLOSURE_HOME_PROCESS), vm.nil());
    assert_eq!(vm.heap.slot(c, CLOSURE_COMPILED_BLOCK), blk);
}

#[test]
fn nested_block_nlr_uses_method_home() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let nil = vm.nil();
    let class = vm.new_test_class(FMT_FIXED, 0);

    // inner block: [^7] (created *inside* the outer block activation; its
    // home must still be the method's frame)
    let inner = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 7 }, Nlr { a: 1 }])
        .build_block(&mut vm, nil, 0, true);
    // outer block: [ [^7] value. -2 ]
    let outer = MethodBuilder::new(0, 7)
        .insns(vec![
            MkClosure { d: 4, b: 0 },
            Send { d: 1, r: 4, site: 0 },
            LoadInt { d: 2, imm: -2 },
            Ret { a: 2 },
        ])
        .literals(vec![inner])
        .site_named(&mut vm, "value", 0)
        .build_block(&mut vm, nil, 0, true);
    // h: aBlock — aBlock value. ^-1
    let h = MethodBuilder::new(1, 7)
        .insns(vec![
            Move { d: 4, a: 1 },
            Send { d: 2, r: 4, site: 0 },
            LoadInt { d: 3, imm: -1 },
            Ret { a: 3 },
        ])
        .site_named(&mut vm, "value", 0)
        .build(&mut vm);
    let h_sel = vm.intern("h:");
    vm.install_method(class, h_sel, h);
    // m — self h: [ [^7] value. -2 ]. ^-5
    let m = MethodBuilder::new(0, 8)
        .insns(vec![
            MkClosure { d: 5, b: 0 },
            LoadSelf { d: 4 },
            Send { d: 1, r: 4, site: 0 },
            LoadInt { d: 2, imm: -5 },
            Ret { a: 2 },
        ])
        .literals(vec![outer])
        .site_named(&mut vm, "h:", 1)
        .build(&mut vm);

    let obj = vm.make_instance(class).unwrap();
    // The NLR from the inner block returns 7 from m itself.
    assert_eq!(vm.call(m, obj, &[]).unwrap(), int(7));
}
