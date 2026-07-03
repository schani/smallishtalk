//! Object headers, spaces, and allocation (SPEC §2, §3, §14).
//!
//! Memory is owned by `Space`s (stable raw allocations); object references
//! everywhere are plain addresses (`usize`) of the header word. No Rust
//! reference into heap memory is ever held across an allocation — all access
//! goes through `Heap` accessors taking addresses.

use crate::treaty::*;
use crate::value::Value;

/// One 64-bit object header word.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Header(u64);

fn mask(bits: u32) -> u64 {
    (1u64 << bits) - 1
}

impl Header {
    pub fn new(class_index: u32, format: u64, num_slots_field: u64) -> Header {
        debug_assert!((class_index as u64) <= mask(HDR_CLASS_BITS));
        debug_assert!(format <= mask(HDR_FORMAT_BITS));
        debug_assert!(num_slots_field <= mask(HDR_NSLOTS_BITS));
        Header(
            ((class_index as u64) << HDR_CLASS_SHIFT)
                | (num_slots_field << HDR_NSLOTS_SHIFT)
                | (format << HDR_FORMAT_SHIFT),
        )
    }

    #[inline(always)]
    pub const fn raw(self) -> u64 {
        self.0
    }

    #[inline(always)]
    pub const fn from_raw(w: u64) -> Header {
        Header(w)
    }

    #[inline(always)]
    pub fn class_index(self) -> u32 {
        ((self.0 >> HDR_CLASS_SHIFT) & mask(HDR_CLASS_BITS)) as u32
    }

    #[inline(always)]
    pub fn hash(self) -> u32 {
        ((self.0 >> HDR_HASH_SHIFT) & mask(HDR_HASH_BITS)) as u32
    }

    pub fn with_hash(self, hash: u32) -> Header {
        debug_assert!((hash as u64) <= mask(HDR_HASH_BITS));
        Header(
            (self.0 & !(mask(HDR_HASH_BITS) << HDR_HASH_SHIFT))
                | ((hash as u64) << HDR_HASH_SHIFT),
        )
    }

    /// The raw 8-bit field; 255 (`HDR_NSLOTS_OVERFLOW`) means the true count
    /// lives in the overflow word preceding the header.
    #[inline(always)]
    pub fn num_slots_field(self) -> u64 {
        (self.0 >> HDR_NSLOTS_SHIFT) & mask(HDR_NSLOTS_BITS)
    }

    #[inline(always)]
    pub fn format(self) -> u64 {
        (self.0 >> HDR_FORMAT_SHIFT) & mask(HDR_FORMAT_BITS)
    }

    #[inline(always)]
    pub fn gc_bits(self) -> u64 {
        (self.0 >> HDR_GC_SHIFT) & mask(HDR_GC_BITS)
    }

    pub fn with_gc_bits(self, bits: u64) -> Header {
        debug_assert!(bits <= mask(HDR_GC_BITS));
        Header((self.0 & !(mask(HDR_GC_BITS) << HDR_GC_SHIFT)) | (bits << HDR_GC_SHIFT))
    }

    #[inline(always)]
    pub fn is_bytes(self) -> bool {
        self.format() >= FMT_BYTES_BASE
    }
}

/// A contiguous, stable region of word-aligned memory with bump allocation.
pub struct Space {
    ptr: *mut u64,
    words: usize,
    /// Bump offset, in words.
    pub alloc: usize,
}

impl Space {
    pub fn new(bytes: usize) -> Space {
        let words = bytes / 8;
        let mut v: Vec<u64> = vec![0; words];
        let ptr = v.as_mut_ptr();
        std::mem::forget(v);
        Space { ptr, words, alloc: 0 }
    }

    #[inline(always)]
    pub fn base(&self) -> usize {
        self.ptr as usize
    }

    #[inline(always)]
    pub fn limit(&self) -> usize {
        self.base() + self.words * 8
    }

    #[inline(always)]
    pub fn top(&self) -> usize {
        self.base() + self.alloc * 8
    }

    #[inline(always)]
    pub fn contains(&self, addr: usize) -> bool {
        addr >= self.base() && addr < self.limit()
    }

    pub fn bytes_remaining(&self) -> usize {
        (self.words - self.alloc) * 8
    }

    pub fn used_bytes(&self) -> usize {
        self.alloc * 8
    }

