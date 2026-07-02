//! GC stress mode (SPEC §20 Phase 4): the feature set exercised under a
//! tiny young space with aggressive tenuring, so collections happen
//! constantly under real programs — closures, exceptions, ensure blocks,
//! processes, and semaphores all interacting with a moving heap.

use smallishtalk::asm::Insn::*;
use smallishtalk::fixture::MethodBuilder;
use smallishtalk::heap::HeapConfig;
use smallishtalk::treaty::*;
use smallishtalk::value::Value;
use smallishtalk::vm::{Vm, VmConfig};

fn int(n: i64) -> Value {
    Value::from_int(n)
}

fn stress_vm() -> Vm {
    let mut vm = Vm::bare(VmConfig {
        heap: HeapConfig {
            young_bytes: 64 * 1024,
            old_bytes: 32 * 1024 * 1024,
            ..HeapConfig::default()
        },
        ..VmConfig::default()
    });
    vm.tenure_threshold = 2; // aggressive tenuring
    vm
}

/// Closures allocated in a hot loop, evaluated through the block-value
/// primitive, with captures — every iteration allocates and many trigger
/// scavenges.
#[test]
fn closure_churn_under_gc_pressure() {
    let mut vm = stress_vm();
    let bc = vm.class_table_at(CLASS_BLOCKCLOSURE);
    let value_m = MethodBuilder::new(0, 3)
        .primitive(PRIM_BLOCK_VALUE_0)
        .insns(vec![LoadInt { d: 2, imm: -77 }, Ret { a: 2 }])
        .build(&mut vm);
    let sel = vm.intern("value");
    vm.install_method(bc, sel, value_m);

    let nil = vm.nil();
    // block: [captured]
    let blk = MethodBuilder::new(0, 2)
        .insns(vec![
            GetIvar { d: 1, i: CLOSURE_CAPTURED_BASE as u8 },
            Ret { a: 1 },
        ])
        .build_block(&mut vm, nil, 1, false);
    // sum := 0. i := arg. [i>0] whileTrue: [sum := sum + [i] value. i := i-1]
    let m = MethodBuilder::new(1, 12)
        .insns(vec![
            LoadInt { d: 2, imm: 0 },
            LoadInt { d: 3, imm: 0 },
            // loop: (slots 4..7 are clobbered by the r=8 send; the loop
            // constant is reloaded after it)
            Gt { d: 5, a: 1, b: 3 },
            JumpFalse { a: 5, off: 7 },
            MkClosure { d: 8, b: 0 },
            Capture { c: 0, a: 1 },
            Send { d: 6, r: 8, site: 0 },
            Add { d: 2, a: 2, b: 6 },
            LoadInt { d: 4, imm: 1 },
            Sub { d: 1, a: 1, b: 4 },
            Jump { off: -9 },
            Ret { a: 2 },
        ])
        .literals(vec![blk])
        .site_named(&mut vm, "value", 0)
        .build(&mut vm);
    let n = 30_000i64;
    assert_eq!(vm.call(m, vm.nil(), &[int(n)]).unwrap(), int(n * (n + 1) / 2));
    assert!(vm.scavenge_count > 10, "constant collection ({} scavenges)", vm.scavenge_count);
}

