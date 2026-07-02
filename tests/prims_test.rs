//! Phase 1 primitive tests (SPEC §16): every primitive either fully
//! succeeds or fails cleanly into its fallback body. Fallback bodies here
//! return the failure code (a positive SmallInteger), so tests observe both
//! paths.

use smallishtalk::asm::Insn::*;
use smallishtalk::fixture::MethodBuilder;
use smallishtalk::treaty::*;
use smallishtalk::value::Value;
use smallishtalk::vm::Vm;

fn int(n: i64) -> Value {
    Value::from_int(n)
}

/// Install `selector` on `class` as primitive `prim`; the fallback body
/// answers the (positive) failure code.
fn install_prim(vm: &mut Vm, class: Value, selector: &str, prim: u16, argc: u8) {
    let m = MethodBuilder::new(argc, argc + 3)
        .primitive(prim)
        .insns(vec![Ret { a: argc + 1 }]) // the failure-code slot
        .build(vm);
    let sel = vm.intern(selector);
    vm.install_method(class, sel, m);
}

/// Evaluate `recv sel: args...` via a fresh one-shot method.
fn send(vm: &mut Vm, sel: &str, recv: Value, args: &[Value]) -> Value {
    let mut lits = vec![recv];
    lits.extend_from_slice(args);
    let mut insns = vec![LoadK { d: 4, k: 0 }];
    for i in 0..args.len() {
        insns.push(LoadK {
            d: (5 + i) as u8,
            k: (1 + i) as u16,
        });
    }
    insns.push(Send { d: 1, r: 4, site: 0 });
    insns.push(Ret { a: 1 });
    let m = MethodBuilder::new(0, (7 + args.len()) as u8)
        .insns(insns)
        .literals(lits)
        .site_named(vm, sel, args.len() as u8)
        .build(vm);
    vm.call(m, vm.nil(), &[]).unwrap()
}

fn object_class(vm: &Vm) -> Value {
    vm.class_table_at(CLASS_OBJECT)
}

// --- Object essentials ---

#[test]
fn prim_class_and_identity() {
    let mut vm = Vm::bare_test();
    let obj_cls = object_class(&vm);
    install_prim(&mut vm, obj_cls, "class", PRIM_CLASS, 0);
    install_prim(&mut vm, obj_cls, "==", PRIM_IDENTICAL, 1);
    install_prim(&mut vm, obj_cls, "identityHash", PRIM_IDENTITY_HASH, 0);

    assert_eq!(
        send(&mut vm, "class", int(5), &[]),
        vm.class_table_at(CLASS_SMALLINTEGER)
    );
    let s = vm.make_string("x").unwrap();
    assert_eq!(send(&mut vm, "class", s, &[]), vm.class_table_at(CLASS_BYTESTRING));

    let t = vm.true_v();
    assert_eq!(send(&mut vm, "==", t, &[t]), vm.true_v());
    let f = vm.false_v();
    assert_eq!(send(&mut vm, "==", t, &[f]), vm.false_v());

    // SmallIntegers hash as their value; objects get a stable nonzero hash.
    assert_eq!(send(&mut vm, "identityHash", int(1234), &[]), int(1234));
    let arr = vm.make_array(&[]).unwrap();
    let h1 = send(&mut vm, "identityHash", arr, &[]);
    let h2 = send(&mut vm, "identityHash", arr, &[]);
    assert_eq!(h1, h2);
    assert!(h1.as_int() > 0);
}

#[test]
fn prim_new_and_new_sized() {
    let mut vm = Vm::bare_test();
    let obj_cls = object_class(&vm);
    install_prim(&mut vm, obj_cls, "new", PRIM_NEW, 0);
    install_prim(&mut vm, obj_cls, "new:", PRIM_NEW_SIZED, 1);

    let point = vm.new_test_class(FMT_FIXED, 2);
    let inst = send(&mut vm, "new", point, &[]);
    assert!(inst.is_ptr());
    assert_eq!(vm.class_of(inst), point);
    assert_eq!(vm.heap.slot(inst.as_ptr(), 0), vm.nil());

    let arr_cls = vm.class_table_at(CLASS_ARRAY);
    let arr = send(&mut vm, "new:", arr_cls, &[int(5)]);
    assert_eq!(vm.heap.num_slots(arr.as_ptr()), 5);
    assert_eq!(vm.class_of(arr), arr_cls);

    let ba_cls = vm.class_table_at(CLASS_BYTEARRAY);
    let ba = send(&mut vm, "new:", ba_cls, &[int(9)]);
    assert_eq!(vm.heap.byte_size(ba.as_ptr()), 9);

    // new on an indexable class fails cleanly (code > 0).
    let r = send(&mut vm, "new", arr_cls, &[]);
    assert!(r.is_int() && r.as_int() > 0, "expected failure code, got {r:?}");
}

