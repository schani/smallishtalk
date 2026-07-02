//! Phase 1 tests: processes, semaphores, the scheduler, termination, and
//! timers (SPEC §13, §11 termination, §20).

use smallishtalk::asm::Insn::*;
use smallishtalk::fixture::MethodBuilder;
use smallishtalk::treaty::*;
use smallishtalk::value::Value;
use smallishtalk::vm::Vm;

fn int(n: i64) -> Value {
    Value::from_int(n)
}

/// Install the process/semaphore primitives with failure-code fallbacks.
fn setup(vm: &mut Vm) {
    let object = vm.class_table_at(CLASS_OBJECT);
    let sem_cls = vm.class_table_at(CLASS_SEMAPHORE);
    let proc_cls = vm.class_table_at(CLASS_PROCESS);
    let installs: &[(Value, &str, u16, u8)] = &[
        (proc_cls, "transferTo:", PRIM_TRANSFER_TO, 1),
        (sem_cls, "wait", PRIM_SEMAPHORE_WAIT, 0),
        (sem_cls, "signal", PRIM_SEMAPHORE_SIGNAL, 0),
        (object, "yield", PRIM_YIELD, 0),
        (proc_cls, "resume", PRIM_PROCESS_RESUME, 0),
        (proc_cls, "suspend", PRIM_PROCESS_SUSPEND, 0),
        (proc_cls, "terminate", PRIM_PROCESS_TERMINATE, 0),
        (object, "signal:atMs:", PRIM_SIGNAL_AT_MS, 2),
    ];
    for (class, sel, prim, argc) in installs {
        let m = MethodBuilder::new(*argc, argc + 3)
            .primitive(*prim)
            .insns(vec![Ret { a: argc + 1 }])
            .build(vm);
        let s = vm.intern(sel);
        vm.install_method(*class, s, m);
    }
}

fn make_box(vm: &mut Vm, v: i64) -> Value {
    let b = vm.make_instance(vm.class_table_at(CLASS_BOX)).unwrap();
    vm.heap.set_slot_raw(b.as_ptr(), 0, int(v));
    b
}

#[test]
fn transfer_to_switches_and_back() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let boxv = make_box(&mut vm, 0);

    // B: box := 7; transfer back to A; (never reached:) box := -1.
    let a_box = make_box(&mut vm, 0); // will hold process A
    let b_m = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 1, k: 0 },
            LoadInt { d: 2, imm: 7 },
            SetBox { a: 1, b: 2 },
            LoadK { d: 1, k: 1 },
            GetBox { d: 4, a: 1 }, // process A
            Send { d: 2, r: 4, site: 0 },
            LoadK { d: 1, k: 0 },
            LoadInt { d: 2, imm: -1 },
            SetBox { a: 1, b: 2 },
            RetSelf,
        ])
        .literals(vec![boxv, a_box])
        .site_named(&mut vm, "transferTo:", 1)
        .build(&mut vm);
    // The transferTo: send needs a Process receiver... B transfers to A:
    // receiver A, so site selector transferTo: goes to A. But wait —
    // transferTo:'s receiver should be the *target* process. We send
    // `A transferTo: nil`-style: receiver is the target.
    // (Argument unused; VM switches to the receiver.)
    let b_proc = vm.spawn_process(b_m, vm.nil(), &[]).unwrap();

    // A: transfer to B; after B transfers back: ^box value.
    let a_m = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            LoadK { d: 1, k: 1 },
            GetBox { d: 2, a: 1 },
            Ret { a: 2 },
        ])
        .literals(vec![b_proc, boxv])
        .site_named(&mut vm, "transferTo:", 1)
        .build(&mut vm);
    // transferTo: takes 1 arg by selector; pass nil.
    // Adjust: stage nil arg.
    let a_m = {
        let _ = a_m;
        MethodBuilder::new(0, 8)
            .insns(vec![
                LoadK { d: 4, k: 0 },
                LoadNil { d: 5 },
                Send { d: 1, r: 4, site: 0 },
                LoadK { d: 1, k: 1 },
                GetBox { d: 2, a: 1 },
                Ret { a: 2 },
            ])
            .literals(vec![b_proc, boxv])
            .site_named(&mut vm, "transferTo:", 1)
            .build(&mut vm)
    };
    let a_proc = vm.spawn_process(a_m, vm.nil(), &[]).unwrap();
    vm.heap.set_slot_raw(a_box.as_ptr(), 0, a_proc);
    vm.write_barrier(a_box.as_ptr(), a_proc);

    // B's transferTo: also needs the nil arg staged — patch B: it stages
    // slot 5? B sends `A transferTo: <slot5 garbage>` — args exist as
    // slot 5 whatever; acceptable: VM ignores the argument.
    assert_eq!(vm.run(a_proc).unwrap(), int(7));
    assert_eq!(
        vm.heap.slot(boxv.as_ptr(), 0),
        int(7),
        "B never ran past its transfer back"
    );
}

