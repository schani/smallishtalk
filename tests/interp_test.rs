//! Phase 1 tests: the interpreter against hand-assembled bytecode
//! (SPEC §20 Phase 1). Baseline: data movement, sends, returns, jumps,
//! DNU, mustBeBoolean, stack growth, specialized-send fast and slow paths.

use smallishtalk::asm::Insn::*;
use smallishtalk::fixture::MethodBuilder;
use smallishtalk::treaty::*;
use smallishtalk::value::Value;
use smallishtalk::vm::Vm;

fn int(n: i64) -> Value {
    Value::from_int(n)
}

// --- Data movement and returns ---

#[test]
fn return_immediate() {
    let mut vm = Vm::bare_test();
    let m = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 42 }, Ret { a: 1 }])
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), int(42));
}

#[test]
fn return_negative_immediate() {
    let mut vm = Vm::bare_test();
    let m = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: -7 }, Ret { a: 1 }])
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), int(-7));
}

#[test]
fn move_and_literals() {
    let mut vm = Vm::bare_test();
    let lit = int(123456789);
    let m = MethodBuilder::new(0, 3)
        .insns(vec![
            LoadK { d: 1, k: 0 },
            Move { d: 2, a: 1 },
            Ret { a: 2 },
        ])
        .literals(vec![lit])
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), lit);
}

#[test]
fn known_constants() {
    let mut vm = Vm::bare_test();
    for (insn, expect) in [
        (LoadNil { d: 1 }, vm.nil()),
        (LoadTrue { d: 1 }, vm.true_v()),
        (LoadFalse { d: 1 }, vm.false_v()),
    ] {
        let m = MethodBuilder::new(0, 2)
            .insns(vec![insn, Ret { a: 1 }])
            .build(&mut vm);
        assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), expect);
    }
}

#[test]
fn retself_and_loadself() {
    let mut vm = Vm::bare_test();
    let m = MethodBuilder::new(0, 1).insns(vec![RetSelf]).build(&mut vm);
    assert_eq!(vm.call(m, int(77), &[]).unwrap(), int(77));

    let m2 = MethodBuilder::new(0, 2)
        .insns(vec![LoadSelf { d: 1 }, Ret { a: 1 }])
        .build(&mut vm);
    assert_eq!(vm.call(m2, int(88), &[]).unwrap(), int(88));
}

#[test]
fn arguments_arrive_in_slots() {
    let mut vm = Vm::bare_test();
    // arg1 is bytecode slot 1, arg2 slot 2; return arg2.
    let m = MethodBuilder::new(2, 3)
        .insns(vec![Ret { a: 2 }])
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[int(1), int(2)]).unwrap(), int(2));
}

#[test]
fn get_and_set_ivars() {
    let mut vm = Vm::bare_test();
    let class = vm.new_test_class(FMT_FIXED, 2);
    let obj = vm.make_instance(class).unwrap();
    vm.heap.set_slot_raw(obj.as_ptr(), 0, int(10));
    vm.heap.set_slot_raw(obj.as_ptr(), 1, int(20));

    // ivar1 := ivar0 + arg; return ivar1
    let m = MethodBuilder::new(1, 4)
        .insns(vec![
            GetIvar { d: 2, i: 0 },
            Add { d: 3, a: 2, b: 1 },
            SetIvar { i: 1, a: 3 },
            GetIvar { d: 2, i: 1 },
            Ret { a: 2 },
        ])
        .build(&mut vm);
    assert_eq!(vm.call(m, obj, &[int(5)]).unwrap(), int(15));
    assert_eq!(vm.heap.slot(obj.as_ptr(), 1), int(15));
}

// --- Jumps ---

#[test]
fn conditional_jumps_and_loop() {
    let mut vm = Vm::bare_test();
    // sum := 0; i := arg. [i > 0] whileTrue: [sum := sum + i. i := i - 1]. ^sum
    let m = MethodBuilder::new(1, 6)
        .insns(vec![
            LoadInt { d: 2, imm: 0 },  // sum
            LoadInt { d: 3, imm: 0 },  // zero
            LoadInt { d: 4, imm: 1 },  // one
            // loop:
            Gt { d: 5, a: 1, b: 3 },   // i > 0
            JumpFalse { a: 5, off: 3 },
            Add { d: 2, a: 2, b: 1 },  // sum += i
            Sub { d: 1, a: 1, b: 4 },  // i -= 1
            Jump { off: -5 },
            Ret { a: 2 },
        ])
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[int(10)]).unwrap(), int(55));
}