#[test]
fn prim_at_family() {
    let mut vm = Vm::bare_test();
    let obj_cls = object_class(&vm);
    install_prim(&mut vm, obj_cls, "basicAt:", PRIM_AT, 1);
    install_prim(&mut vm, obj_cls, "basicAt:put:", PRIM_AT_PUT, 2);
    install_prim(&mut vm, obj_cls, "basicSize", PRIM_SIZE, 0);

    let arr = vm.make_array(&[int(10), int(20)]).unwrap();
    assert_eq!(send(&mut vm, "basicAt:", arr, &[int(2)]), int(20));
    assert_eq!(send(&mut vm, "basicAt:put:", arr, &[int(1), int(99)]), int(99));
    assert_eq!(vm.heap.slot(arr.as_ptr(), 0), int(99));
    assert_eq!(send(&mut vm, "basicSize", arr, &[]), int(2));

    // Out of bounds / immutable / wrong receiver → clean failures.
    assert!(send(&mut vm, "basicAt:", arr, &[int(3)]).as_int() > 0);
    assert!(send(&mut vm, "basicAt:", int(5), &[int(1)]).as_int() > 0);
    vm.heap.set_immutable(arr.as_ptr());
    assert!(send(&mut vm, "basicAt:put:", arr, &[int(1), int(5)]).as_int() > 0);

    let s = vm.make_string("hé").unwrap();
    assert_eq!(send(&mut vm, "basicSize", s, &[]), int(3), "UTF-8 bytes");
    assert_eq!(send(&mut vm, "basicAt:", s, &[int(1)]), int('h' as i64));
}

#[test]
fn prim_inst_var_access() {
    let mut vm = Vm::bare_test();
    let obj_cls = object_class(&vm);
    install_prim(&mut vm, obj_cls, "instVarAt:", PRIM_INST_VAR_AT, 1);
    install_prim(&mut vm, obj_cls, "instVarAt:put:", PRIM_INST_VAR_AT_PUT, 2);

    let point = vm.new_test_class(FMT_FIXED, 2);
    let p = vm.make_instance(point).unwrap();
    assert_eq!(send(&mut vm, "instVarAt:put:", p, &[int(1), int(7)]), int(7));
    assert_eq!(send(&mut vm, "instVarAt:", p, &[int(1)]), int(7));
    assert_eq!(vm.heap.slot(p.as_ptr(), 0), int(7));
    assert!(send(&mut vm, "instVarAt:", p, &[int(3)]).as_int() > 0);
}

#[test]
fn prim_perform_with_arguments() {
    let mut vm = Vm::bare_test();
    let obj_cls = object_class(&vm);
    install_prim(&mut vm, obj_cls, "perform:withArguments:", PRIM_PERFORM_WITH_ARGS, 2);

    let class = vm.new_test_class(FMT_FIXED, 0);
    let sel = vm.intern("sum:and:");
    let m = MethodBuilder::new(2, 4)
        .insns(vec![Add { d: 3, a: 1, b: 2 }, Ret { a: 3 }])
        .build(&mut vm);
    vm.install_method(class, sel, m);

    let obj = vm.make_instance(class).unwrap();
    let sel_v = vm.intern("sum:and:");
    let args = vm.make_array(&[int(30), int(12)]).unwrap();
    assert_eq!(
        send(&mut vm, "perform:withArguments:", obj, &[sel_v, args]),
        int(42)
    );
}

// --- SmallInteger arithmetic ---

