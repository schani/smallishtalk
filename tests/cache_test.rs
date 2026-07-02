//! Phase 1 tests for the send caches (SPEC §8): inline-cache fill and hit,
//! eager invalidation on method install, megamorphic sites, and the sealed
//! SmallInteger selectors (§12).

use smallishtalk::asm::Insn::*;
use smallishtalk::fixture::MethodBuilder;
use smallishtalk::treaty::*;
use smallishtalk::value::Value;
use smallishtalk::vm::Vm;

fn int(n: i64) -> Value {
    Value::from_int(n)
}

/// A method whose single send site we can inspect afterwards.
fn caller_with_site(vm: &mut Vm, recv: Value, sel: &str) -> Value {
    MethodBuilder::new(0, 6)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![recv])
        .site_named(vm, sel, 0)
        .build(vm)
}

fn site_cache(vm: &Vm, method: Value) -> (i64, Value) {
    let sites = vm.heap.slot(method.as_ptr(), METHOD_SEND_SITES);
    (
        vm.heap.slot(sites.as_ptr(), SITE_CACHE_CLASS).as_int(),
        vm.heap.slot(sites.as_ptr(), SITE_CACHE_METHOD),
    )
}

#[test]
fn inline_cache_fills_on_first_send() {
    let mut vm = Vm::bare_test();
    let class = vm.new_test_class(FMT_FIXED, 0);
    let sel = vm.intern("answer");
    let answer = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 42 }, Ret { a: 1 }])
        .build(&mut vm);
    vm.install_method(class, sel, answer);
    let obj = vm.make_instance(class).unwrap();
    let caller = caller_with_site(&mut vm, obj, "answer");

    assert_eq!(site_cache(&vm, caller), (0, vm.nil()), "cache starts empty");
    assert_eq!(vm.call(caller, vm.nil(), &[]).unwrap(), int(42));
    let class_idx = vm
        .heap
        .slot(class.as_ptr(), BEHAVIOR_CLASS_INDEX)
        .as_int();
    let (cc, cm) = site_cache(&vm, caller);
    assert_eq!(cc, class_idx, "cacheClass filled with receiver's class index");
    assert_eq!(cm, answer, "cacheMethod filled");
    // Second call hits the cache and still answers correctly.
    assert_eq!(vm.call(caller, vm.nil(), &[]).unwrap(), int(42));
}

#[test]
fn method_install_clears_all_caches_eagerly() {
    let mut vm = Vm::bare_test();
    let class = vm.new_test_class(FMT_FIXED, 0);
    let sel = vm.intern("answer");
    let m1 = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 1 }, Ret { a: 1 }])
        .build(&mut vm);
    vm.install_method(class, sel, m1);
    let obj = vm.make_instance(class).unwrap();
    let caller = caller_with_site(&mut vm, obj, "answer");

    assert_eq!(vm.call(caller, vm.nil(), &[]).unwrap(), int(1));
    assert_ne!(site_cache(&vm, caller).0, 0, "cache is filled");

    // Install a replacement: every inline cache must be cleared eagerly.
    let m2 = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 2 }, Ret { a: 1 }])
        .build(&mut vm);
    vm.install_method(class, sel, m2);
    assert_eq!(site_cache(&vm, caller), (0, vm.nil()), "cache cleared by install");
    assert_eq!(vm.call(caller, vm.nil(), &[]).unwrap(), int(2));
}

#[test]
fn register_class_clears_caches() {
    let mut vm = Vm::bare_test();
    let class = vm.new_test_class(FMT_FIXED, 0);
    let sel = vm.intern("answer");
    let m1 = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 1 }, Ret { a: 1 }])
        .build(&mut vm);
    vm.install_method(class, sel, m1);
    let obj = vm.make_instance(class).unwrap();
    let caller = caller_with_site(&mut vm, obj, "answer");
    vm.call(caller, vm.nil(), &[]).unwrap();
    assert_ne!(site_cache(&vm, caller).0, 0);

    vm.new_test_class(FMT_FIXED, 0); // registerClass: flushes
    assert_eq!(site_cache(&vm, caller), (0, vm.nil()));
}

#[test]
fn megamorphic_site_stays_correct() {
    let mut vm = Vm::bare_test();
    let sel = vm.intern("tag");
    // Ten classes, each answering its own tag through one shared site.
    let mut objs = Vec::new();
    for i in 0..10 {
        let c = vm.new_test_class(FMT_FIXED, 0);
        let m = MethodBuilder::new(0, 2)
            .insns(vec![LoadInt { d: 1, imm: i }, Ret { a: 1 }])
            .build(&mut vm);
        vm.install_method(c, sel, m);
        objs.push(vm.make_instance(c).unwrap());
    }
    // One method, one send site, receiver passed as the argument.
    let poly = MethodBuilder::new(1, 6)
        .insns(vec![
            Move { d: 4, a: 1 },
            Send { d: 2, r: 4, site: 0 },
            Ret { a: 2 },
        ])
        .site_named(&mut vm, "tag", 0)
        .build(&mut vm);
    for round in 0..3 {
        for (i, obj) in objs.iter().enumerate() {
            assert_eq!(
                vm.call(poly, vm.nil(), &[*obj]).unwrap(),
                int(i as i64),
                "round {round}"
            );
        }
    }
}