    /// Bump-allocate `n` words, zeroed at construction time (reset re-zeroes).
    pub fn alloc_words(&mut self, n: usize) -> Option<usize> {
        if self.alloc + n > self.words {
            return None;
        }
        let addr = self.base() + self.alloc * 8;
        self.alloc += n;
        Some(addr)
    }

    /// Empty the space and zero its memory (semispace flip support).
    pub fn reset(&mut self) {
        unsafe { std::ptr::write_bytes(self.ptr, 0, self.words) };
        self.alloc = 0;
    }
}

impl Drop for Space {
    fn drop(&mut self) {
        unsafe { drop(Vec::from_raw_parts(self.ptr, self.words, self.words)) };
    }
}

// Space owns its memory exclusively; raw pointer is not shared.
unsafe impl Send for Space {}

#[derive(Clone)]
pub struct HeapConfig {
    pub young_bytes: usize,
    pub old_bytes: usize,
    pub large_object_bytes: usize,
}

impl Default for HeapConfig {
    fn default() -> HeapConfig {
        HeapConfig {
            young_bytes: YOUNG_SPACE_BYTES_DEFAULT,
            old_bytes: 64 * 1024 * 1024,
            large_object_bytes: LARGE_OBJECT_BYTES,
        }
    }
}

pub struct Heap {
    pub young_from: Space,
    pub young_to: Space,
    pub old: Space,
    /// Sequential-store buffer: addresses of old objects with the remembered
    /// bit set (they may contain young pointers). Drained by scavenge.
    pub ssb: Vec<usize>,
    pub config: HeapConfig,
    // Mutator allocation counters (profiling plan §3, always-on tier).
    // GC copies bypass these: they go through Space::alloc_words directly.
    pub alloc_young_count: u64,
    pub alloc_young_bytes: u64,
    pub alloc_old_count: u64,
    pub alloc_old_bytes: u64,
    pub alloc_large_count: u64,
}

impl Heap {
    pub fn new(config: HeapConfig) -> Heap {
        Heap {
            young_from: Space::new(config.young_bytes),
            young_to: Space::new(config.young_bytes),
            old: Space::new(config.old_bytes),
            ssb: Vec::new(),
            config: config.clone(),
            alloc_young_count: 0,
            alloc_young_bytes: 0,
            alloc_old_count: 0,
            alloc_old_bytes: 0,
            alloc_large_count: 0,
        }
    }

    // --- Raw word access ---

    #[inline(always)]
    pub fn word(&self, addr: usize) -> u64 {
        debug_assert!(self.plausible(addr));
        unsafe { *(addr as *const u64) }
    }

    #[inline(always)]
    pub fn set_word(&mut self, addr: usize, w: u64) {
        debug_assert!(self.plausible(addr));
        unsafe { *(addr as *mut u64) = w };
    }

    fn plausible(&self, addr: usize) -> bool {
        addr % 8 == 0
            && (self.young_from.contains(addr)
                || self.young_to.contains(addr)
                || self.old.contains(addr))
    }

    // --- Headers and object shape ---

    #[inline(always)]
    pub fn header(&self, obj: usize) -> Header {
        Header::from_raw(self.word(obj))
    }

    #[inline(always)]
    pub fn set_header(&mut self, obj: usize, h: Header) {
        self.set_word(obj, h.raw());
    }

    /// True slot count, reading the overflow word when the field says 255.
    #[inline(always)]
    pub fn num_slots(&self, obj: usize) -> u64 {
        let f = self.header(obj).num_slots_field();
        if f == HDR_NSLOTS_OVERFLOW {
            self.word(obj - 8)
        } else {
            f
        }
    }

    /// Whether this object's header is preceded by an overflow size word.
    #[inline(always)]
    pub fn has_overflow_word(&self, obj: usize) -> bool {
        self.header(obj).num_slots_field() == HDR_NSLOTS_OVERFLOW
    }

    /// Body byte size for byte-format objects: numSlots*8 - pad.
    pub fn byte_size(&self, obj: usize) -> usize {
        let h = self.header(obj);
        debug_assert!(h.is_bytes());
        let pad = (h.format() - FMT_BYTES_BASE) as usize;
        (self.num_slots(obj) as usize) * 8 - pad
    }

    /// Total footprint in bytes including header and overflow word.
    pub fn footprint(&self, obj: usize) -> usize {
        let over = if self.has_overflow_word(obj) { 8 } else { 0 };
        8 + (self.num_slots(obj) as usize) * 8 + over
    }

