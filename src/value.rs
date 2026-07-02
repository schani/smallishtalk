//! Tagged 64-bit values (SPEC §1).
//!
//! `...1` SmallInteger (value = word >> 1 arithmetic); `...000` object
//! pointer. Tag `010` is reserved for immediate floats (v2) and never
//! produced. Value is Copy and exactly one word — it is never a Rust
//! reference into the heap.

use crate::treaty::{TAG_INT_BIT, TAG_PTR, TAG_PTR_MASK};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Value(u64);

impl Value {
    pub const SMALLINT_MAX: i64 = (1 << 62) - 1;
    pub const SMALLINT_MIN: i64 = -(1 << 62);

    #[inline(always)]
    pub const fn raw(self) -> u64 {
        self.0
    }

    #[inline(always)]
    pub const fn from_raw(w: u64) -> Value {
        Value(w)
    }

    /// tag(n) = (n << 1) | 1. Panics outside the 63-bit range.
    #[inline(always)]
    pub fn from_int(n: i64) -> Value {
        debug_assert!(
            (Value::SMALLINT_MIN..=Value::SMALLINT_MAX).contains(&n),
            "SmallInteger out of 63-bit range: {n}"
        );
        Value(((n as u64) << 1) | TAG_INT_BIT)
    }

    #[inline(always)]
    pub fn try_from_int(n: i64) -> Option<Value> {
        if (Value::SMALLINT_MIN..=Value::SMALLINT_MAX).contains(&n) {
            Some(Value(((n as u64) << 1) | TAG_INT_BIT))
        } else {
            None
        }
    }

    /// untag(w) = w >> 1, arithmetic shift.
    #[inline(always)]
    pub const fn as_int(self) -> i64 {
        (self.0 as i64) >> 1
    }

    #[inline(always)]
    pub const fn is_int(self) -> bool {
        self.0 & TAG_INT_BIT != 0
    }

    #[inline(always)]
    pub const fn is_ptr(self) -> bool {
        self.0 & TAG_PTR_MASK == TAG_PTR
    }

    /// An object pointer: the 8-byte-aligned address of an object header.
    #[inline(always)]
    pub fn from_ptr(addr: usize) -> Value {
        assert!(addr & TAG_PTR_MASK as usize == 0, "unaligned object address");
        Value(addr as u64)
    }

    #[inline(always)]
    pub const fn as_ptr(self) -> usize {
        debug_assert!(self.is_ptr());
        self.0 as usize
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_int() {
            write!(f, "int({})", self.as_int())
        } else {
            write!(f, "ptr({:#x})", self.0)
        }
    }
}