#[test]
fn semaphore_excess_signals_do_not_block() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let sem = vm.make_semaphore_old();
    // sem signal. sem wait. ^5
    let m = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 1 },
            LoadInt { d: 1, imm: 5 },
            Ret { a: 1 },
        ])
        .literals(vec![sem])
        .site_named(&mut vm, "signal", 0)
        .site_named(&mut vm, "wait", 0)
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), int(5));
    assert_eq!(
        vm.heap.slot(sem.as_ptr(), SEMAPHORE_EXCESS_SIGNALS),
        int(0),
        "wait consumed the excess signal"
    );
}

#[test]
fn wait_with_no_signaler_is_deadlock() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let sem = vm.make_semaphore_old();
    let m = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            RetSelf,
        ])
        .literals(vec![sem])
        .site_named(&mut vm, "wait", 0)
        .build(&mut vm);
    assert!(vm.call(m, vm.nil(), &[]).is_err());
}

#[test]
fn producer_consumer_via_semaphore() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let sem = vm.make_semaphore_old();
    let boxv = make_box(&mut vm, 0);

    // Worker: box := 99. sem signal. done.
    let w_m = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 1, k: 0 },
            LoadInt { d: 2, imm: 99 },
            SetBox { a: 1, b: 2 },
            LoadK { d: 4, k: 1 },
            Send { d: 1, r: 4, site: 0 },
            RetSelf,
        ])
        .literals(vec![boxv, sem])
        .site_named(&mut vm, "signal", 0)
        .build(&mut vm);
    let worker = vm.spawn_process(w_m, vm.nil(), &[]).unwrap();

    // Main: worker resume. sem wait. ^box value.
    let m_m = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            LoadK { d: 4, k: 1 },
            Send { d: 1, r: 4, site: 1 },
            LoadK { d: 1, k: 2 },
            GetBox { d: 2, a: 1 },
            Ret { a: 2 },
        ])
        .literals(vec![worker, sem, boxv])
        .site_named(&mut vm, "resume", 0)
        .site_named(&mut vm, "wait", 0)
        .build(&mut vm);
    let main = vm.spawn_process(m_m, vm.nil(), &[]).unwrap();
    assert_eq!(vm.run(main).unwrap(), int(99));
    // Worker ran to completion: terminated, stack nil.
    assert_eq!(vm.heap.slot(worker.as_ptr(), PROCESS_STACK), vm.nil());
}

#[test]
fn yield_lets_equal_priority_process_run() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let boxv = make_box(&mut vm, 0);

    let w_m = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 1, k: 0 },
            LoadInt { d: 2, imm: 42 },
            SetBox { a: 1, b: 2 },
            RetSelf,
        ])
        .literals(vec![boxv])
        .build(&mut vm);
    let worker = vm.spawn_process(w_m, vm.nil(), &[]).unwrap();

    // Main: worker resume (no preempt, equal priority). Box is still 0.
    // Then yield → worker runs to completion → box is 42.
    let m_m = MethodBuilder::new(0, 10)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            LoadK { d: 1, k: 1 },
            GetBox { d: 2, a: 1 }, // before yield
            LoadSelf { d: 4 },
            Send { d: 3, r: 4, site: 1 },
            LoadK { d: 1, k: 1 },
            GetBox { d: 3, a: 1 }, // after yield
            // ^before * 100 + after
            LoadInt { d: 5, imm: 100 },
            Mul { d: 6, a: 2, b: 5 },
            Add { d: 6, a: 6, b: 3 },
            Ret { a: 6 },
        ])
        .literals(vec![worker, boxv])
        .site_named(&mut vm, "resume", 0)
        .site_named(&mut vm, "yield", 0)
        .build(&mut vm);
    let main = vm.spawn_process(m_m, vm.nil(), &[]).unwrap();
    assert_eq!(vm.run(main).unwrap(), int(42), "0*100 + 42");
}

