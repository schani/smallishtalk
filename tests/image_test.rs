//! Phase 1 tests: image snapshot save/load (SPEC §17) — the STIM format,
//! pointer relocation by delta, class table and registry rebuild, and the
//! classic snapshot-returns-true-on-resume idiom.

use smallishtalk::asm::Insn::*;
use smallishtalk::fixture::MethodBuilder;
use smallishtalk::treaty::*;
use smallishtalk::value::Value;
use smallishtalk::vm::{Vm, VmConfig};

fn int(n: i64) -> Value {
    Value::from_int(n)
}

fn temp_image_path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("smallishtalk-img-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn install_snapshot_prim(vm: &mut Vm) {
    let object = vm.class_table_at(CLASS_OBJECT);
    let m = MethodBuilder::new(1, 4)
        .primitive(PRIM_SNAPSHOT)
        .insns(vec![Ret { a: 2 }])
        .build(vm);
    let sel = vm.intern("snapshot:");
    vm.install_method(object, sel, m);
}

/// The classic idiom: a program snapshots mid-run; the original run takes
/// the false branch, the resumed image takes the true branch — with all
/// pre-snapshot state (a box, a helper send) intact.
#[test]
fn snapshot_round_trip_resumes_mid_method() {
    let path = temp_image_path("roundtrip.im");
    let mut vm = Vm::bare_test();
    install_snapshot_prim(&mut vm);

    // Helper class so the resumed image proves method lookup still works.
    let class = vm.new_test_class(FMT_FIXED, 0);
    let sel = vm.intern("plus1000:");
    let helper = MethodBuilder::new(1, 4)
        .insns(vec![
            LoadInt { d: 2, imm: 1000 },
            Add { d: 3, a: 1, b: 2 },
            Ret { a: 3 },
        ])
        .build(&mut vm);
    vm.install_method(class, sel, helper);
    let obj = vm.make_instance(class).unwrap();

    let boxv = vm.make_instance(vm.class_table_at(CLASS_BOX)).unwrap();
    vm.heap.set_slot_raw(boxv.as_ptr(), 0, int(20));
    let path_v = vm.make_string(path.to_str().unwrap()).unwrap();

    // x := box value. r := nil snapshot: path.
    // y := x + 22. r == true ifTrue: [^obj plus1000: y] ifFalse: [^y]
    let main = MethodBuilder::new(0, 12)
        .insns(vec![
            LoadK { d: 1, k: 0 },      // box
            GetBox { d: 2, a: 1 },     // x = 20
            LoadNil { d: 8 },
            LoadK { d: 9, k: 1 },      // path
            Send { d: 3, r: 8, site: 0 }, // r := snapshot
            LoadInt { d: 4, imm: 22 },
            Add { d: 5, a: 2, b: 4 },  // y = 42
            LoadTrue { d: 6 },
            IdEq { d: 7, a: 3, b: 6 },
            JumpFalse { a: 7, off: 3 },
            LoadK { d: 8, k: 2 },      // obj
            Move { d: 9, a: 5 },
            Send { d: 1, r: 8, site: 1 },
            Ret { a: 1 },              // resumed image: 1042  (pc 13)
        ])
        .literals(vec![boxv, path_v, obj])
        .site_named(&mut vm, "snapshot:", 1)
        .site_named(&mut vm, "plus1000:", 1)
        .build(&mut vm);
    // JumpFalse target: pc 10+3 = 13 → Ret? No: false branch must return y.
    // (See adjusted instruction list below — the last Ret handles the true
    // branch; append the false-branch Ret at 14.)
    let main = {
        let _ = main;
        MethodBuilder::new(0, 12)
            .insns(vec![
                LoadK { d: 1, k: 0 },
                GetBox { d: 2, a: 1 },
                LoadNil { d: 8 },
                LoadK { d: 9, k: 1 },
                Send { d: 3, r: 8, site: 0 },
                LoadInt { d: 4, imm: 22 },
                Add { d: 5, a: 2, b: 4 },
                LoadTrue { d: 6 },
                IdEq { d: 7, a: 3, b: 6 },
                JumpFalse { a: 7, off: 4 }, // → pc 14 (false branch)
                LoadK { d: 8, k: 2 },
                Move { d: 9, a: 5 },
                Send { d: 1, r: 8, site: 1 },
                Ret { a: 1 },     // true branch: 1042
                Ret { a: 5 },     // false branch: 42
            ])
            .literals(vec![boxv, path_v, obj])
            .site_named(&mut vm, "snapshot:", 1)
            .site_named(&mut vm, "plus1000:", 1)
            .build(&mut vm)
    };

    // Original run: snapshot answers false → 42.
    assert_eq!(vm.call(main, vm.nil(), &[]).unwrap(), int(42));
    assert!(path.exists());

    // Load and continue: snapshot answers true → helper send → 1042.
    let mut vm2 = Vm::load_image(path.to_str().unwrap(), VmConfig::default()).unwrap();
    let active = vm2.active_process;
    assert!(active.is_ptr());
    assert_eq!(vm2.run(active).unwrap(), int(1042));

    std::fs::remove_file(&path).ok();
}

