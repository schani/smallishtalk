//! Phase 1 exception tests (SPEC §11, §20): handler frames, the handler
//! walk, the re-entrant unwinder, resume/return semantics, and ensure
//! blocks during normal completion, NLR, and exception unwinds.
//!
//! The "image side" of §11 (Exception>>signal, on:do:, ensure:) is written
//! here as hand-assembled bytecode over the VM's primitives — exactly the
//! division of labor the spec prescribes.

use smallishtalk::asm::Insn::{self, *};
use smallishtalk::fixture::MethodBuilder;
use smallishtalk::treaty::*;
use smallishtalk::value::Value;
use smallishtalk::vm::Vm;

fn int(n: i64) -> Value {
    Value::from_int(n)
}

struct Kernel {
    exc_class: Value,
    sub_exc_class: Value,
    non_exc_class: Value,
}

/// Exc instance layout: ivars 0..3 = signalOff, signalSerial, handlerOff,
/// handlerSerial.
fn setup(vm: &mut Vm) -> Kernel {
    let object = vm.class_table_at(CLASS_OBJECT);
    let bc = vm.class_table_at(CLASS_BLOCKCLOSURE);

    // --- Block value primitives ---
    for (name, prim, argc) in [
        ("value", PRIM_BLOCK_VALUE_0, 0u8),
        ("value:", PRIM_BLOCK_VALUE_1, 1),
    ] {
        let m = MethodBuilder::new(argc, argc + 3)
            .primitive(prim)
            .insns(vec![
                LoadInt { d: argc + 2, imm: -77 },
                Ret { a: argc + 2 },
            ])
            .build(vm);
        let sel = vm.intern(name);
        vm.install_method(bc, sel, m);
    }

    // --- Exception-system helper primitives, on Object ---
    for (name, prim, argc, marker) in [
        ("findHandlerFor:from:", PRIM_FIND_HANDLER, 2u8, -100i16),
        ("unwindTo:serial:return:", PRIM_UNWIND_TO, 3, -200),
        ("handlerInfoAt:", PRIM_HANDLER_INFO, 1, -300),
        ("setHandlerState:to:", PRIM_SET_HANDLER_STATE, 2, -400),
        ("signalContext", PRIM_SIGNAL_CONTEXT, 0, -500),
    ] {
        let m = MethodBuilder::new(argc, argc + 3)
            .primitive(prim)
            .insns(vec![
                LoadInt { d: argc + 2, imm: marker },
                Ret { a: argc + 2 },
            ])
            .build(vm);
        let sel = vm.intern(name);
        vm.install_method(object, sel, m);
    }

    // --- Exception classes ---
    let exc_class = vm.new_test_class(FMT_FIXED, 4);
    let sub_exc_class = vm.new_test_subclass(exc_class, FMT_FIXED, 4);
    let non_exc_class = vm.new_test_class(FMT_FIXED, 0);

    // --- Exception>>signal ---
    // Sends stage at r=10 (control words in slots 6..9; slots 0..5 live).
    let signal = MethodBuilder::new(0, 14)
        .insns(vec![
            // ctx := self signalContext; store signalOff/signalSerial.
            LoadSelf { d: 10 },
            Send { d: 1, r: 10, site: 0 },
            LoadInt { d: 2, imm: 1 },
            At { d: 3, a: 1, b: 2 },
            SetIvar { i: 0, a: 3 },
            LoadInt { d: 2, imm: 2 },
            At { d: 3, a: 1, b: 2 },
            SetIvar { i: 1, a: 3 },
            // hoff := self findHandlerFor: self class from: nil.
            LoadSelf { d: 10 },
            ClassOf { d: 11, a: 0 },
            LoadNil { d: 12 },
            Send { d: 1, r: 10, site: 1 },
            // hoff == nil → unhandled (marker -1000).
            LoadNil { d: 2 },
            IdEq { d: 3, a: 1, b: 2 },
            JumpFalse { a: 3, off: 2 },
            LoadInt { d: 4, imm: -1000 },
            Ret { a: 4 },
            // info := self handlerInfoAt: hoff.
            LoadSelf { d: 10 },
            Move { d: 11, a: 1 },
            Send { d: 2, r: 10, site: 2 },
            // handlerOff/handlerSerial ivars.
            SetIvar { i: 2, a: 1 },
            LoadInt { d: 3, imm: 3 },
            At { d: 4, a: 2, b: 3 },
            SetIvar { i: 3, a: 4 },
            // self setHandlerState: hoff to: IN_PROGRESS.
            LoadSelf { d: 10 },
            Move { d: 11, a: 1 },
            LoadInt { d: 12, imm: HANDLER_STATE_IN_PROGRESS as i16 },
            Send { d: 3, r: 10, site: 3 },
            // handlerResult := (info at: 2) value: self.
            LoadInt { d: 3, imm: 2 },
            At { d: 4, a: 2, b: 3 },
            Move { d: 10, a: 4 },
            LoadSelf { d: 11 },
            Send { d: 5, r: 10, site: 4 },
            // Falling off the handler = return: its value.
            GetIvar { d: 2, i: 2 },
            GetIvar { d: 3, i: 3 },
            LoadSelf { d: 10 },
            Move { d: 11, a: 2 },
            Move { d: 12, a: 3 },
            Move { d: 13, a: 5 },
            Send { d: 4, r: 10, site: 5 },
            LoadInt { d: 4, imm: -2000 },
            Ret { a: 4 },
        ])
        .site_named(vm, "signalContext", 0)
        .site_named(vm, "findHandlerFor:from:", 2)
        .site_named(vm, "handlerInfoAt:", 1)
        .site_named(vm, "setHandlerState:to:", 2)
        .site_named(vm, "value:", 1)
        .site_named(vm, "unwindTo:serial:return:", 3)
        .build(vm);
    let sel = vm.intern("signal");
    vm.install_method(exc_class, sel, signal);

    // --- Exception>>resume: (unwind to the signal frame) ---
    let resume = MethodBuilder::new(1, 14)
        .insns(vec![
            GetIvar { d: 2, i: 0 },
            GetIvar { d: 3, i: 1 },
            LoadSelf { d: 10 },
            Move { d: 11, a: 2 },
            Move { d: 12, a: 3 },
            Move { d: 13, a: 1 },
            Send { d: 4, r: 10, site: 0 },
            LoadInt { d: 4, imm: -3000 },
            Ret { a: 4 },
        ])
        .site_named(vm, "unwindTo:serial:return:", 3)
        .build(vm);
    let sel = vm.intern("resume:");
    vm.install_method(exc_class, sel, resume);

    // --- Exception>>return: (unwind to the handler frame) ---
    let ret = MethodBuilder::new(1, 14)
        .insns(vec![
            GetIvar { d: 2, i: 2 },
            GetIvar { d: 3, i: 3 },
            LoadSelf { d: 10 },
            Move { d: 11, a: 2 },
            Move { d: 12, a: 3 },
            Move { d: 13, a: 1 },
            Send { d: 4, r: 10, site: 0 },
            LoadInt { d: 4, imm: -4000 },
            Ret { a: 4 },
        ])
        .site_named(vm, "unwindTo:serial:return:", 3)
        .build(vm);
    let sel = vm.intern("return:");
    vm.install_method(exc_class, sel, ret);

    // --- BlockClosure>>on:do: — the handler frame (§11) ---
    // Slots: 0 recv (protected block), 1 excClass, 2 handlerBlock,
    // 3 stored class, 4 stored block, 5 state; handlerSlotBase = 3.
    let on_do = MethodBuilder::new(2, 12)
        .handler_slot_base(3)
        .mh_flags(MH_FLAG_IS_HANDLER)
        .insns(vec![
            Move { d: 3, a: 1 },
            Move { d: 4, a: 2 },
            LoadInt { d: 5, imm: HANDLER_STATE_ARMED as i16 },
            Move { d: 10, a: 0 },
            Send { d: 1, r: 10, site: 0 },
            LoadInt { d: 5, imm: 0 }, // disarm on normal completion
            Ret { a: 1 },
        ])
        .site_named(vm, "value", 0)
        .build(vm);
    let sel = vm.intern("on:do:");
    vm.install_method(bc, sel, on_do);

    // --- BlockClosure>>ensure: — the ensure frame (§11) ---
    // Slots: 0 recv (protected), 1 aBlock, 2 ensure-block slot,
    // 3 pending target, 4 pending serial, 5 pending value;
    // handlerSlotBase = 2. On normal completion the body consumes the
    // block slot (nils it) and runs the block itself.
    let ensure = MethodBuilder::new(1, 12)
        .handler_slot_base(2)
        .mh_flags(MH_FLAG_IS_ENSURE)
        .insns(vec![
            Move { d: 2, a: 1 },
            Move { d: 10, a: 0 },
            Send { d: 1, r: 10, site: 0 },
            // normal completion: blk := slot2. slot2 := nil. blk value.
            Move { d: 3, a: 2 },
            LoadNil { d: 2 },
            Move { d: 10, a: 3 },
            Send { d: 4, r: 10, site: 1 },
            Ret { a: 1 },
        ])
        .site_named(vm, "value", 0)
        .site_named(vm, "value", 0)
        .build(vm);
    let sel = vm.intern("ensure:");
    vm.install_method(bc, sel, ensure);

    Kernel {
        exc_class,
        sub_exc_class,
        non_exc_class,
    }
}