#[test]
fn prim_int_arithmetic() {
    let mut vm = Vm::bare_test();
    let si = vm.class_table_at(CLASS_SMALLINTEGER);
    for (sel, prim) in [
        ("p+", PRIM_INT_ADD),
        ("p-", PRIM_INT_SUB),
        ("p*", PRIM_INT_MUL),
        ("p//", PRIM_INT_DIV),
        ("p\\\\", PRIM_INT_MOD),
        ("pquo:", PRIM_INT_QUO),
        ("pbitAnd:", PRIM_INT_BIT_AND),
        ("pbitOr:", PRIM_INT_BIT_OR),
        ("pbitXor:", PRIM_INT_BIT_XOR),
        ("pbitShift:", PRIM_INT_BIT_SHIFT),
    ] {
        install_prim(&mut vm, si, sel, prim, 1);
    }
    assert_eq!(send(&mut vm, "p+", int(3), &[int(4)]), int(7));
    assert_eq!(send(&mut vm, "p-", int(3), &[int(4)]), int(-1));
    assert_eq!(send(&mut vm, "p*", int(-6), &[int(7)]), int(-42));
    assert_eq!(send(&mut vm, "p//", int(-7), &[int(2)]), int(-4), "floored");
    assert_eq!(send(&mut vm, "p\\\\", int(-7), &[int(2)]), int(1), "floored");
    assert_eq!(send(&mut vm, "pquo:", int(-7), &[int(2)]), int(-3), "truncated");
    assert_eq!(send(&mut vm, "pbitAnd:", int(12), &[int(10)]), int(8));
    assert_eq!(send(&mut vm, "pbitOr:", int(12), &[int(10)]), int(14));
    assert_eq!(send(&mut vm, "pbitXor:", int(12), &[int(10)]), int(6));
    assert_eq!(send(&mut vm, "pbitShift:", int(3), &[int(4)]), int(48));
    assert_eq!(send(&mut vm, "pbitShift:", int(-16), &[int(-2)]), int(-4));

    // Overflow, div-by-zero, wrong type: clean failures.
    assert!(send(&mut vm, "p+", int(Value::SMALLINT_MAX), &[int(1)]).as_int() > 0);
    assert!(send(&mut vm, "p//", int(1), &[int(0)]).as_int() > 0);
    let s = vm.make_string("x").unwrap();
    assert!(send(&mut vm, "p+", int(1), &[s]).as_int() > 0);
}

#[test]
fn prim_int_comparisons() {
    let mut vm = Vm::bare_test();
    let si = vm.class_table_at(CLASS_SMALLINTEGER);
    for (sel, prim) in [
        ("p<", PRIM_INT_LT),
        ("p>", PRIM_INT_GT),
        ("p<=", PRIM_INT_LE),
        ("p>=", PRIM_INT_GE),
        ("p=", PRIM_INT_EQ),
    ] {
        install_prim(&mut vm, si, sel, prim, 1);
    }
    let t = vm.true_v();
    let f = vm.false_v();
    assert_eq!(send(&mut vm, "p<", int(1), &[int(2)]), t);
    assert_eq!(send(&mut vm, "p>", int(1), &[int(2)]), f);
    assert_eq!(send(&mut vm, "p<=", int(2), &[int(2)]), t);
    assert_eq!(send(&mut vm, "p>=", int(1), &[int(2)]), f);
    assert_eq!(send(&mut vm, "p=", int(2), &[int(2)]), t);
}

// --- Float ---