    /// Start address of the object's storage (overflow word if present,
    /// else the header).
    pub fn storage_start(&self, obj: usize) -> usize {
        if self.has_overflow_word(obj) { obj - 8 } else { obj }
    }

    /// Walk the objects allocated in `[base, top)` in address order,
    /// calling `f` with each object's header address.
    ///
    /// Boundary rule: a real header always has a nonzero classIndex (class
    /// table entry 0 is invalid), and overflow size words never reach bit
    /// 42 — so a storage word with zero class bits is an overflow word and
    /// the header follows it.
    pub fn walk_region(&self, base: usize, top: usize, mut f: impl FnMut(usize)) {
        let mut p = base;
        while p < top {
            let w = self.word(p);
            let obj = if (w >> HDR_CLASS_SHIFT) == 0 { p + 8 } else { p };
            f(obj);
            p = self.storage_start(obj) + self.footprint(obj);
        }
    }

    // --- Slot / byte access (0-indexed at this level) ---

    #[inline(always)]
    pub fn slot(&self, obj: usize, i: usize) -> Value {
        debug_assert!(
            (i as u64) < self.num_slots(obj),
            "slot index out of range: obj {obj:#x} class {} nslots {} index {i}",
            self.header(obj).class_index(),
            self.num_slots(obj)
        );
        Value::from_raw(self.word(obj + 8 + i * 8))
    }

    /// Raw slot store, no write barrier. GC-visible stores must go through
    /// the barrier in the VM layer.
    #[inline(always)]
    pub fn set_slot_raw(&mut self, obj: usize, i: usize, v: Value) {
        debug_assert!((i as u64) < self.num_slots(obj), "slot index out of range");
        self.set_word(obj + 8 + i * 8, v.raw());
    }

    #[inline(always)]
    pub fn byte(&self, obj: usize, i: usize) -> u8 {
        debug_assert!(i < self.byte_size(obj));
        unsafe { *((obj + 8 + i) as *const u8) }
    }

    #[inline(always)]
    pub fn set_byte(&mut self, obj: usize, i: usize, b: u8) {
        debug_assert!(i < self.byte_size(obj));
        unsafe { *((obj + 8 + i) as *mut u8) = b };
    }

    /// Fetch the i-th 32-bit instruction from a bytecodes ByteArray.
    #[inline(always)]
    pub fn insn_at(&self, obj: usize, i: usize) -> u32 {
        debug_assert!(i * 4 + 4 <= self.byte_size(obj), "pc out of method bounds");
        unsafe { *((obj + 8 + i * 4) as *const u32) }
    }

    pub fn bytes(&self, obj: usize) -> &[u8] {
        let len = self.byte_size(obj);
        unsafe { std::slice::from_raw_parts((obj + 8) as *const u8, len) }
    }