/// Block: [exc signal] — signals the literal exception, answers the
/// signal result (so resume: values surface).
fn protected_signal_block(vm: &mut Vm, exc: Value) -> Value {
    let nil = vm.nil();
    MethodBuilder::new(0, 6)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![exc])
        .site_named(vm, "signal", 0)
        .build_block(vm, nil, 0, false)
}

/// Main method: `protected on: excClass do: handler`, returning the result.
fn on_do_main(vm: &mut Vm, protected: Value, exc_class: Value, handler: Value) -> Value {
    MethodBuilder::new(0, 12)
        .insns(vec![
            MkClosure { d: 8, b: 0 },
            LoadK { d: 9, k: 1 },
            MkClosure { d: 10, b: 2 },
            Send { d: 1, r: 8, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![protected, exc_class, handler])
        .site_named(vm, "on:do:", 2)
        .build(vm)
}

fn const_handler(vm: &mut Vm, value: i16) -> Value {
    let nil = vm.nil();
    MethodBuilder::new(1, 3)
        .insns(vec![LoadInt { d: 2, imm: value }, Ret { a: 2 }])
        .build_block(vm, nil, 0, false)
}

#[test]
fn catch_and_fall_off_handler() {
    let mut vm = Vm::bare_test();
    let k = setup(&mut vm);
    let exc = vm.make_instance(k.exc_class).unwrap();
    let prot = protected_signal_block(&mut vm, exc);
    let handler = const_handler(&mut vm, 42);
    let main = on_do_main(&mut vm, prot, k.exc_class, handler);
    assert_eq!(vm.call(main, vm.nil(), &[]).unwrap(), int(42));
}

#[test]
fn handler_receives_the_exception() {
    let mut vm = Vm::bare_test();
    let k = setup(&mut vm);
    let exc = vm.make_instance(k.exc_class).unwrap();
    let prot = protected_signal_block(&mut vm, exc);
    let nil = vm.nil();
    // [:e | e]
    let handler = MethodBuilder::new(1, 3)
        .insns(vec![Ret { a: 1 }])
        .build_block(&mut vm, nil, 0, false);
    let main = on_do_main(&mut vm, prot, k.exc_class, handler);
    let result = vm.call(main, vm.nil(), &[]).unwrap();
    assert_eq!(result, exc, "handler argument is the signaled exception");
}

#[test]
fn subclass_exception_is_handled() {
    let mut vm = Vm::bare_test();
    let k = setup(&mut vm);
    let exc = vm.make_instance(k.sub_exc_class).unwrap();
    let prot = protected_signal_block(&mut vm, exc);
    let handler = const_handler(&mut vm, 42);
    // Handler registered for the *superclass*.
    let main = on_do_main(&mut vm, prot, k.exc_class, handler);
    assert_eq!(vm.call(main, vm.nil(), &[]).unwrap(), int(42));
}

#[test]
fn unrelated_exception_is_not_handled() {
    let mut vm = Vm::bare_test();
    let k = setup(&mut vm);
    // Signal an Exc, but the handler is registered for NonExc: the search
    // finds nothing and signal answers the unhandled marker.
    let exc = vm.make_instance(k.exc_class).unwrap();
    let prot = protected_signal_block(&mut vm, exc);
    let handler = const_handler(&mut vm, 42);
    let main = on_do_main(&mut vm, prot, k.non_exc_class, handler);
    assert_eq!(vm.call(main, vm.nil(), &[]).unwrap(), int(-1000));
}

#[test]
fn resume_continues_after_the_signal() {
    let mut vm = Vm::bare_test();
    let k = setup(&mut vm);
    let exc = vm.make_instance(k.exc_class).unwrap();
    let nil = vm.nil();
    // protected: [(exc signal) + 1]
    let prot = MethodBuilder::new(0, 6)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            LoadInt { d: 2, imm: 1 },
            Add { d: 3, a: 1, b: 2 },
            Ret { a: 3 },
        ])
        .literals(vec![exc])
        .site_named(&mut vm, "signal", 0)
        .build_block(&mut vm, nil, 0, false);
    // handler: [:e | e resume: 7]
    let handler = MethodBuilder::new(1, 7)
        .insns(vec![
            Move { d: 4, a: 1 },
            LoadInt { d: 5, imm: 7 },
            Send { d: 2, r: 4, site: 0 },
            LoadInt { d: 3, imm: -9 },
            Ret { a: 3 },
        ])
        .site_named(&mut vm, "resume:", 1)
        .build_block(&mut vm, nil, 0, false);
    let main = on_do_main(&mut vm, prot, k.exc_class, handler);
    // signal answers 7, protected answers 8, on:do: completes normally.
    assert_eq!(vm.call(main, vm.nil(), &[]).unwrap(), int(8));
}