#[test]
fn jumptrue_takes_true_branch() {
    let mut vm = Vm::bare_test();
    let m = MethodBuilder::new(0, 3)
        .insns(vec![
            LoadTrue { d: 1 },
            JumpTrue { a: 1, off: 2 },
            LoadInt { d: 2, imm: 1 },
            Ret { a: 2 },
            LoadInt { d: 2, imm: 2 },
            Ret { a: 2 },
        ])
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), int(2));
}

#[test]
fn jump_on_non_boolean_sends_must_be_boolean() {
    let mut vm = Vm::bare_test();
    // Install SmallInteger>>mustBeBoolean returning true, so the retried
    // jump proceeds down the true branch.
    let sel = vm.specials()[SPECIAL_SEL_MUST_BE_BOOLEAN];
    let mbb = MethodBuilder::new(0, 2)
        .insns(vec![LoadTrue { d: 1 }, Ret { a: 1 }])
        .build(&mut vm);
    vm.install_method(vm.class_table_at(CLASS_SMALLINTEGER), sel, mbb);

    let m = MethodBuilder::new(0, 3)
        .insns(vec![
            LoadInt { d: 1, imm: 5 }, // not a boolean!
            JumpTrue { a: 1, off: 2 },
            LoadInt { d: 2, imm: 111 },
            Ret { a: 2 },
            LoadInt { d: 2, imm: 222 },
            Ret { a: 2 },
        ])
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[]).unwrap(), int(222));
}

// --- Sends ---

#[test]
fn send_through_dictionary_lookup() {
    let mut vm = Vm::bare_test();
    let class = vm.new_test_class(FMT_FIXED, 0);
    let sel = vm.intern("double:");
    // double: x  ^x + x
    let double = MethodBuilder::new(1, 3)
        .insns(vec![Add { d: 2, a: 1, b: 1 }, Ret { a: 2 }])
        .build(&mut vm);
    vm.install_method(class, sel, double);

    let obj = vm.make_instance(class).unwrap();
    // caller: ^obj double: 21 — receiver staged at slot 4 (slots 0..3 are
    // the callee's control-word area under the overlapping convention).
    let caller = MethodBuilder::new(0, 7)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            LoadInt { d: 5, imm: 21 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![obj])
        .site_named(&mut vm, "double:", 1)
        .build(&mut vm);
    assert_eq!(vm.call(caller, vm.nil(), &[]).unwrap(), int(42));
}

#[test]
fn send_finds_method_in_superclass() {
    let mut vm = Vm::bare_test();
    let sup = vm.new_test_class(FMT_FIXED, 0);
    let sub = vm.new_test_subclass(sup, FMT_FIXED, 0);
    let sel = vm.intern("answer");
    let answer = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 42 }, Ret { a: 1 }])
        .build(&mut vm);
    vm.install_method(sup, sel, answer);

    let obj = vm.make_instance(sub).unwrap();
    let caller = MethodBuilder::new(0, 6)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![obj])
        .site_named(&mut vm, "answer", 0)
        .build(&mut vm);
    assert_eq!(vm.call(caller, vm.nil(), &[]).unwrap(), int(42));
}