#[test]
fn sealed_smallinteger_selectors_ignore_prim_install() {
    let mut vm = Vm::bare_test();
    let obj_cls = vm.class_table_at(CLASS_OBJECT);
    // methodInstall primitive.
    let installer = MethodBuilder::new(2, 5)
        .primitive(PRIM_METHOD_INSTALL)
        .insns(vec![Ret { a: 3 }])
        .build(&mut vm);
    let inst_sel = vm.intern("install:as:");
    vm.install_method(obj_cls, inst_sel, installer);

    // A legitimate install of SmallInteger>>+ (bootstrap-style, Rust side).
    let legit = MethodBuilder::new(1, 3)
        .primitive(PRIM_INT_ADD)
        .insns(vec![Ret { a: 2 }])
        .build(&mut vm);
    let plus = vm.intern("+");
    let si = vm.class_table_at(CLASS_SMALLINTEGER);
    vm.install_method(si, plus, legit);

    // An in-image attempt to override the sealed method must be ignored.
    let evil = MethodBuilder::new(1, 2)
        .insns(vec![LoadInt { d: 1, imm: -666 }, Ret { a: 1 }])
        .build(&mut vm);
    let main = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            LoadK { d: 5, k: 1 },
            LoadK { d: 6, k: 2 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![si, plus, evil])
        .site_named(&mut vm, "install:as:", 2)
        .build(&mut vm);
    vm.call(main, vm.nil(), &[]).unwrap();

    // + still resolves to the legitimate primitive method.
    assert_eq!(vm.lookup_method(CLASS_SMALLINTEGER, plus), Some(legit));
    // But installing a *non-sealed* selector on SmallInteger works.
    let ok_sel = vm.intern("frob");
    let frob = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 5 }, Ret { a: 1 }])
        .build(&mut vm);
    let main2 = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            LoadK { d: 5, k: 1 },
            LoadK { d: 6, k: 2 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![si, ok_sel, frob])
        .site_named(&mut vm, "install:as:", 2)
        .build(&mut vm);
    vm.call(main2, vm.nil(), &[]).unwrap();
    assert_eq!(vm.lookup_method(CLASS_SMALLINTEGER, ok_sel), Some(frob));
}

#[test]
fn super_send_site_caches_too() {
    let mut vm = Vm::bare_test();
    let sup = vm.new_test_class(FMT_FIXED, 0);
    let sub = vm.new_test_subclass(sup, FMT_FIXED, 0);
    let sel = vm.intern("answer");
    let sup_m = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 7 }, Ret { a: 1 }])
        .build(&mut vm);
    vm.install_method(sup, sel, sup_m);

    let via_super = MethodBuilder::new(0, 6)
        .insns(vec![
            LoadSelf { d: 4 },
            SendSuper { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .super_site_named(&mut vm, "answer", 0, sub)
        .build(&mut vm);
    let call_sel = vm.intern("callSuper");
    vm.install_method(sub, call_sel, via_super);

    let obj = vm.make_instance(sub).unwrap();
    let main = caller_with_site(&mut vm, obj, "callSuper");
    assert_eq!(vm.call(main, vm.nil(), &[]).unwrap(), int(7));
    // Repeat: hits the filled cache.
    assert_eq!(vm.call(main, vm.nil(), &[]).unwrap(), int(7));
    let (cc, cm) = site_cache(&vm, via_super);
    let sub_idx = vm.heap.slot(sub.as_ptr(), BEHAVIOR_CLASS_INDEX).as_int();
    assert_eq!(cc, sub_idx);
    assert_eq!(cm, sup_m);
}

/// §8: "every CompiledMethod registers its table at installation" — a
/// method installed through install_method (the loader/methodInstall path)
/// must have its send-site cache cleared by later installs.
#[test]
fn installed_methods_send_sites_are_registered_for_clearing() {
    let mut vm = Vm::bare_test();
    let class = vm.new_test_class(FMT_FIXED, 0);
    let target_sel = vm.intern("answer");
    let m1 = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 1 }, Ret { a: 1 }])
        .build(&mut vm);
    vm.install_method(class, target_sel, m1);

    // A calling method, installed on the class (not just fixture-built).
    let caller = MethodBuilder::new(0, 6)
        .insns(vec![
            LoadSelf { d: 4 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .site_named(&mut vm, "answer", 0)
        .build(&mut vm);
    let call_sel = vm.intern("callIt");
    vm.install_method(class, call_sel, caller);
    // Remove it from the fixture's registry to prove installation alone
    // suffices (the builder also registers as a convenience).
    let sites = vm.heap.slot(caller.as_ptr(), METHOD_SEND_SITES);
    vm.site_registry.retain(|s| *s != sites);
    vm.install_method(class, call_sel, caller); // re-install → must register

    let obj = vm.make_instance(class).unwrap();
    let main = caller_with_site(&mut vm, obj, "callIt");
    assert_eq!(vm.call(main, vm.nil(), &[]).unwrap(), int(1));
    // caller's inline cache for #answer is now filled.
    assert_ne!(site_cache(&vm, caller).0, 0);
    // Replacing #answer must clear caller's cache and take effect.
    let m2 = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 2 }, Ret { a: 1 }])
        .build(&mut vm);
    vm.install_method(class, target_sel, m2);
    assert_eq!(site_cache(&vm, caller), (0, vm.nil()), "registered at install");
    assert_eq!(vm.call(main, vm.nil(), &[]).unwrap(), int(2));
}