#[test]
fn data_survives_save_load() {
    let path = temp_image_path("data.im");
    let mut vm = Vm::bare_test();
    install_snapshot_prim(&mut vm);

    // Park a data structure in the Smalltalk special slot.
    let s = vm.make_string("héllo wörld").unwrap();
    let f = vm.make_float(2.5).unwrap();
    let sym = vm.intern("someSelector:");
    let big = {
        let nil = vm.nil();
        let addr = vm.heap.alloc_ptrs(CLASS_ARRAY, 300, nil).unwrap();
        vm.heap.set_slot_raw(addr, 299, int(7));
        Value::from_ptr(addr)
    };
    let inner = vm.make_array(&[int(1), s, f]).unwrap();
    let outer = vm.make_array(&[inner, sym, big, vm.true_v()]).unwrap();
    vm.set_special(SPECIAL_SMALLTALK, outer);

    let path_v = vm.make_string(path.to_str().unwrap()).unwrap();
    let main = MethodBuilder::new(0, 12)
        .insns(vec![
            LoadNil { d: 8 },
            LoadK { d: 9, k: 0 },
            Send { d: 1, r: 8, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![path_v])
        .site_named(&mut vm, "snapshot:", 1)
        .build(&mut vm);
    let r = vm.call(main, vm.nil(), &[]).unwrap();
    assert_eq!(r, vm.false_v());

    let vm2 = Vm::load_image(path.to_str().unwrap(), VmConfig::default()).unwrap();
    let outer2 = vm2.specials()[SPECIAL_SMALLTALK];
    assert!(outer2.is_ptr());
    let oa = outer2.as_ptr();
    assert_eq!(vm2.heap.num_slots(oa), 4);
    let inner2 = vm2.heap.slot(oa, 0);
    assert_eq!(vm2.heap.header(inner2.as_ptr()).class_index(), CLASS_ARRAY);
    assert_eq!(vm2.heap.slot(inner2.as_ptr(), 0), int(1));
    let s2 = vm2.heap.slot(inner2.as_ptr(), 1);
    assert_eq!(vm2.heap.bytes(s2.as_ptr()), "héllo wörld".as_bytes());
    let f2 = vm2.heap.slot(inner2.as_ptr(), 2);
    assert_eq!(vm2.float_value(f2), 2.5);
    let sym2 = vm2.heap.slot(oa, 1);
    assert_eq!(vm2.heap.header(sym2.as_ptr()).class_index(), CLASS_SYMBOL);
    assert_eq!(vm2.heap.bytes(sym2.as_ptr()), b"someSelector:");
    assert!(vm2.heap.is_immutable(sym2.as_ptr()), "symbol immutability survives");
    let big2 = vm2.heap.slot(oa, 2);
    assert_eq!(vm2.heap.num_slots(big2.as_ptr()), 300, "overflow word survives");
    assert_eq!(vm2.heap.slot(big2.as_ptr(), 299), int(7));
    assert_eq!(vm2.heap.slot(oa, 3), vm2.true_v());

    std::fs::remove_file(&path).ok();
}

#[test]
fn symbol_identity_survives_reload() {
    let path = temp_image_path("symids.im");
    let mut vm = Vm::bare_test();
    install_snapshot_prim(&mut vm);
    let sym = vm.intern("uniqueTestSelector");
    let holder = vm.make_array(&[sym]).unwrap();
    vm.set_special(SPECIAL_SMALLTALK, holder);

    let path_v = vm.make_string(path.to_str().unwrap()).unwrap();
    let main = MethodBuilder::new(0, 12)
        .insns(vec![
            LoadNil { d: 8 },
            LoadK { d: 9, k: 0 },
            Send { d: 1, r: 8, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![path_v])
        .site_named(&mut vm, "snapshot:", 1)
        .build(&mut vm);
    vm.call(main, vm.nil(), &[]).unwrap();

    let mut vm2 = Vm::load_image(path.to_str().unwrap(), VmConfig::default()).unwrap();
    let holder2 = vm2.specials()[SPECIAL_SMALLTALK];
    let saved_sym = vm2.heap.slot(holder2.as_ptr(), 0);
    // Interning the same characters must answer the identical object.
    let re_interned = vm2.intern("uniqueTestSelector");
    assert_eq!(saved_sym, re_interned, "symbol identity = pointer identity after load");

    std::fs::remove_file(&path).ok();
}