#[test]
fn send_super_starts_above_static_class() {
    let mut vm = Vm::bare_test();
    let sup = vm.new_test_class(FMT_FIXED, 0);
    let sub = vm.new_test_subclass(sup, FMT_FIXED, 0);
    let sel = vm.intern("answer");
    let sup_m = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 1 }, Ret { a: 1 }])
        .build(&mut vm);
    vm.install_method(sup, sel, sup_m);
    let sub_m = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 2 }, Ret { a: 1 }])
        .build(&mut vm);
    vm.install_method(sub, sel, sub_m);

    // A method installed in `sub` doing `^super answer` — static class sub.
    let caller = MethodBuilder::new(0, 6)
        .insns(vec![
            LoadSelf { d: 4 },
            SendSuper { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .super_site_named(&mut vm, "answer", 0, sub)
        .build(&mut vm);
    let caller_sel = vm.intern("callSuper");
    vm.install_method(sub, caller_sel, caller);

    let obj = vm.make_instance(sub).unwrap();
    // plain send of #answer hits the override...
    let direct = MethodBuilder::new(0, 6)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![obj])
        .site_named(&mut vm, "answer", 0)
        .build(&mut vm);
    assert_eq!(vm.call(direct, vm.nil(), &[]).unwrap(), int(2));
    // ...but callSuper reaches the superclass method.
    let via_super = MethodBuilder::new(0, 6)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![obj])
        .site_named(&mut vm, "callSuper", 0)
        .build(&mut vm);
    assert_eq!(vm.call(via_super, vm.nil(), &[]).unwrap(), int(1));
}

#[test]
fn dnu_dispatches_does_not_understand_with_message() {
    let mut vm = Vm::bare_test();
    let class = vm.new_test_class(FMT_FIXED, 0);
    let dnu_sel = vm.specials()[SPECIAL_SEL_DOES_NOT_UNDERSTAND];
    // doesNotUnderstand: msg  ^msg
    let dnu = MethodBuilder::new(1, 2)
        .insns(vec![Ret { a: 1 }])
        .build(&mut vm);
    vm.install_method(class, dnu_sel, dnu);

    let obj = vm.make_instance(class).unwrap();
    let caller = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            LoadInt { d: 5, imm: 9 },
            LoadInt { d: 6, imm: 8 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![obj])
        .site_named(&mut vm, "frobnicate:with:", 2)
        .build(&mut vm);
    let msg = vm.call(caller, vm.nil(), &[]).unwrap();

    // The result is a Message: slot 0 selector, slot 1 arguments Array.
    assert!(msg.is_ptr());
    let maddr = msg.as_ptr();
    assert_eq!(vm.heap.header(maddr).class_index(), CLASS_MESSAGE);
    assert_eq!(vm.heap.slot(maddr, 0), vm.intern("frobnicate:with:"));
    let args = vm.heap.slot(maddr, 1).as_ptr();
    assert_eq!(vm.heap.num_slots(args), 2);
    assert_eq!(vm.heap.slot(args, 0), int(9));
    assert_eq!(vm.heap.slot(args, 1), int(8));
}

#[test]
fn dnu_without_handler_is_fatal() {
    let mut vm = Vm::bare_test();
    let class = vm.new_test_class(FMT_FIXED, 0);
    let obj = vm.make_instance(class).unwrap();
    let caller = MethodBuilder::new(0, 6)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![obj])
        .site_named(&mut vm, "nope", 0)
        .build(&mut vm);
    assert!(vm.call(caller, vm.nil(), &[]).is_err());
}

// --- Recursion and stack growth ---

#[test]
fn recursion_grows_stack_and_continues() {
    let mut vm = Vm::bare_test();
    let class = vm.new_test_class(FMT_FIXED, 0);
    let sel = vm.intern("down:");
    // down: n  n <= 0 ifTrue: [^0]. ^(self down: n - 1) + 1
    let down = MethodBuilder::new(1, 9)
        .insns(vec![
            LoadInt { d: 2, imm: 0 },
            Le { d: 3, a: 1, b: 2 },
            JumpFalse { a: 3, off: 2 },
            LoadInt { d: 4, imm: 0 },
            Ret { a: 4 },
            LoadSelf { d: 4 },        // stage receiver at slot 4
            LoadInt { d: 6, imm: 1 },
            Sub { d: 5, a: 1, b: 6 }, // arg n-1 at slot 5
            Send { d: 2, r: 4, site: 0 },
            LoadInt { d: 6, imm: 1 },
            Add { d: 3, a: 2, b: 6 },
            Ret { a: 3 },
        ])
        .site_named(&mut vm, "down:", 1)
        .build(&mut vm);
    vm.install_method(class, sel, down);

    let obj = vm.make_instance(class).unwrap();
    let caller = MethodBuilder::new(0, 7)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            LoadInt { d: 5, imm: 2000 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![obj])
        .site_named(&mut vm, "down:", 1)
        .build(&mut vm);

    let process = vm.spawn_process(caller, vm.nil(), &[]).unwrap();
    let result = vm.run(process).unwrap();
    assert_eq!(result, int(2000));
    assert!(vm.stack_grow_count > 0, "the stack object was replaced by growth");
    assert_eq!(
        vm.heap.slot(process.as_ptr(), PROCESS_STACK),
        vm.nil(),
        "finished process is terminated (stack nil)"
    );
}

