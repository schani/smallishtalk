//! Phase 1 tests: tag round-trips (SPEC §1, §20).

use smallishtalk::value::Value;

#[test]
fn smallint_round_trip() {
    for n in [
        0i64,
        1,
        -1,
        42,
        -42,
        1 << 40,
        -(1 << 40),
        Value::SMALLINT_MAX,
        Value::SMALLINT_MIN,
    ] {
        let v = Value::from_int(n);
        assert!(v.is_int(), "{n} should tag as SmallInteger");
        assert!(!v.is_ptr());
        assert_eq!(v.as_int(), n, "round-trip of {n}");
    }
}

#[test]
fn smallint_range() {
    assert_eq!(Value::SMALLINT_MAX, (1 << 62) - 1);
    assert_eq!(Value::SMALLINT_MIN, -(1 << 62));
    assert!(Value::try_from_int(Value::SMALLINT_MAX).is_some());
    assert!(Value::try_from_int(Value::SMALLINT_MIN).is_some());
    assert!(Value::try_from_int(Value::SMALLINT_MAX + 1).is_none());
    assert!(Value::try_from_int(Value::SMALLINT_MIN - 1).is_none());
}

#[test]
fn tagging_formula() {
    // tag(n) = (n << 1) | 1; untag(w) = w >> 1 arithmetic.
    assert_eq!(Value::from_int(3).raw(), 7);
    assert_eq!(Value::from_int(0).raw(), 1);
    assert_eq!(Value::from_int(-1).raw(), u64::MAX); // (-1 << 1) | 1 as two's complement
}

#[test]
fn pointer_round_trip() {
    for addr in [8usize, 0x1000, 0xdead_beef_00, usize::MAX & !7] {
        let v = Value::from_ptr(addr);
        assert!(v.is_ptr(), "{addr:#x} should be a pointer value");
        assert!(!v.is_int());
        assert_eq!(v.as_ptr(), addr);
    }
}

#[test]
#[should_panic]
fn unaligned_pointer_rejected() {
    Value::from_ptr(0x1004); // not 8-byte aligned
}

#[test]
fn identity_is_raw_word_equality() {
    assert_eq!(Value::from_int(7), Value::from_int(7));
    assert_ne!(Value::from_int(7), Value::from_int(8));
    assert_eq!(Value::from_ptr(0x1000), Value::from_ptr(0x1000));
    assert_ne!(Value::from_ptr(0x1000), Value::from_int(0x800));
}

#[test]
fn value_is_copy_and_word_sized() {
    assert_eq!(std::mem::size_of::<Value>(), 8);
    let v = Value::from_int(5);
    let w = v; // Copy
    assert_eq!(v, w);
}