/// Exceptions raised and handled in a loop under GC pressure: handler
/// frames, unwinder state, and exception objects all survive moves.
#[test]
fn exceptions_in_a_loop_under_gc_pressure() {
    let mut vm = stress_vm();
    // Reuse the exception kernel shape from exception_test, minimal form.
    let object = vm.class_table_at(CLASS_OBJECT);
    let bc = vm.class_table_at(CLASS_BLOCKCLOSURE);
    for (name, prim, argc) in [
        ("value", PRIM_BLOCK_VALUE_0, 0u8),
        ("value:", PRIM_BLOCK_VALUE_1, 1),
    ] {
        let m = MethodBuilder::new(argc, argc + 3)
            .primitive(prim)
            .insns(vec![LoadInt { d: argc + 2, imm: -77 }, Ret { a: argc + 2 }])
            .build(&mut vm);
        let sel = vm.intern(name);
        vm.install_method(bc, sel, m);
    }
    for (name, prim, argc, marker) in [
        ("findHandlerFor:from:", PRIM_FIND_HANDLER, 2u8, -100i16),
        ("unwindTo:serial:return:", PRIM_UNWIND_TO, 3, -200),
        ("handlerInfoAt:", PRIM_HANDLER_INFO, 1, -300),
        ("setHandlerState:to:", PRIM_SET_HANDLER_STATE, 2, -400),
        ("signalContext", PRIM_SIGNAL_CONTEXT, 0, -500),
    ] {
        let m = MethodBuilder::new(argc, argc + 3)
            .primitive(prim)
            .insns(vec![LoadInt { d: argc + 2, imm: marker }, Ret { a: argc + 2 }])
            .build(&mut vm);
        let sel = vm.intern(name);
        vm.install_method(object, sel, m);
    }
    let exc_class = vm.new_test_class(FMT_FIXED, 4);
    // Minimal Exception>>signal: find, mark in-progress, run handler,
    // unwind to handler frame with the handler's result.
    let signal = MethodBuilder::new(0, 14)
        .insns(vec![
            LoadSelf { d: 10 },
            Send { d: 1, r: 10, site: 0 }, // signalContext (ivars unused here)
            LoadSelf { d: 10 },
            ClassOf { d: 11, a: 0 },
            LoadNil { d: 12 },
            Send { d: 1, r: 10, site: 1 }, // hoff
            LoadSelf { d: 10 },
            Move { d: 11, a: 1 },
            Send { d: 2, r: 10, site: 2 }, // info
            LoadSelf { d: 10 },
            Move { d: 11, a: 1 },
            LoadInt { d: 12, imm: HANDLER_STATE_IN_PROGRESS as i16 },
            Send { d: 3, r: 10, site: 3 },
            LoadInt { d: 3, imm: 2 },
            At { d: 4, a: 2, b: 3 },
            Move { d: 10, a: 4 },
            LoadSelf { d: 11 },
            Send { d: 5, r: 10, site: 4 }, // handler result
            LoadInt { d: 3, imm: 3 },
            At { d: 4, a: 2, b: 3 },       // serial
            LoadSelf { d: 10 },
            Move { d: 11, a: 1 },
            Move { d: 12, a: 4 },
            Move { d: 13, a: 5 },
            Send { d: 4, r: 10, site: 5 },
            LoadInt { d: 4, imm: -2000 },
            Ret { a: 4 },
        ])
        .site_named(&mut vm, "signalContext", 0)
        .site_named(&mut vm, "findHandlerFor:from:", 2)
        .site_named(&mut vm, "handlerInfoAt:", 1)
        .site_named(&mut vm, "setHandlerState:to:", 2)
        .site_named(&mut vm, "value:", 1)
        .site_named(&mut vm, "unwindTo:serial:return:", 3)
        .build(&mut vm);
    let sel = vm.intern("signal");
    vm.install_method(exc_class, sel, signal);

    let on_do = MethodBuilder::new(2, 12)
        .handler_slot_base(3)
        .mh_flags(MH_FLAG_IS_HANDLER)
        .insns(vec![
            Move { d: 3, a: 1 },
            Move { d: 4, a: 2 },
            LoadInt { d: 5, imm: HANDLER_STATE_ARMED as i16 },
            Move { d: 10, a: 0 },
            Send { d: 1, r: 10, site: 0 },
            LoadInt { d: 5, imm: 0 },
            Ret { a: 1 },
        ])
        .site_named(&mut vm, "value", 0)
        .build(&mut vm);
    let sel = vm.intern("on:do:");
    vm.install_method(bc, sel, on_do);

    // one: [ ExcNew-ish: allocate a fresh exception and signal it ] — the
    // protected block allocates via a class literal + new... simplest: the
    // exception instance is fresh per iteration via `new` primitive.
    let new_m = MethodBuilder::new(0, 3)
        .primitive(PRIM_NEW)
        .insns(vec![Ret { a: 1 }])
        .build(&mut vm);
    let sel = vm.intern("new");
    vm.install_method(object, sel, new_m);

    let nil = vm.nil();
    // protected: [(ExcClass new) signal]
    let prot = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            Move { d: 4, a: 1 },
            Send { d: 2, r: 4, site: 1 },
            Ret { a: 2 },
        ])
        .literals(vec![exc_class])
        .site_named(&mut vm, "new", 0)
        .site_named(&mut vm, "signal", 0)
        .build_block(&mut vm, nil, 0, false);
    // handler: [:e | 1]
    let handler = MethodBuilder::new(1, 3)
        .insns(vec![LoadInt { d: 2, imm: 1 }, Ret { a: 2 }])
        .build_block(&mut vm, nil, 0, false);

    // sum := 0. i := arg. [i>0] whileTrue: [
    //   sum := sum + ([prot] on: Exc do: handler). i := i-1]. ^sum
    let m = MethodBuilder::new(1, 14)
        .insns(vec![
            LoadInt { d: 2, imm: 0 },
            LoadInt { d: 3, imm: 0 },
            // loop: (constant reloaded after the send — slots 4..7 are the
            // callee's control-word area)
            Gt { d: 5, a: 1, b: 3 },
            JumpFalse { a: 5, off: 8 },
            MkClosure { d: 8, b: 0 },
            LoadK { d: 9, k: 1 },
            MkClosure { d: 10, b: 2 },
            Send { d: 6, r: 8, site: 0 },
            Add { d: 2, a: 2, b: 6 },
            LoadInt { d: 4, imm: 1 },
            Sub { d: 1, a: 1, b: 4 },
            Jump { off: -10 },
            Ret { a: 2 },
        ])
        .literals(vec![prot, exc_class, handler])
        .site_named(&mut vm, "on:do:", 2)
        .build(&mut vm);
    let n = 3000i64;
    assert_eq!(vm.call(m, vm.nil(), &[int(n)]).unwrap(), int(n));
    assert!(vm.scavenge_count > 5, "{} scavenges", vm.scavenge_count);
}