// --- Specialized sends: fast paths ---

#[test]
fn specialized_arithmetic_fast_paths() {
    let mut vm = Vm::bare_test();
    let cases: Vec<(smallishtalk::asm::Insn, i64, i64, i64)> = vec![
        (Add { d: 3, a: 1, b: 2 }, 3, 4, 7),
        (Sub { d: 3, a: 1, b: 2 }, 3, 4, -1),
        (Mul { d: 3, a: 1, b: 2 }, -6, 7, -42),
        (Div { d: 3, a: 1, b: 2 }, 7, 2, 3),
        (Div { d: 3, a: 1, b: 2 }, -7, 2, -4), // floored!
        (Mod { d: 3, a: 1, b: 2 }, 7, 2, 1),
        (Mod { d: 3, a: 1, b: 2 }, -7, 2, 1), // floored: -7 \\ 2 = 1
        (Mod { d: 3, a: 1, b: 2 }, 7, -2, -1),
    ];
    for (insn, x, y, expect) in cases {
        let m = MethodBuilder::new(2, 4)
            .insns(vec![insn, Ret { a: 3 }])
            .build(&mut vm);
        assert_eq!(
            vm.call(m, vm.nil(), &[int(x), int(y)]).unwrap(),
            int(expect),
            "{insn:?} on {x},{y}"
        );
    }
}

#[test]
fn specialized_comparisons() {
    let mut vm = Vm::bare_test();
    let t = vm.true_v();
    let f = vm.false_v();
    let cases: Vec<(smallishtalk::asm::Insn, i64, i64, Value)> = vec![
        (Lt { d: 3, a: 1, b: 2 }, 1, 2, t),
        (Lt { d: 3, a: 1, b: 2 }, 2, 1, f),
        (Gt { d: 3, a: 1, b: 2 }, 2, 1, t),
        (Le { d: 3, a: 1, b: 2 }, 2, 2, t),
        (Ge { d: 3, a: 1, b: 2 }, 1, 2, f),
        (EqNum { d: 3, a: 1, b: 2 }, 5, 5, t),
        (EqNum { d: 3, a: 1, b: 2 }, 5, 6, f),
        (Lt { d: 3, a: 1, b: 2 }, -3, 2, t),
    ];
    for (insn, x, y, expect) in cases {
        let m = MethodBuilder::new(2, 4)
            .insns(vec![insn, Ret { a: 3 }])
            .build(&mut vm);
        assert_eq!(vm.call(m, vm.nil(), &[int(x), int(y)]).unwrap(), expect);
    }
}

#[test]
fn ideq_is_universal_raw_compare() {
    let mut vm = Vm::bare_test();
    let m = MethodBuilder::new(2, 4)
        .insns(vec![IdEq { d: 3, a: 1, b: 2 }, Ret { a: 3 }])
        .build(&mut vm);
    let class = vm.new_test_class(FMT_FIXED, 0);
    let a = vm.make_instance(class).unwrap();
    let b = vm.make_instance(class).unwrap();
    assert_eq!(vm.call(m, vm.nil(), &[a, a]).unwrap(), vm.true_v());
    assert_eq!(vm.call(m, vm.nil(), &[a, b]).unwrap(), vm.false_v());
    assert_eq!(vm.call(m, vm.nil(), &[int(3), int(3)]).unwrap(), vm.true_v());
    assert_eq!(vm.call(m, vm.nil(), &[int(3), a]).unwrap(), vm.false_v());
}