#[test]
fn prim_float_arithmetic() {
    let mut vm = Vm::bare_test();
    let si = vm.class_table_at(CLASS_SMALLINTEGER);
    let fc = vm.class_table_at(CLASS_FLOAT);
    install_prim(&mut vm, si, "asFloat", PRIM_INT_AS_FLOAT, 0);
    for (sel, prim) in [
        ("f+", PRIM_FLOAT_ADD),
        ("f-", PRIM_FLOAT_SUB),
        ("f*", PRIM_FLOAT_MUL),
        ("f/", PRIM_FLOAT_DIV),
        ("f<", PRIM_FLOAT_LT),
        ("f=", PRIM_FLOAT_EQ),
    ] {
        install_prim(&mut vm, fc, sel, prim, 1);
    }
    install_prim(&mut vm, fc, "truncated", PRIM_FLOAT_TRUNCATED, 0);
    install_prim(&mut vm, fc, "sqrt", PRIM_FLOAT_SQRT, 0);

    let three = send(&mut vm, "asFloat", int(3), &[]);
    assert!(three.is_ptr());
    assert_eq!(vm.heap.header(three.as_ptr()).class_index(), CLASS_FLOAT);
    assert_eq!(vm.float_value(three), 3.0);

    let half = vm.make_float(0.5).unwrap();
    let sum = send(&mut vm, "f+", three, &[half]);
    assert_eq!(vm.float_value(sum), 3.5);
    let diff = send(&mut vm, "f-", three, &[half]);
    assert_eq!(vm.float_value(diff), 2.5);
    let prod = send(&mut vm, "f*", three, &[half]);
    assert_eq!(vm.float_value(prod), 1.5);
    let quot = send(&mut vm, "f/", three, &[half]);
    assert_eq!(vm.float_value(quot), 6.0);

    assert_eq!(send(&mut vm, "f<", half, &[three]), vm.true_v());
    let three2 = vm.make_float(3.0).unwrap();
    assert_eq!(send(&mut vm, "f=", three, &[three2]), vm.true_v());

    assert_eq!(send(&mut vm, "truncated", quot, &[]), int(6));
    let nine = vm.make_float(9.0).unwrap();
    let root = send(&mut vm, "sqrt", nine, &[]);
    assert_eq!(vm.float_value(root), 3.0);
}

// --- valueWithArguments: ---

#[test]
fn prim_value_with_arguments() {
    let mut vm = Vm::bare_test();
    let bc = vm.class_table_at(CLASS_BLOCKCLOSURE);
    install_prim(&mut vm, bc, "valueWithArguments:", PRIM_BLOCK_VALUE_ARGS, 1);

    let nil = vm.nil();
    let blk = MethodBuilder::new(2, 4)
        .insns(vec![Sub { d: 3, a: 1, b: 2 }, Ret { a: 3 }])
        .build_block(&mut vm, nil, 0, false);
    let mk = MethodBuilder::new(0, 6)
        .insns(vec![MkClosure { d: 4, b: 0 }, Ret { a: 4 }])
        .literals(vec![blk])
        .build(&mut vm);
    let closure = vm.call(mk, vm.nil(), &[]).unwrap();

    let args = vm.make_array(&[int(50), int(8)]).unwrap();
    assert_eq!(
        send(&mut vm, "valueWithArguments:", closure, &[args]),
        int(42)
    );
    // Wrong count fails cleanly.
    let bad = vm.make_array(&[int(1)]).unwrap();
    assert!(send(&mut vm, "valueWithArguments:", closure, &[bad]).as_int() > 0);
}

// --- Clocks ---

#[test]
fn prim_clocks() {
    let mut vm = Vm::bare_test();
    let nil = vm.nil();
    let obj_cls = object_class(&vm);
    install_prim(&mut vm, obj_cls, "monoMs", PRIM_CLOCK_MONOTONIC_MS, 0);
    install_prim(&mut vm, obj_cls, "wallMs", PRIM_CLOCK_WALL_MS, 0);

    let t1 = send(&mut vm, "monoMs", nil, &[]);
    let t2 = send(&mut vm, "monoMs", nil, &[]);
    assert!(t1.is_int() && t2.is_int() && t2.as_int() >= t1.as_int());

    let w = send(&mut vm, "wallMs", nil, &[]);
    // Sanity: after 2020-01-01 (in ms).
    assert!(w.as_int() > 1_577_836_800_000);
}

// --- stdio and files ---

#[test]
fn prim_stdio_write_captures() {
    let mut vm = Vm::bare_test();
    let nil = vm.nil();
    let obj_cls = object_class(&vm);
    install_prim(&mut vm, obj_cls, "stdioWrite:on:", PRIM_STDIO_WRITE, 2);

    let s = vm.make_string("hello, world").unwrap();
    let n = send(&mut vm, "stdioWrite:on:", nil, &[s, int(1)]);
    assert_eq!(n, int(12));
    assert_eq!(vm.stdout_capture, b"hello, world");
}

