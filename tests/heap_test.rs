//! Phase 1 tests: header pack/unpack, allocation into a test heap,
//! per-format accessor semantics (SPEC §2, §3, §20).

use smallishtalk::heap::{Header, Heap, HeapConfig};
use smallishtalk::treaty::*;
use smallishtalk::value::Value;

fn test_heap() -> Heap {
    Heap::new(HeapConfig {
        young_bytes: 256 * 1024,
        old_bytes: 1024 * 1024,
        ..HeapConfig::default()
    })
}

#[test]
fn header_pack_unpack_round_trip() {
    let h = Header::new(17, FMT_FIXED, 3);
    assert_eq!(h.class_index(), 17);
    assert_eq!(h.format(), FMT_FIXED);
    assert_eq!(h.num_slots_field(), 3);
    assert_eq!(h.hash(), 0);
    assert_eq!(h.gc_bits(), 0);
    assert_eq!(h.raw(), 0x0000_4400_0000_3000); // the Phase-0 golden header

    let h2 = h.with_hash(0x3FFFFF);
    assert_eq!(h2.hash(), 0x3FFFFF);
    assert_eq!(h2.class_index(), 17);
    assert_eq!(h2.num_slots_field(), 3);

    let h3 = Header::new(0x3FFFFF, FMT_BYTES_BASE + 7, 254);
    assert_eq!(h3.class_index(), 0x3FFFFF);
    assert_eq!(h3.format(), 15);
    assert_eq!(h3.num_slots_field(), 254);
}

#[test]
fn alloc_fixed_object() {
    let mut heap = test_heap();
    let nil = Value::from_int(0); // stand-in fill for these unit tests
    let addr = heap.alloc_fixed(17, 3, nil).unwrap();
    assert_eq!(addr & 7, 0, "8-byte aligned");

    let h = heap.header(addr);
    assert_eq!(h.class_index(), 17);
    assert_eq!(h.format(), FMT_FIXED);
    assert_eq!(h.num_slots_field(), 3);
    assert_eq!(heap.num_slots(addr), 3);
    for i in 0..3 {
        assert_eq!(heap.slot(addr, i), nil);
    }
    heap.set_slot_raw(addr, 1, Value::from_int(99));
    assert_eq!(heap.slot(addr, 1).as_int(), 99);
}

#[test]
fn alloc_pointer_indexable() {
    let mut heap = test_heap();
    let fill = Value::from_int(0);
    let addr = heap.alloc_ptrs(CLASS_ARRAY, 10, fill).unwrap();
    assert_eq!(heap.header(addr).format(), FMT_PTRS);
    assert_eq!(heap.num_slots(addr), 10);
}

#[test]
fn alloc_bytes_padding() {
    let mut heap = test_heap();
    // byteSize = numSlots*8 - pad, pad = format - 8
    for len in 0..=17usize {
        let addr = heap.alloc_bytes(CLASS_BYTEARRAY, len).unwrap();
        let h = heap.header(addr);
        let expect_slots = len.div_ceil(8);
        assert_eq!(heap.num_slots(addr), expect_slots as u64, "len {len}");
        let pad = h.format() - FMT_BYTES_BASE;
        assert_eq!(expect_slots * 8 - len, pad as usize, "len {len}");
        assert_eq!(heap.byte_size(addr), len, "len {len}");
        for i in 0..len {
            assert_eq!(heap.byte(addr, i), 0, "zero-initialized");
        }
    }
}

#[test]
fn byte_object_read_write() {
    let mut heap = test_heap();
    let addr = heap.alloc_bytes(CLASS_BYTESTRING, 5).unwrap();
    for (i, b) in b"hello".iter().enumerate() {
        heap.set_byte(addr, i, *b);
    }
    assert_eq!(heap.bytes(addr), b"hello");
}

#[test]
fn overflow_slot_count() {
    let mut heap = test_heap();
    let fill = Value::from_int(7);
    // 300 slots >= 255 forces the overflow word before the header.
    let addr = heap.alloc_ptrs(CLASS_ARRAY, 300, fill).unwrap();
    let h = heap.header(addr);
    assert_eq!(h.num_slots_field(), HDR_NSLOTS_OVERFLOW);
    assert_eq!(heap.num_slots(addr), 300);
    assert_eq!(heap.slot(addr, 299), fill);
    heap.set_slot_raw(addr, 299, Value::from_int(-5));
    assert_eq!(heap.slot(addr, 299).as_int(), -5);

    // The very next allocation must not overlap the 300-slot body.
    let next = heap.alloc_fixed(1, 1, fill).unwrap();
    assert!(next >= addr + (1 + 300) * 8);
}

#[test]
fn identity_hash_assignment() {
    let mut heap = test_heap();
    let addr = heap.alloc_fixed(17, 1, Value::from_int(0)).unwrap();
    assert_eq!(heap.header(addr).hash(), 0, "born unhashed");
    heap.set_header(addr, heap.header(addr).with_hash(1234));
    assert_eq!(heap.header(addr).hash(), 1234);
    assert_eq!(heap.header(addr).class_index(), 17, "class survives");
}

#[test]
fn immutable_bit() {
    let mut heap = test_heap();
    let addr = heap.alloc_bytes(CLASS_BYTESTRING, 3).unwrap();
    assert!(!heap.is_immutable(addr));
    heap.set_immutable(addr);
    assert!(heap.is_immutable(addr));
    assert_eq!(heap.header(addr).gc_bits() & GC_BIT_IMMUTABLE, GC_BIT_IMMUTABLE);
}

#[test]
fn young_space_bounds() {
    let mut heap = test_heap();
    let a = heap.alloc_fixed(1, 2, Value::from_int(0)).unwrap();
    assert!(heap.is_young(a));
    let old = heap.alloc_fixed_old(1, 2, Value::from_int(0)).unwrap();
    assert!(!heap.is_young(old));
    assert!(heap.in_old_space(old));
}

#[test]
fn large_objects_go_to_old_space() {
    let mut heap = test_heap();
    // > 64KB body allocates directly in old space (SPEC §14).
    let big = heap
        .alloc_bytes(CLASS_BYTEARRAY, LARGE_OBJECT_BYTES + 8)
        .unwrap();
    assert!(heap.in_old_space(big));
    let small = heap.alloc_bytes(CLASS_BYTEARRAY, 16).unwrap();
    assert!(heap.is_young(small));
}

#[test]
fn young_exhaustion_returns_none() {
    let mut heap = Heap::new(HeapConfig {
        young_bytes: 4096,
        old_bytes: 64 * 1024,
        ..HeapConfig::default()
    });
    let fill = Value::from_int(0);
    let mut last = None;
    for _ in 0..1000 {
        match heap.alloc_fixed(1, 32, fill) {
            Some(a) => last = Some(a),
            None => break,
        }
    }
    assert!(last.is_some());
    assert!(
        heap.alloc_fixed(1, 32, fill).is_none(),
        "young space exhausted must return None (GC trigger)"
    );
}

#[test]
fn footprint_is_slots_plus_header() {
    let mut heap = test_heap();
    let fill = Value::from_int(0);
    let a = heap.alloc_fixed(1, 3, fill).unwrap();
    let b = heap.alloc_fixed(1, 0, fill).unwrap();
    let c = heap.alloc_fixed(1, 1, fill).unwrap();
    // 8*(numSlots) + 8 bytes each, contiguous bump allocation.
    assert_eq!(b - a, 8 * 3 + 8);
    assert_eq!(c - b, 8 * 0 + 8);
}