#[test]
fn not_flips_booleans() {
    let mut vm = Vm::bare_test();
    let m = MethodBuilder::new(1, 3)
        .insns(vec![Not { d: 2, a: 1 }, Ret { a: 2 }])
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[vm.true_v()]).unwrap(), vm.false_v());
    assert_eq!(vm.call(m, vm.nil(), &[vm.false_v()]).unwrap(), vm.true_v());
}

#[test]
fn classof_reads_class_table() {
    let mut vm = Vm::bare_test();
    let m = MethodBuilder::new(1, 3)
        .insns(vec![ClassOf { d: 2, a: 1 }, Ret { a: 2 }])
        .build(&mut vm);
    assert_eq!(
        vm.call(m, vm.nil(), &[int(3)]).unwrap(),
        vm.class_table_at(CLASS_SMALLINTEGER)
    );
    assert_eq!(
        vm.call(m, vm.nil(), &[vm.nil()]).unwrap(),
        vm.class_table_at(CLASS_UNDEFINED_OBJECT)
    );
}

#[test]
fn at_atput_size_on_arrays_and_bytes() {
    let mut vm = Vm::bare_test();
    let arr = vm.make_array(&[int(10), int(20), int(30)]).unwrap();

    let size_m = MethodBuilder::new(1, 3)
        .insns(vec![Size { d: 2, a: 1 }, Ret { a: 2 }])
        .build(&mut vm);
    assert_eq!(vm.call(size_m, vm.nil(), &[arr]).unwrap(), int(3));

    // at: is 1-indexed
    let at_m = MethodBuilder::new(2, 4)
        .insns(vec![At { d: 3, a: 1, b: 2 }, Ret { a: 3 }])
        .build(&mut vm);
    assert_eq!(vm.call(at_m, vm.nil(), &[arr, int(1)]).unwrap(), int(10));
    assert_eq!(vm.call(at_m, vm.nil(), &[arr, int(3)]).unwrap(), int(30));

    // ATPUT: receiver a=1, index b=2, value in slot b+1=3; result d.
    let atput_m = MethodBuilder::new(3, 5)
        .insns(vec![AtPut { d: 4, a: 1, b: 2 }, Ret { a: 4 }])
        .build(&mut vm);
    assert_eq!(
        vm.call(atput_m, vm.nil(), &[arr, int(2), int(99)]).unwrap(),
        int(99)
    );
    assert_eq!(vm.heap.slot(arr.as_ptr(), 1), int(99));

    // Byte objects: at: yields bytes as SmallIntegers ('héllo' size = 6).
    let s = vm.make_string("héllo").unwrap();
    assert_eq!(vm.call(size_m, vm.nil(), &[s]).unwrap(), int(6));
    assert_eq!(vm.call(at_m, vm.nil(), &[s, int(1)]).unwrap(), int('h' as i64));
    assert_eq!(vm.call(at_m, vm.nil(), &[s, int(2)]).unwrap(), int(0xC3));
}

// --- Specialized sends: slow paths fall through to real sends ---

#[test]
fn add_overflow_falls_to_send() {
    let mut vm = Vm::bare_test();
    // SmallInteger>>+ installed as plain bytecode returning a marker.
    let plus = MethodBuilder::new(1, 2)
        .insns(vec![LoadInt { d: 1, imm: -999 }, Ret { a: 1 }])
        .build(&mut vm);
    let plus_sel = vm.intern("+");
    vm.install_method(vm.class_table_at(CLASS_SMALLINTEGER), plus_sel, plus);

    let m = MethodBuilder::new(2, 6)
        .insns(vec![Add { d: 3, a: 1, b: 2 }, Ret { a: 3 }])
        .build(&mut vm);
    // Fast path unaffected:
    assert_eq!(vm.call(m, vm.nil(), &[int(1), int(2)]).unwrap(), int(3));
    // Overflow → send #+ → marker.
    assert_eq!(
        vm.call(m, vm.nil(), &[int(Value::SMALLINT_MAX), int(1)]).unwrap(),
        int(-999)
    );
}