#[test]
fn prim_file_roundtrip() {
    let mut vm = Vm::bare_test();
    let nil = vm.nil();
    let obj_cls = object_class(&vm);
    install_prim(&mut vm, obj_cls, "fopen:mode:", PRIM_FILE_OPEN, 2);
    install_prim(&mut vm, obj_cls, "fclose:", PRIM_FILE_CLOSE, 1);
    install_prim(&mut vm, obj_cls, "fread:count:", PRIM_FILE_READ, 2);
    install_prim(&mut vm, obj_cls, "fwrite:bytes:", PRIM_FILE_WRITE, 2);
    install_prim(&mut vm, obj_cls, "fpos:", PRIM_FILE_POSITION, 1);
    install_prim(&mut vm, obj_cls, "fsetpos:to:", PRIM_FILE_SET_POSITION, 2);
    install_prim(&mut vm, obj_cls, "fsize:", PRIM_FILE_SIZE, 1);
    install_prim(&mut vm, obj_cls, "fdelete:", PRIM_FILE_DELETE, 1);

    let dir = std::env::temp_dir().join(format!("smallishtalk-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("prim_file_roundtrip.bin");
    let path_v = vm.make_string(path.to_str().unwrap()).unwrap();

    // mode 1 = write (create/truncate)
    let fd = send(&mut vm, "fopen:mode:", nil, &[path_v, int(1)]);
    assert!(fd.is_int() && fd.as_int() >= 0);
    let data = vm.make_string("smalltalk bytes").unwrap();
    let n = send(&mut vm, "fwrite:bytes:", nil, &[fd, data]);
    assert_eq!(n, int(15));
    assert_eq!(send(&mut vm, "fpos:", nil, &[fd]), int(15));
    assert_eq!(send(&mut vm, "fclose:", nil, &[fd]), vm.nil());

    // mode 0 = read
    let path_v = vm.make_string(path.to_str().unwrap()).unwrap();
    let fd = send(&mut vm, "fopen:mode:", nil, &[path_v, int(0)]);
    assert_eq!(send(&mut vm, "fsize:", nil, &[fd]), int(15));
    send(&mut vm, "fsetpos:to:", nil, &[fd, int(10)]);
    let tail = send(&mut vm, "fread:count:", nil, &[fd, int(100)]);
    assert!(tail.is_ptr());
    assert_eq!(vm.heap.bytes(tail.as_ptr()), b"bytes");
    send(&mut vm, "fclose:", nil, &[fd]);

    let path_v = vm.make_string(path.to_str().unwrap()).unwrap();
    assert_eq!(send(&mut vm, "fdelete:", nil, &[path_v]), vm.nil());
    assert!(!path.exists());
    std::fs::remove_dir_all(&dir).ok();
}

// --- System ---

#[test]
fn prim_register_class_and_method_install() {
    let mut vm = Vm::bare_test();
    let obj_cls = object_class(&vm);
    install_prim(&mut vm, obj_cls, "registerClass", PRIM_REGISTER_CLASS, 0);
    install_prim(&mut vm, obj_cls, "install:as:", PRIM_METHOD_INSTALL, 2);

    // Build a raw class object (as the image-side compiler would).
    let nil = vm.nil();
    let object = vm.class_table_at(CLASS_OBJECT);
    let raw = vm
        .heap
        .alloc_fixed_old(CLASS_CLASS, BEHAVIOR_NUM_VM_SLOTS, nil)
        .unwrap();
    vm.heap.set_slot_raw(raw, BEHAVIOR_SUPERCLASS, object);
    let mdict = vm.make_instance(vm.class_table_at(CLASS_METHODDICTIONARY));
    let mdict = mdict.unwrap();
    let empty = vm.make_array(&[]).unwrap();
    vm.store_slot(mdict.as_ptr(), MDICT_KEYS, empty);
    let empty2 = vm.make_array(&[]).unwrap();
    vm.store_slot(mdict.as_ptr(), MDICT_VALUES, empty2);
    vm.store_slot(raw, BEHAVIOR_METHOD_DICTIONARY, mdict);
    vm.heap.set_slot_raw(
        raw,
        BEHAVIOR_FORMAT_AND_SLOTS,
        int((FMT_FIXED as i64) << FORMAT_AND_SLOTS_FORMAT_SHIFT | 1),
    );
    let class = Value::from_ptr(raw);

    let idx = send(&mut vm, "registerClass", class, &[]);
    assert!(idx.as_int() >= FIRST_UNRESERVED_CLASS_INDEX as i64);
    assert_eq!(vm.class_table_at(idx.as_int() as u32), class);
    assert_eq!(
        vm.heap.slot(raw, BEHAVIOR_CLASS_INDEX),
        idx,
        "index stamped into slot 3"
    );

    // Install a method via the primitive and call it.
    let m = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 31 }, Ret { a: 1 }])
        .build(&mut vm);
    let sel = vm.intern("answer");
    send(&mut vm, "install:as:", class, &[sel, m]);

    let inst = vm.make_instance(class).unwrap();
    assert_eq!(send(&mut vm, "answer", inst, &[]), int(31));
    assert_eq!(vm.heap.slot(m.as_ptr(), METHOD_SELECTOR), sel);
    assert_eq!(vm.heap.slot(m.as_ptr(), METHOD_CLASS), class);
}

