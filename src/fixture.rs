//! Heap-builder fixture (SPEC §20 Phase 1): hand-construct CompiledMethods
//! and CompiledBlocks for interpreter tests. Long-lived structure goes to
//! old space so Rust-held Values survive scavenges.

use crate::asm::Insn;
use crate::treaty::*;
use crate::value::Value;
use crate::vm::Vm;

struct SiteSpec {
    selector: Value,
    argc: u8,
    static_class: Value, // nil unless a SENDSUPER site
}

pub struct MethodBuilder {
    argc: u8,
    frame_slots: u8,
    insns: Vec<Insn>,
    literals: Vec<Value>,
    sites: Vec<SiteSpec>,
    primitive: Option<u16>,
    handler_slot_base: u8,
    mh_flags: u64,
}

impl MethodBuilder {
    /// `frame_slots` counts bytecode-visible slots: receiver (slot 0),
    /// args, temps, scratch.
    pub fn new(argc: u8, frame_slots: u8) -> MethodBuilder {
        assert!(frame_slots as usize >= 1 + argc as usize);
        MethodBuilder {
            argc,
            frame_slots,
            insns: Vec::new(),
            literals: Vec::new(),
            sites: Vec::new(),
            primitive: None,
            handler_slot_base: 0,
            mh_flags: 0,
        }
    }

    pub fn insns(mut self, insns: Vec<Insn>) -> Self {
        self.insns = insns;
        self
    }

    pub fn literals(mut self, lits: Vec<Value>) -> Self {
        self.literals = lits;
        self
    }

    pub fn lit(mut self, v: Value) -> Self {
        self.literals.push(v);
        self
    }

    pub fn site(mut self, selector: Value, argc: u8) -> Self {
        self.sites.push(SiteSpec {
            selector,
            argc,
            static_class: Value::from_raw(0),
        });
        self
    }

    pub fn site_named(self, vm: &mut Vm, name: &str, argc: u8) -> Self {
        let sel = vm.intern(name);
        self.site(sel, argc)
    }

    pub fn super_site(mut self, selector: Value, argc: u8, static_class: Value) -> Self {
        self.sites.push(SiteSpec {
            selector,
            argc,
            static_class,
        });
        self
    }

    pub fn super_site_named(self, vm: &mut Vm, name: &str, argc: u8, static_class: Value) -> Self {
        let sel = vm.intern(name);
        self.super_site(sel, argc, static_class)
    }

    /// Sets the primitive number in the header and prepends the PRIM
    /// instruction (§6: only as first instruction of a body).
    pub fn primitive(mut self, n: u16) -> Self {
        self.primitive = Some(n);
        self
    }

    pub fn handler_slot_base(mut self, b: u8) -> Self {
        self.handler_slot_base = b;
        self
    }

    pub fn mh_flags(mut self, flags: u64) -> Self {
        self.mh_flags = flags;
        self
    }