#[test]
fn return_exits_the_handler_frame() {
    let mut vm = Vm::bare_test();
    let k = setup(&mut vm);
    let exc = vm.make_instance(k.exc_class).unwrap();
    let nil = vm.nil();
    let prot = protected_signal_block(&mut vm, exc);
    // handler: [:e | e return: 99]
    let handler = MethodBuilder::new(1, 7)
        .insns(vec![
            Move { d: 4, a: 1 },
            LoadInt { d: 5, imm: 99 },
            Send { d: 2, r: 4, site: 0 },
            LoadInt { d: 3, imm: -8 },
            Ret { a: 3 },
        ])
        .site_named(&mut vm, "return:", 1)
        .build_block(&mut vm, nil, 0, false);
    let main = on_do_main(&mut vm, prot, k.exc_class, handler);
    assert_eq!(vm.call(main, vm.nil(), &[]).unwrap(), int(99));
}

#[test]
fn signal_inside_handler_skips_in_progress_handler() {
    let mut vm = Vm::bare_test();
    let k = setup(&mut vm);
    let exc1 = vm.make_instance(k.exc_class).unwrap();
    let exc2 = vm.make_instance(k.exc_class).unwrap();
    let nil = vm.nil();

    // inner protected: [exc1 signal]
    let prot_inner = protected_signal_block(&mut vm, exc1);
    // inner handler: [:e | exc2 signal. -1] — the second signal must skip
    // this (in-progress) handler and reach the outer one.
    let inner_handler = MethodBuilder::new(1, 7)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 2, r: 4, site: 0 },
            LoadInt { d: 3, imm: -1 },
            Ret { a: 3 },
        ])
        .literals(vec![exc2])
        .site_named(&mut vm, "signal", 0)
        .build_block(&mut vm, nil, 0, false);
    // inner block: [ prot_inner on: Exc do: inner_handler ]
    let inner = MethodBuilder::new(0, 12)
        .insns(vec![
            MkClosure { d: 8, b: 0 },
            LoadK { d: 9, k: 1 },
            MkClosure { d: 10, b: 2 },
            Send { d: 1, r: 8, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![prot_inner, k.exc_class, inner_handler])
        .site_named(&mut vm, "on:do:", 2)
        .build_block(&mut vm, nil, 0, false);
    let outer_handler = const_handler(&mut vm, 55);
    let main = on_do_main(&mut vm, inner, k.exc_class, outer_handler);
    assert_eq!(vm.call(main, vm.nil(), &[]).unwrap(), int(55));
}

fn make_box(vm: &mut Vm, v: i64) -> Value {
    let b = vm.make_instance(vm.class_table_at(CLASS_BOX)).unwrap();
    vm.heap.set_slot_raw(b.as_ptr(), 0, int(v));
    b
}

/// Ensure block: [box := box * 10 + k]
fn tracing_ensure_block(vm: &mut Vm, boxv: Value, k: i16) -> Value {
    let nil = vm.nil();
    MethodBuilder::new(0, 6)
        .insns(vec![
            LoadK { d: 1, k: 0 },
            GetBox { d: 2, a: 1 },
            LoadInt { d: 3, imm: 10 },
            Mul { d: 4, a: 2, b: 3 },
            LoadInt { d: 3, imm: k },
            Add { d: 2, a: 4, b: 3 },
            SetBox { a: 1, b: 2 },
            Ret { a: 2 },
        ])
        .literals(vec![boxv])
        .build_block(vm, nil, 0, false)
}

#[test]
fn ensure_runs_on_normal_completion() {
    let mut vm = Vm::bare_test();
    let _k = setup(&mut vm);
    let boxv = make_box(&mut vm, 0);
    let nil = vm.nil();
    let prot = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 5 }, Ret { a: 1 }])
        .build_block(&mut vm, nil, 0, false);
    let ens = tracing_ensure_block(&mut vm, boxv, 7);
    let main = MethodBuilder::new(0, 12)
        .insns(vec![
            MkClosure { d: 8, b: 0 },
            MkClosure { d: 9, b: 1 },
            Send { d: 1, r: 8, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![prot, ens])
        .site_named(&mut vm, "ensure:", 1)
        .build(&mut vm);
    assert_eq!(vm.call(main, vm.nil(), &[]).unwrap(), int(5));
    assert_eq!(vm.heap.slot(boxv.as_ptr(), 0), int(7), "ensure block ran once");
}

#[test]
fn ensure_runs_during_nlr_unwind() {
    let mut vm = Vm::bare_test();
    let _k = setup(&mut vm);
    let class = vm.new_test_class(FMT_FIXED, 0);
    let boxv = make_box(&mut vm, 0);
    let nil = vm.nil();

    // [^42]
    let nlr_blk = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 42 }, Nlr { a: 1 }])
        .build_block(&mut vm, nil, 0, true);
    // [blk value] — captures blk (h:'s argument)
    let call_blk = MethodBuilder::new(0, 7)
        .insns(vec![
            GetIvar { d: 1, i: CLOSURE_CAPTURED_BASE as u8 },
            Move { d: 4, a: 1 },
            Send { d: 2, r: 4, site: 0 },
            Ret { a: 2 },
        ])
        .site_named(&mut vm, "value", 0)
        .build_block(&mut vm, nil, 1, false);
    let ens = tracing_ensure_block(&mut vm, boxv, 8);
    // h: blk — [blk value] ensure: [box trace]
    let h = MethodBuilder::new(1, 12)
        .insns(vec![
            MkClosure { d: 8, b: 0 },
            Capture { c: 0, a: 1 },
            MkClosure { d: 9, b: 1 },
            Send { d: 2, r: 8, site: 0 },
            Ret { a: 2 },
        ])
        .literals(vec![call_blk, ens])
        .site_named(&mut vm, "ensure:", 1)
        .build(&mut vm);
    let h_sel = vm.intern("h:");
    vm.install_method(class, h_sel, h);
    // m — self h: [^42]. ^-5
    let m = MethodBuilder::new(0, 8)
        .insns(vec![
            MkClosure { d: 5, b: 0 },
            LoadSelf { d: 4 },
            Send { d: 1, r: 4, site: 0 },
            LoadInt { d: 2, imm: -5 },
            Ret { a: 2 },
        ])
        .literals(vec![nlr_blk])
        .site_named(&mut vm, "h:", 1)
        .build(&mut vm);

    let obj = vm.make_instance(class).unwrap();
    assert_eq!(vm.call(m, obj, &[]).unwrap(), int(42));
    assert_eq!(vm.heap.slot(boxv.as_ptr(), 0), int(8), "ensure ran during NLR");
}

