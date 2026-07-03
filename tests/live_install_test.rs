//! PERMANENT regression gate for live method installation (UI.md §9.3 / §14.1,
//! pulled forward to M1). The Class Browser's headline feature — edit a method
//! and "accept" it — is worthless if warm call sites keep hitting the OLD
//! method. So this test pins the contract that installing a method through
//! PRIM_METHOD_INSTALL (the exact path `Behavior>>compile:classified:` uses)
//! invalidates BOTH the inline caches at warmed send sites AND the global
//! lookup cache, so subsequent sends observe the NEW behavior.
//!
//! This test must never be deleted. The browser milestones (M4/M5) are gated on
//! it being green, and later JIT work (JIT.md) can re-break it — that is exactly
//! what it is here to catch.

use smallishtalk::asm::Insn::*;
use smallishtalk::fixture::MethodBuilder;
use smallishtalk::treaty::*;
use smallishtalk::value::Value;
use smallishtalk::vm::Vm;

fn int(n: i64) -> Value {
    Value::from_int(n)
}

/// A method whose single `#answer` send site we can warm and inspect.
fn caller_with_site(vm: &mut Vm, recv: Value) -> Value {
    MethodBuilder::new(0, 6)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![recv])
        .site_named(vm, "answer", 0)
        .build(vm)
}

fn site_cache_class(vm: &Vm, method: Value) -> i64 {
    let sites = vm.heap.slot(method.as_ptr(), METHOD_SEND_SITES);
    vm.heap.slot(sites.as_ptr(), SITE_CACHE_CLASS).as_int()
}

/// Install `method` for `selector` on `class` through the real
/// PRIM_METHOD_INSTALL primitive, driven from compiled Smalltalk — i.e. the
/// browser's `compile:classified:` path, not the Rust loader helper.
fn install_via_primitive(vm: &mut Vm, class: Value, selector: Value, method: Value) {
    let obj_cls = vm.class_table_at(CLASS_OBJECT);
    let installer = MethodBuilder::new(2, 5)
        .primitive(PRIM_METHOD_INSTALL)
        .insns(vec![Ret { a: 3 }])
        .build(vm);
    let inst_sel = vm.intern("install:as:");
    vm.install_method(obj_cls, inst_sel, installer);

    let main = MethodBuilder::new(0, 8)
        .insns(vec![
            LoadK { d: 4, k: 0 },
            LoadK { d: 5, k: 1 },
            LoadK { d: 6, k: 2 },
            Send { d: 1, r: 4, site: 0 },
            Ret { a: 1 },
        ])
        .literals(vec![class, selector, method])
        .site_named(vm, "install:as:", 2)
        .build(vm);
    vm.call(main, vm.nil(), &[]).unwrap();
}

#[test]
fn warm_call_site_observes_live_replacement_via_prim_install() {
    let mut vm = Vm::bare_test();
    let class = vm.new_test_class(FMT_FIXED, 0);
    let class_idx = vm.heap.slot(class.as_ptr(), BEHAVIOR_CLASS_INDEX).as_int() as u32;
    let sel = vm.intern("answer");

    // Original method: #answer -> 1.
    let m1 = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 1 }, Ret { a: 1 }])
        .build(&mut vm);
    vm.install_method(class, sel, m1);

    let obj = vm.make_instance(class).unwrap();
    let caller = caller_with_site(&mut vm, obj);

    // WARM the call site hard: a tight loop of real sends, so both the inline
    // cache and the global lookup cache are hot on the old method.
    for _ in 0..1000 {
        assert_eq!(vm.call(caller, vm.nil(), &[]).unwrap(), int(1));
    }
    assert_ne!(site_cache_class(&vm, caller), 0, "site cache warmed on m1");
    assert_eq!(vm.lookup_method(class_idx, sel), Some(m1));

    // LIVE ACCEPT: install #answer -> 2 through PRIM_METHOD_INSTALL.
    let m2 = MethodBuilder::new(0, 2)
        .insns(vec![LoadInt { d: 1, imm: 2 }, Ret { a: 1 }])
        .build(&mut vm);
    install_via_primitive(&mut vm, class, sel, m2);

    // The install must have eagerly cleared the warmed inline cache...
    assert_eq!(
        site_cache_class(&vm, caller),
        0,
        "warmed inline cache cleared by live install"
    );
    // ...and the global lookup cache must resolve to the new method.
    assert_eq!(vm.lookup_method(class_idx, sel), Some(m2));

    // Every subsequent send — including re-warming the site — observes m2.
    for _ in 0..1000 {
        assert_eq!(
            vm.call(caller, vm.nil(), &[]).unwrap(),
            int(2),
            "warm site must never resurrect the old method"
        );
    }
}