    fn build_parts(&mut self, vm: &mut Vm) -> (usize, usize, usize) {
        let nil = vm.nil();
        if let Some(n) = self.primitive {
            self.insns.insert(0, Insn::Prim { n });
        }
        assert!(self.insns.len() <= MAX_METHOD_INSTRUCTIONS);
        assert!(self.literals.len() <= MAX_LITERALS);
        assert!(self.sites.len() <= MAX_SEND_SITES);

        let code = vm
            .heap
            .alloc_bytes_old(CLASS_BYTEARRAY, self.insns.len() * 4)
            .expect("old space");
        for (i, insn) in self.insns.iter().enumerate() {
            let w = insn.encode().to_le_bytes();
            for (j, b) in w.iter().enumerate() {
                vm.heap.set_byte(code, i * 4 + j, *b);
            }
        }
        vm.heap.set_immutable(code);

        let lits = vm
            .heap
            .alloc_ptrs_old(CLASS_ARRAY, self.literals.len(), nil)
            .expect("old space");
        for (i, v) in self.literals.iter().enumerate() {
            // Barrier: literals may be young (test objects).
            vm.store_slot(lits, i, *v);
        }
        vm.heap.set_immutable(lits);

        let sites = vm
            .heap
            .alloc_ptrs_old(CLASS_ARRAY, self.sites.len() * SITE_STRIDE, nil)
            .expect("old space");
        for (i, s) in self.sites.iter().enumerate() {
            let base = i * SITE_STRIDE;
            vm.heap.set_slot_raw(sites, base + SITE_SELECTOR, s.selector);
            vm.heap
                .set_slot_raw(sites, base + SITE_ARGC, Value::from_int(s.argc as i64));
            vm.heap
                .set_slot_raw(sites, base + SITE_CACHE_CLASS, Value::from_int(0));
            vm.heap.set_slot_raw(sites, base + SITE_CACHE_METHOD, nil);
            vm.heap
                .set_slot_raw(sites, base + SITE_COUNTERS, Value::from_int(0));
            let sc = if s.static_class == Value::from_raw(0) {
                nil
            } else {
                s.static_class
            };
            vm.heap.set_slot_raw(sites, base + SITE_STATIC_CLASS, sc);
        }
        vm.site_registry.push(Value::from_ptr(sites));
        (code, lits, sites)
    }

    pub fn build(mut self, vm: &mut Vm) -> Value {
        let nil = vm.nil();
        let (code, lits, sites) = self.build_parts(vm);
        let header = Vm::pack_method_header(
            self.frame_slots as usize,
            self.argc as usize,
            self.primitive,
            self.handler_slot_base as usize,
            self.mh_flags,
        );
        let m = vm
            .heap
            .alloc_fixed_old(CLASS_COMPILEDMETHOD, METHOD_NUM_SLOTS, nil)
            .expect("old space");
        vm.heap.set_slot_raw(m, METHOD_HEADER, Value::from_int(header));
        vm.heap.set_slot_raw(m, METHOD_BYTECODES, Value::from_ptr(code));
        vm.heap.set_slot_raw(m, METHOD_LITERALS, Value::from_ptr(lits));
        vm.heap.set_slot_raw(m, METHOD_SEND_SITES, Value::from_ptr(sites));
        vm.heap.set_slot_raw(m, METHOD_VMSTATE, Value::from_int(0));
        Value::from_ptr(m)
    }

    /// Build as a CompiledBlock instead: same slots 0-3, plus outer method
    /// and blockInfo (numCaptured, hasNLR).
    pub fn build_block(mut self, vm: &mut Vm, outer: Value, num_captured: u8, has_nlr: bool) -> Value {
        let nil = vm.nil();
        let (code, lits, sites) = self.build_parts(vm);
        let header = Vm::pack_method_header(
            self.frame_slots as usize,
            self.argc as usize,
            self.primitive,
            self.handler_slot_base as usize,
            self.mh_flags,
        );
        let info = (num_captured as i64) | ((has_nlr as i64) << BI_HAS_NLR_SHIFT);
        let b = vm
            .heap
            .alloc_fixed_old(CLASS_COMPILEDBLOCK, BLOCK_NUM_SLOTS, nil)
            .expect("old space");
        vm.heap.set_slot_raw(b, BLOCK_HEADER, Value::from_int(header));
        vm.heap.set_slot_raw(b, BLOCK_BYTECODES, Value::from_ptr(code));
        vm.heap.set_slot_raw(b, BLOCK_LITERALS, Value::from_ptr(lits));
        vm.heap.set_slot_raw(b, BLOCK_SEND_SITES, Value::from_ptr(sites));
        vm.heap.set_slot_raw(b, BLOCK_OUTER_METHOD, outer);
        vm.heap.set_slot_raw(b, BLOCK_INFO, Value::from_int(info));
        vm.heap.set_slot_raw(b, BLOCK_PAD, Value::from_int(0));
        vm.heap.set_slot_raw(b, BLOCK_VMSTATE, Value::from_int(0));
        Value::from_ptr(b)
    }
}