/// primFrameInfo (§19): decode a suspended process's frames, including the
/// resume pc of frames below the top (recovered from callee returnInfo).
#[test]
fn prim_frame_info_decodes_suspended_frames() {
    let mut vm = Vm::bare_test();
    let obj_cls = object_class(&vm);
    install_prim(&mut vm, obj_cls, "frameInfo:at:", PRIM_FRAME_INFO, 2);
    let proc_cls = vm.class_table_at(CLASS_PROCESS);
    install_prim(&mut vm, proc_cls, "transferTo:", PRIM_TRANSFER_TO, 1);

    // Worker: method A sends #b to self; B transfers back to main, leaving
    // the worker suspended with two frames.
    let class = vm.new_test_class(FMT_FIXED, 0);
    let main_box = vm.make_instance(vm.class_table_at(CLASS_BOX)).unwrap();
    let b = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 1, k: 0 },
            GetBox { d: 4, a: 1 },
            LoadNil { d: 5 },
            Send { d: 2, r: 4, site: 0 }, // transfer back to main
            RetSelf,
        ])
        .literals(vec![main_box])
        .site_named(&mut vm, "transferTo:", 1)
        .build(&mut vm);
    let b_sel = vm.intern("b");
    vm.install_method(class, b_sel, b);
    // Stage at r=8 so A's own receiver (slot 0) survives the send and can
    // be decoded afterwards.
    let a = MethodBuilder::new(0, 10)
        .insns(vec![
            LoadSelf { d: 8 },
            Send { d: 1, r: 8, site: 0 }, // resume pc = 2 after this send
            Ret { a: 1 },
        ])
        .site_named(&mut vm, "b", 0)
        .build(&mut vm);
    let recv = vm.make_instance(class).unwrap();
    let worker = vm.spawn_process(a, recv, &[]).unwrap();

    // Main: transfer to worker; it comes right back.
    let m_m = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            LoadNil { d: 5 },
            Send { d: 1, r: 4, site: 0 },
            LoadInt { d: 1, imm: 1 },
            Ret { a: 1 },
        ])
        .literals(vec![worker])
        .site_named(&mut vm, "transferTo:", 1)
        .build(&mut vm);
    let main = vm.spawn_process(m_m, vm.nil(), &[]).unwrap();
    vm.heap.set_slot_raw(main_box.as_ptr(), 0, main);
    vm.write_barrier(main_box.as_ptr(), main);
    assert_eq!(vm.run(main).unwrap(), int(1));

    // Worker is suspended inside B (called from A at base offset 1).
    let nil = vm.nil();
    let top_off = vm.heap.slot(worker.as_ptr(), PROCESS_FRAME_OFFSET);
    let info_b = send(&mut vm, "frameInfo:at:", nil, &[worker, top_off]);
    assert!(info_b.is_ptr());
    assert_eq!(vm.heap.slot(info_b.as_ptr(), 0), b, "top frame method is B");
    assert_eq!(
        vm.heap.slot(info_b.as_ptr(), 1),
        vm.heap.slot(worker.as_ptr(), PROCESS_PC),
        "top frame pc is the saved pc"
    );
    assert_eq!(vm.heap.slot(info_b.as_ptr(), 2), recv, "receiver decoded");

    let info_a = send(&mut vm, "frameInfo:at:", nil, &[worker, int(1)]);
    assert_eq!(vm.heap.slot(info_a.as_ptr(), 0), a, "base frame method is A");
    assert_eq!(
        vm.heap.slot(info_a.as_ptr(), 1),
        int(2),
        "A's resume pc recovered from B's returnInfo"
    );
    assert_eq!(vm.heap.slot(info_a.as_ptr(), 2), recv);
}