#[test]
fn higher_priority_process_preempts_on_resume() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let boxv = make_box(&mut vm, 0);

    let w_m = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 1, k: 0 },
            LoadInt { d: 2, imm: 42 },
            SetBox { a: 1, b: 2 },
            RetSelf,
        ])
        .literals(vec![boxv])
        .build(&mut vm);
    let worker = vm.spawn_process(w_m, vm.nil(), &[]).unwrap();
    vm.heap
        .set_slot_raw(worker.as_ptr(), PROCESS_PRIORITY, int(6)); // > default 4

    // Main: worker resume → immediate preemption → after resume returns,
    // the box is already 42.
    let m_m = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            LoadK { d: 1, k: 1 },
            GetBox { d: 2, a: 1 },
            Ret { a: 2 },
        ])
        .literals(vec![worker, boxv])
        .site_named(&mut vm, "resume", 0)
        .build(&mut vm);
    let main = vm.spawn_process(m_m, vm.nil(), &[]).unwrap();
    assert_eq!(vm.run(main).unwrap(), int(42));
}

#[test]
fn terminate_self_runs_ensure_blocks() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let boxv = make_box(&mut vm, 0);
    let self_box = make_box(&mut vm, 0);

    // Install BlockClosure>>value and ensure: (as in the exception tests).
    let bc = vm.class_table_at(CLASS_BLOCKCLOSURE);
    let value_m = MethodBuilder::new(0, 3)
        .primitive(PRIM_BLOCK_VALUE_0)
        .insns(vec![LoadInt { d: 2, imm: -77 }, Ret { a: 2 }])
        .build(&mut vm);
    let sel = vm.intern("value");
    vm.install_method(bc, sel, value_m);
    let ensure_m = MethodBuilder::new(1, 12)
        .handler_slot_base(2)
        .mh_flags(MH_FLAG_IS_ENSURE)
        .insns(vec![
            Move { d: 2, a: 1 },
            Move { d: 10, a: 0 },
            Send { d: 1, r: 10, site: 0 },
            Move { d: 3, a: 2 },
            LoadNil { d: 2 },
            Move { d: 10, a: 3 },
            Send { d: 4, r: 10, site: 1 },
            Ret { a: 1 },
        ])
        .site_named(&mut vm, "value", 0)
        .site_named(&mut vm, "value", 0)
        .build(&mut vm);
    let sel = vm.intern("ensure:");
    vm.install_method(bc, sel, ensure_m);

    let nil = vm.nil();
    // protected: [ (self_box value) terminate. -1 ]
    let prot = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 1, k: 0 },
            GetBox { d: 4, a: 1 },
            Send { d: 2, r: 4, site: 0 },
            LoadInt { d: 2, imm: -1 },
            Ret { a: 2 },
        ])
        .literals(vec![self_box])
        .site_named(&mut vm, "terminate", 0)
        .build_block(&mut vm, nil, 0, false);
    // ensure: [box := 7]
    let ens = MethodBuilder::new(0, 6)
        .insns(vec![
            LoadK { d: 1, k: 0 },
            LoadInt { d: 2, imm: 7 },
            SetBox { a: 1, b: 2 },
            Ret { a: 2 },
        ])
        .literals(vec![boxv])
        .build_block(&mut vm, nil, 0, false);
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

    let p = vm.spawn_process(main, vm.nil(), &[]).unwrap();
    vm.heap.set_slot_raw(self_box.as_ptr(), 0, p);
    vm.write_barrier(self_box.as_ptr(), p);
    vm.run(p).unwrap();
    assert_eq!(vm.heap.slot(boxv.as_ptr(), 0), int(7), "ensure ran during terminate");
    assert_eq!(vm.heap.slot(p.as_ptr(), PROCESS_STACK), vm.nil(), "terminated");
}

#[test]
fn terminate_other_process_via_trampoline() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    // The Treaty terminate trampoline: a method whose primitive is
    // PRIM_PROCESS_TERMINATE with the process itself as receiver.
    let tramp = MethodBuilder::new(0, 3)
        .primitive(PRIM_PROCESS_TERMINATE)
        .insns(vec![RetSelf])
        .build(&mut vm);
    vm.set_special(SPECIAL_TERMINATE_TRAMPOLINE, tramp);

    let boxv = make_box(&mut vm, 0);
    // Worker (never actually runs its body): box := -1.
    let w_m = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 1, k: 0 },
            LoadInt { d: 2, imm: -1 },
            SetBox { a: 1, b: 2 },
            RetSelf,
        ])
        .literals(vec![boxv])
        .build(&mut vm);
    let worker = vm.spawn_process(w_m, vm.nil(), &[]).unwrap();

    // Main: worker resume (queued). worker terminate (redirects it).
    // self yield (worker runs its trampoline, dies). ^box value (still 0).
    let m_m = MethodBuilder::new(0, 10)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 1 },
            LoadSelf { d: 4 },
            Send { d: 1, r: 4, site: 2 },
            LoadK { d: 1, k: 1 },
            GetBox { d: 2, a: 1 },
            Ret { a: 2 },
        ])
        .literals(vec![worker, boxv])
        .site_named(&mut vm, "resume", 0)
        .site_named(&mut vm, "terminate", 0)
        .site_named(&mut vm, "yield", 0)
        .build(&mut vm);
    let main = vm.spawn_process(m_m, vm.nil(), &[]).unwrap();
    assert_eq!(vm.run(main).unwrap(), int(0), "worker body never ran");
    assert_eq!(vm.heap.slot(worker.as_ptr(), PROCESS_STACK), vm.nil());
}