/// Producer/consumer ping-pong under GC pressure: process objects,
/// stacks, and semaphores move while suspended and blocked.
#[test]
fn processes_and_semaphores_under_gc_pressure() {
    let mut vm = stress_vm();
    let object = vm.class_table_at(CLASS_OBJECT);
    let sem_cls = vm.class_table_at(CLASS_SEMAPHORE);
    for (class, sel, prim, argc) in [
        (sem_cls, "wait", PRIM_SEMAPHORE_WAIT, 0u8),
        (sem_cls, "signal", PRIM_SEMAPHORE_SIGNAL, 0),
        (object, "yield", PRIM_YIELD, 0),
    ] {
        let m = MethodBuilder::new(argc, argc + 3)
            .primitive(prim)
            .insns(vec![Ret { a: argc + 1 }])
            .build(&mut vm);
        let s = vm.intern(sel);
        vm.install_method(class, s, m);
    }
    let proc_cls = vm.class_table_at(CLASS_PROCESS);
    let resume_m = MethodBuilder::new(0, 3)
        .primitive(PRIM_PROCESS_RESUME)
        .insns(vec![Ret { a: 1 }])
        .build(&mut vm);
    let s = vm.intern("resume");
    vm.install_method(proc_cls, s, resume_m);

    let sem_in = vm.make_semaphore_old();
    let sem_out = vm.make_semaphore_old();
    let boxv = vm.make_instance(vm.class_table_at(CLASS_BOX)).unwrap();
    vm.heap.set_slot_raw(boxv.as_ptr(), 0, int(0));

    // Worker: loop forever: sem_in wait. box := box + 1 (with a garbage
    // allocation via MKBOX). sem_out signal.
    let w_m = MethodBuilder::new(0, 10)
        .insns(vec![
            // loop:
            LoadK { d: 4, k: 0 }, // sem_in
            Send { d: 1, r: 4, site: 0 },
            LoadK { d: 1, k: 2 }, // box
            GetBox { d: 2, a: 1 },
            LoadInt { d: 3, imm: 1 },
            Add { d: 2, a: 2, b: 3 },
            MkBox { d: 5, a: 2 }, // garbage
            GetBox { d: 2, a: 5 },
            SetBox { a: 1, b: 2 },
            LoadK { d: 4, k: 1 }, // sem_out
            Send { d: 1, r: 4, site: 1 },
            Jump { off: -12 },
        ])
        .literals(vec![sem_in, sem_out, boxv])
        .site_named(&mut vm, "wait", 0)
        .site_named(&mut vm, "signal", 0)
        .build(&mut vm);
    let worker = vm.spawn_process(w_m, vm.nil(), &[]).unwrap();

    // Main: worker resume. i := 500. [i>0] whileTrue: [
    //   sem_in signal. sem_out wait. i := i-1]. ^box value
    let m_m = MethodBuilder::new(0, 12)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 }, // resume worker
            LoadInt { d: 1, imm: 500 },
            LoadInt { d: 2, imm: 0 },
            LoadInt { d: 3, imm: 1 },
            // loop:
            Gt { d: 5, a: 1, b: 2 },
            JumpFalse { a: 5, off: 6 },
            LoadK { d: 6, k: 1 },
            Send { d: 5, r: 6, site: 1 }, // sem_in signal
            LoadK { d: 6, k: 2 },
            Send { d: 5, r: 6, site: 2 }, // sem_out wait
            Sub { d: 1, a: 1, b: 3 },
            Jump { off: -8 },
            LoadK { d: 1, k: 3 },
            GetBox { d: 2, a: 1 },
            Ret { a: 2 },
        ])
        .literals(vec![worker, sem_in, sem_out, boxv])
        .site_named(&mut vm, "resume", 0)
        .site_named(&mut vm, "signal", 0)
        .site_named(&mut vm, "wait", 0)
        .build(&mut vm);
    let main = vm.spawn_process(m_m, vm.nil(), &[]).unwrap();
    assert_eq!(vm.run(main).unwrap(), int(500));
}