#[test]
fn division_by_zero_falls_to_send() {
    let mut vm = Vm::bare_test();
    let div = MethodBuilder::new(1, 2)
        .insns(vec![LoadInt { d: 1, imm: -888 }, Ret { a: 1 }])
        .build(&mut vm);
    let sel = vm.intern("//");
    vm.install_method(vm.class_table_at(CLASS_SMALLINTEGER), sel, div);

    let m = MethodBuilder::new(2, 6)
        .insns(vec![Div { d: 3, a: 1, b: 2 }, Ret { a: 3 }])
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[int(7), int(0)]).unwrap(), int(-888));
}

#[test]
fn specialized_send_on_user_class_takes_slow_path() {
    let mut vm = Vm::bare_test();
    let class = vm.new_test_class(FMT_FIXED, 0);
    // Install #+ on the user class: it works via the ordinary send machinery.
    let plus = MethodBuilder::new(1, 3)
        .insns(vec![
            LoadInt { d: 2, imm: 1000 },
            Add { d: 2, a: 2, b: 1 },
            Ret { a: 2 },
        ])
        .build(&mut vm);
    let sel = vm.intern("+");
    vm.install_method(class, sel, plus);

    let obj = vm.make_instance(class).unwrap();
    let m = MethodBuilder::new(2, 6)
        .insns(vec![Add { d: 3, a: 1, b: 2 }, Ret { a: 3 }])
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[obj, int(7)]).unwrap(), int(1007));
}

#[test]
fn at_on_fixed_object_takes_slow_path() {
    let mut vm = Vm::bare_test();
    let class = vm.new_test_class(FMT_FIXED, 1);
    let at_m = MethodBuilder::new(1, 3)
        .insns(vec![LoadInt { d: 2, imm: 777 }, Ret { a: 2 }])
        .build(&mut vm);
    let sel = vm.intern("at:");
    vm.install_method(class, sel, at_m);

    let obj = vm.make_instance(class).unwrap();
    let m = MethodBuilder::new(2, 6)
        .insns(vec![At { d: 3, a: 1, b: 2 }, Ret { a: 3 }])
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[obj, int(1)]).unwrap(), int(777));
}

#[test]
fn atput_immutable_falls_to_send() {
    let mut vm = Vm::bare_test();
    // Immutable Array literal: at:put: must not touch it on the fast path.
    let arr = vm.make_array(&[int(1)]).unwrap();
    vm.heap.set_immutable(arr.as_ptr());

    let handler = MethodBuilder::new(2, 4)
        .insns(vec![LoadInt { d: 3, imm: -111 }, Ret { a: 3 }])
        .build(&mut vm);
    let sel = vm.intern("at:put:");
    vm.install_method(vm.class_table_at(CLASS_ARRAY), sel, handler);

    let m = MethodBuilder::new(3, 6)
        .insns(vec![AtPut { d: 4, a: 1, b: 2 }, Ret { a: 4 }])
        .build(&mut vm);
    assert_eq!(
        vm.call(m, vm.nil(), &[arr, int(1), int(5)]).unwrap(),
        int(-111)
    );
    assert_eq!(vm.heap.slot(arr.as_ptr(), 0), int(1), "unchanged");
}

#[test]
fn at_out_of_bounds_falls_to_send() {
    let mut vm = Vm::bare_test();
    let handler = MethodBuilder::new(1, 2)
        .insns(vec![LoadInt { d: 1, imm: -222 }, Ret { a: 1 }])
        .build(&mut vm);
    let sel = vm.intern("at:");
    vm.install_method(vm.class_table_at(CLASS_ARRAY), sel, handler);

    let arr = vm.make_array(&[int(1), int(2)]).unwrap();
    let m = MethodBuilder::new(2, 6)
        .insns(vec![At { d: 3, a: 1, b: 2 }, Ret { a: 3 }])
        .build(&mut vm);
    assert_eq!(vm.call(m, vm.nil(), &[arr, int(0)]).unwrap(), int(-222));
    assert_eq!(vm.call(m, vm.nil(), &[arr, int(3)]).unwrap(), int(-222));
}