    pub fn write_bytes(&mut self, obj: usize, data: &[u8]) {
        debug_assert!(data.len() <= self.byte_size(obj));
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), (obj + 8) as *mut u8, data.len())
        };
    }

    // --- Immutability (Treaty: gcBits bit 7) ---

    pub fn is_immutable(&self, obj: usize) -> bool {
        self.header(obj).gc_bits() & GC_BIT_IMMUTABLE != 0
    }

    pub fn set_immutable(&mut self, obj: usize) {
        let h = self.header(obj);
        self.set_header(obj, h.with_gc_bits(h.gc_bits() | GC_BIT_IMMUTABLE));
    }

    // --- Space predicates ---

    #[inline(always)]
    pub fn is_young(&self, addr: usize) -> bool {
        self.young_from.contains(addr)
    }

    #[inline(always)]
    pub fn in_old_space(&self, addr: usize) -> bool {
        self.old.contains(addr)
    }

    // --- Allocation ---

    /// Allocate header (+ overflow word) + body in the given space.
    fn alloc_in(space: &mut Space, class_index: u32, format: u64, nslots: usize) -> Option<usize> {
        let (field, extra) = if nslots >= HDR_NSLOTS_OVERFLOW as usize {
            (HDR_NSLOTS_OVERFLOW, 1)
        } else {
            (nslots as u64, 0)
        };
        let start = space.alloc_words(extra + 1 + nslots)?;
        let obj = start + extra * 8;
        unsafe {
            if extra == 1 {
                *(start as *mut u64) = nslots as u64;
            }
            *(obj as *mut u64) = Header::new(class_index, format, field).raw();
        }
        Some(obj)
    }

    fn fill_slots(&mut self, obj: usize, nslots: usize, fill: Value) {
        for i in 0..nslots {
            self.set_word(obj + 8 + i * 8, fill.raw());
        }
    }

    /// Footprint in bytes of an allocation of `nslots` body slots
    /// (header + overflow word when needed).
    fn alloc_footprint(nslots: usize) -> u64 {
        let extra = (nslots >= HDR_NSLOTS_OVERFLOW as usize) as usize;
        ((extra + 1 + nslots) * 8) as u64
    }

    #[inline(always)]
    fn count_young(&mut self, nslots: usize) {
        self.alloc_young_count += 1;
        self.alloc_young_bytes += Self::alloc_footprint(nslots);
    }

    #[inline(always)]
    fn count_old(&mut self, nslots: usize, large: bool) {
        self.alloc_old_count += 1;
        self.alloc_old_bytes += Self::alloc_footprint(nslots);
        self.alloc_large_count += large as u64;
    }

    /// True when an allocation of `nslots` body slots goes straight to
    /// old space: over the large-object threshold, or too big to ever fit
    /// the (possibly stress-shrunken) young space.
    fn is_large(&self, nslots: usize) -> bool {
        nslots * 8 > self.config.large_object_bytes
            || (nslots + 2) * 8 > self.config.young_bytes
    }

    /// Allocate in young space, or old space directly for large objects.
    /// Returns None when young space is exhausted — the GC trigger.
    fn alloc_body(
        &mut self,
        class_index: u32,
        format: u64,
        nslots: usize,
    ) -> Option<usize> {
        if self.is_large(nslots) {
            let obj = Self::alloc_in(&mut self.old, class_index, format, nslots)?;
            self.count_old(nslots, true);
            Some(obj)
        } else {
            let obj = Self::alloc_in(&mut self.young_from, class_index, format, nslots)?;
            self.count_young(nslots);
            Some(obj)
        }
    }

    pub fn alloc_fixed(&mut self, class_index: u32, nslots: usize, fill: Value) -> Option<usize> {
        let obj = self.alloc_body(class_index, FMT_FIXED, nslots)?;
        self.fill_slots(obj, nslots, fill);
        Some(obj)
    }

    pub fn alloc_ptrs(&mut self, class_index: u32, nslots: usize, fill: Value) -> Option<usize> {
        let obj = self.alloc_body(class_index, FMT_PTRS, nslots)?;
        self.fill_slots(obj, nslots, fill);
        Some(obj)
    }

    /// Byte-format object, zero-filled (spaces are pre-zeroed).
    pub fn alloc_bytes(&mut self, class_index: u32, byte_len: usize) -> Option<usize> {
        let nslots = byte_len.div_ceil(8);
        let pad = nslots * 8 - byte_len;
        if self.is_large(nslots) {
            let obj =
                Self::alloc_in(&mut self.old, class_index, FMT_BYTES_BASE + pad as u64, nslots)?;
            self.count_old(nslots, true);
            Some(obj)
        } else {
            let obj = Self::alloc_in(
                &mut self.young_from,
                class_index,
                FMT_BYTES_BASE + pad as u64,
                nslots,
            )?;
            self.count_young(nslots);
            Some(obj)
        }
    }

    /// Force allocation into old space (image loading, stack growth's
    /// must-not-fail path, tenuring).
    pub fn alloc_fixed_old(&mut self, class_index: u32, nslots: usize, fill: Value) -> Option<usize> {
        let obj = Self::alloc_in(&mut self.old, class_index, FMT_FIXED, nslots)?;
        self.count_old(nslots, false);
        self.fill_slots(obj, nslots, fill);
        Some(obj)
    }

    pub fn alloc_ptrs_old(&mut self, class_index: u32, nslots: usize, fill: Value) -> Option<usize> {
        let obj = Self::alloc_in(&mut self.old, class_index, FMT_PTRS, nslots)?;
        self.count_old(nslots, false);
        self.fill_slots(obj, nslots, fill);
        Some(obj)
    }

    pub fn alloc_bytes_old(&mut self, class_index: u32, byte_len: usize) -> Option<usize> {
        let nslots = byte_len.div_ceil(8);
        let pad = nslots * 8 - byte_len;
        let obj = Self::alloc_in(&mut self.old, class_index, FMT_BYTES_BASE + pad as u64, nslots)?;
        self.count_old(nslots, false);
        Some(obj)
    }
}