#[test]
fn ensure_runs_during_exception_unwind() {
    let mut vm = Vm::bare_test();
    let k = setup(&mut vm);
    let exc = vm.make_instance(k.exc_class).unwrap();
    let boxv = make_box(&mut vm, 0);
    let nil = vm.nil();

    let prot_inner = protected_signal_block(&mut vm, exc);
    let ens = tracing_ensure_block(&mut vm, boxv, 1);
    // P: [ [exc signal] ensure: [trace] ]
    let p = MethodBuilder::new(0, 12)
        .insns(vec![
            MkClosure { d: 8, b: 0 },
            MkClosure { d: 9, b: 1 },
            Send { d: 1, r: 8, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![prot_inner, ens])
        .site_named(&mut vm, "ensure:", 1)
        .build_block(&mut vm, nil, 0, false);
    // handler: [:e | e return: 9]
    let handler = MethodBuilder::new(1, 7)
        .insns(vec![
            Move { d: 4, a: 1 },
            LoadInt { d: 5, imm: 9 },
            Send { d: 2, r: 4, site: 0 },
            LoadInt { d: 3, imm: -8 },
            Ret { a: 3 },
        ])
        .site_named(&mut vm, "return:", 1)
        .build_block(&mut vm, nil, 0, false);
    let main = on_do_main(&mut vm, p, k.exc_class, handler);
    assert_eq!(vm.call(main, vm.nil(), &[]).unwrap(), int(9));
    assert_eq!(
        vm.heap.slot(boxv.as_ptr(), 0),
        int(1),
        "ensure ran during the return: unwind"
    );
}

#[test]
fn nested_ensures_run_inner_first_on_normal_completion() {
    let mut vm = Vm::bare_test();
    let _k = setup(&mut vm);
    let boxv = make_box(&mut vm, 0);
    let nil = vm.nil();
    let five = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 5 }, Ret { a: 1 }])
        .build_block(&mut vm, nil, 0, false);
    let e1 = tracing_ensure_block(&mut vm, boxv, 1);
    let e2 = tracing_ensure_block(&mut vm, boxv, 2);
    // P1: [ [5] ensure: e1 ]
    let p1 = MethodBuilder::new(0, 12)
        .insns(vec![
            MkClosure { d: 8, b: 0 },
            MkClosure { d: 9, b: 1 },
            Send { d: 1, r: 8, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![five, e1])
        .site_named(&mut vm, "ensure:", 1)
        .build_block(&mut vm, nil, 0, false);
    // main: [P1] ensure: e2
    let main = MethodBuilder::new(0, 12)
        .insns(vec![
            MkClosure { d: 8, b: 0 },
            MkClosure { d: 9, b: 1 },
            Send { d: 1, r: 8, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![p1, e2])
        .site_named(&mut vm, "ensure:", 1)
        .build(&mut vm);
    assert_eq!(vm.call(main, vm.nil(), &[]).unwrap(), int(5));
    assert_eq!(vm.heap.slot(boxv.as_ptr(), 0), int(12), "inner then outer");
}

#[test]
fn nlr_through_nested_ensures_runs_both_in_order() {
    let mut vm = Vm::bare_test();
    let _k = setup(&mut vm);
    let class = vm.new_test_class(FMT_FIXED, 0);
    let boxv = make_box(&mut vm, 0);
    let nil = vm.nil();

    let nlr_blk = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 42 }, Nlr { a: 1 }])
        .build_block(&mut vm, nil, 0, true);
    let call_blk = MethodBuilder::new(0, 7)
        .insns(vec![
            GetIvar { d: 1, i: CLOSURE_CAPTURED_BASE as u8 },
            Move { d: 4, a: 1 },
            Send { d: 2, r: 4, site: 0 },
            Ret { a: 2 },
        ])
        .site_named(&mut vm, "value", 0)
        .build_block(&mut vm, nil, 1, false);
    let e1 = tracing_ensure_block(&mut vm, boxv, 1);
    let e2 = tracing_ensure_block(&mut vm, boxv, 2);
    // inner: [ [blk value] ensure: e1 ]  (captures blk, passes it on to
    // the call_blk closure it creates)
    let inner = MethodBuilder::new(0, 12)
        .insns(vec![
            GetIvar { d: 1, i: CLOSURE_CAPTURED_BASE as u8 },
            MkClosure { d: 8, b: 0 },
            Capture { c: 0, a: 1 },
            MkClosure { d: 9, b: 1 },
            Send { d: 2, r: 8, site: 0 },
            Ret { a: 2 },
        ])
        .literals(vec![call_blk, e1])
        .site_named(&mut vm, "ensure:", 1)
        .build_block(&mut vm, nil, 1, false);
    // h: blk — [inner(blk)] ensure: e2
    let h = MethodBuilder::new(1, 12)
        .insns(vec![
            MkClosure { d: 8, b: 0 },
            Capture { c: 0, a: 1 },
            MkClosure { d: 9, b: 1 },
            Send { d: 2, r: 8, site: 0 },
            Ret { a: 2 },
        ])
        .literals(vec![inner, e2])
        .site_named(&mut vm, "ensure:", 1)
        .build(&mut vm);
    let h_sel = vm.intern("h:");
    vm.install_method(class, h_sel, h);
    let m = MethodBuilder::new(0, 8)
        .insns(vec![
            MkClosure { d: 5, b: 0 },
            LoadSelf { d: 4 },
            Send { d: 1, r: 4, site: 0 },
            LoadInt { d: 2, imm: -5 },
            Ret { a: 2 },
        ])
        .literals(vec![nlr_blk])
        .site_named(&mut vm, "h:", 1)
        .build(&mut vm);

    let obj = vm.make_instance(class).unwrap();
    assert_eq!(vm.call(m, obj, &[]).unwrap(), int(42));
    assert_eq!(
        vm.heap.slot(boxv.as_ptr(), 0),
        int(12),
        "innermost ensure first, then outer"
    );
}