#[test]
fn timer_signals_semaphore_at_deadline() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let sem = vm.make_semaphore_old();
    // self signal: sem atMs: 1 (immediately due). sem wait. ^7
    let m = MethodBuilder::new(0, 10)
        .insns(vec![
            LoadSelf { d: 4 },
            LoadK { d: 5, k: 0 },
            LoadInt { d: 6, imm: 1 },
            Send { d: 1, r: 4, site: 0 },
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 1 },
            LoadInt { d: 1, imm: 7 },
            Ret { a: 1 },
        ])
        .literals(vec![sem])
        .site_named(&mut vm, "signal:atMs:", 2)
        .site_named(&mut vm, "wait", 0)
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), int(7));
    assert!(vm.timer_requests.is_empty(), "timer consumed");
}

/// A suspended target whose stack has no room for the trampoline: the VM
/// may grow a non-running process's stack (the Stack Invariant sanctions
/// moving it), so terminate must still work.
#[test]
fn terminate_other_grows_full_target_stack() {
    let mut vm = Vm::bare_test();
    setup(&mut vm);
    let tramp = MethodBuilder::new(0, 3)
        .primitive(PRIM_PROCESS_TERMINATE)
        .insns(vec![RetSelf])
        .build(&mut vm);
    vm.set_special(SPECIAL_TERMINATE_TRAMPOLINE, tramp);

    let w_m = MethodBuilder::new(0, 8)
        .insns(vec![RetSelf])
        .build(&mut vm);
    let worker = vm.spawn_process(w_m, vm.nil(), &[]).unwrap();
    // Fake a frame near the top of the 512-slot stack so the trampoline
    // doesn't fit: a 250-slot method frame at offset 460 would need
    // 460+4+250 > 512 for its own check — instead place a small frame such
    // that new_off + trampoline exceeds the stack.
    let filler = MethodBuilder::new(0, 250).insns(vec![RetSelf]).build(&mut vm);
    let stack = vm.heap.slot(worker.as_ptr(), PROCESS_STACK);
    let sa = stack.as_ptr();
    let off = 256usize;
    vm.heap.set_slot_raw(sa, off + FRAME_CALLER, int(1));
    vm.heap.set_slot_raw(sa, off + FRAME_RETINFO, int(0));
    vm.store_slot(sa, off + FRAME_METHOD, filler);
    vm.heap.set_slot_raw(sa, off + FRAME_FLAGS, int(2 << SERIAL_SHIFT));
    let nilv = vm.nil();
    vm.store_slot(sa, off + FRAME_RECEIVER, nilv);
    vm.heap
        .set_slot_raw(worker.as_ptr(), PROCESS_FRAME_OFFSET, int(off as i64));
    vm.heap.set_slot_raw(worker.as_ptr(), PROCESS_PC, int(0));
    // new_off = 256 + 4 + 250 = 510; 510 + 4 + trampoline > 512 → growth.

    let m_m = MethodBuilder::new(0, 10)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 }, // resume
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 1 }, // terminate
            LoadSelf { d: 4 },
            Send { d: 1, r: 4, site: 2 }, // yield → worker unwinds itself
            LoadInt { d: 1, imm: 5 },
            Ret { a: 1 },
        ])
        .literals(vec![worker])
        .site_named(&mut vm, "resume", 0)
        .site_named(&mut vm, "terminate", 0)
        .site_named(&mut vm, "yield", 0)
        .build(&mut vm);
    let main = vm.spawn_process(m_m, vm.nil(), &[]).unwrap();
    assert_eq!(vm.run(main).unwrap(), int(5));
    assert_eq!(vm.heap.slot(worker.as_ptr(), PROCESS_STACK), vm.nil());
}
