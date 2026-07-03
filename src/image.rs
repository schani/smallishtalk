//! Image snapshot and loading (SPEC §17): STIM format — a header plus one
//! contiguous old-space dump. Headers contain class indices, not pointers,
//! so loading is a single linear pass over object slots adding the
//! relocation delta; the class table, send-site registry, and (bootstrap)
//! symbol table are rebuilt by the same walk.

use crate::heap::Heap;
use crate::treaty::*;
use crate::value::Value;
use crate::vm::{LookupEntry, Vm, VmConfig, VmError};

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn get_u32(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(data[off..off + 4].try_into().unwrap())
}

fn get_u64(data: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(data[off..off + 8].try_into().unwrap())
}

impl Vm {
    /// Write the image. Preconditions (arranged by the snapshot primitive):
    /// young space is empty and the active process's pc/frameOffset are
    /// saved — the image is old-space-only and the process serializes as
    /// suspended.
    pub fn write_image(&mut self, path: &str) -> Result<(), VmError> {
        debug_assert_eq!(self.heap.young_from.used_bytes(), 0, "young must be empty");
        let nil = self.nil();

        // Image-side class list (index-aligned with the VM class table).
        let n = self.class_table.len();
        let cl = self
            .heap
            .alloc_ptrs_old(CLASS_ARRAY, n, nil)
            .ok_or(VmError::OutOfMemory)?;
        for (i, c) in self.class_table.clone().iter().enumerate() {
            self.heap.set_slot_raw(cl, i, *c);
        }
        self.specials[SPECIAL_CLASS_LIST] = Value::from_ptr(cl);

        // The special objects array (A.4).
        let sp = self
            .heap
            .alloc_ptrs_old(CLASS_ARRAY, SPECIAL_OBJECTS_COUNT, nil)
            .ok_or(VmError::OutOfMemory)?;
        for (i, v) in self.specials.clone().iter().enumerate() {
            self.heap.set_slot_raw(sp, i, *v);
        }

        let base = self.heap.old.base();
        let size = self.heap.old.used_bytes();
        let active = self.active_process;
        if !active.is_ptr() || !self.heap.in_old_space(active.as_ptr()) {
            return Err(VmError::Fatal("active process must live in old space".into()));
        }

        let mut out = Vec::with_capacity(IMG_HEADER_SIZE + size);
        put_u32(&mut out, IMG_MAGIC);
        put_u32(&mut out, 1); // version
        put_u64(&mut out, 0); // flags
        put_u64(&mut out, base as u64);
        put_u64(&mut out, size as u64);
        put_u64(&mut out, (sp - base) as u64);
        put_u64(&mut out, (cl - base) as u64);
        put_u64(&mut out, (active.as_ptr() - base) as u64);
        put_u64(&mut out, 0); // reserved
        debug_assert_eq!(out.len(), IMG_HEADER_SIZE);
        out.extend_from_slice(unsafe {
            std::slice::from_raw_parts(base as *const u8, size)
        });
        std::fs::write(path, out)
            .map_err(|e| VmError::Fatal(format!("image write failed: {e}")))
    }

    /// Load an image: mmap-equivalent read, one relocation pass over object
    /// slots, rebuild the class table / send-site registry / symbol intern
    /// table, and locate the active process.
    pub fn load_image(path: &str, config: VmConfig) -> Result<Vm, VmError> {
        let data = std::fs::read(path)
            .map_err(|e| VmError::Fatal(format!("image read failed: {e}")))?;
        if data.len() < IMG_HEADER_SIZE || get_u32(&data, IMG_MAGIC_OFFSET) != IMG_MAGIC {
            return Err(VmError::Fatal("not a STIM image".into()));
        }
        if get_u32(&data, IMG_VERSION_OFFSET) != 1 {
            return Err(VmError::Fatal("unsupported image version".into()));
        }
        let saved_base = get_u64(&data, IMG_SAVED_BASE_OFFSET) as usize;
        let size = get_u64(&data, IMG_OLD_SPACE_SIZE_OFFSET) as usize;
        let sp_off = get_u64(&data, IMG_SPECIAL_OBJECTS_OFFSET) as usize;
        let cl_off = get_u64(&data, IMG_CLASS_LIST_OFFSET) as usize;
        let active_off = get_u64(&data, IMG_ACTIVE_PROCESS_OFFSET) as usize;
        if data.len() < IMG_HEADER_SIZE + size {
            return Err(VmError::Fatal("truncated image".into()));
        }

        let mut heap_cfg = config.heap.clone();
        heap_cfg.old_bytes = heap_cfg.old_bytes.max(size * 2);
        let mut heap = Heap::new(heap_cfg);
        let base = heap.old.base();
        unsafe {
            std::ptr::copy_nonoverlapping(
                data[IMG_HEADER_SIZE..].as_ptr(),
                base as *mut u8,
                size,
            );
        }
        heap.old.alloc = size / 8;
        let delta = base as i64 - saved_base as i64;

        // One linear pass: relocate pointer slots, clear GC bookkeeping
        // bits, and collect methods (send-site registry) and symbols.
        let mut objs: Vec<usize> = Vec::new();
        heap.walk_region(base, base + size, |o| objs.push(o));
        let mut site_registry: Vec<Value> = Vec::new();
        let mut symbols: Vec<(Vec<u8>, Value)> = Vec::new();
        for &obj in &objs {
            let h = heap.header(obj);
            let bits = h.gc_bits() & !(GC_BIT_MARK | GC_BIT_REMEMBERED);
            if bits != h.gc_bits() {
                heap.set_header(obj, h.with_gc_bits(bits));
            }
            if !h.is_bytes() {
                let n = heap.num_slots(obj) as usize;
                for i in 0..n {
                    let w = heap.slot(obj, i);
                    if w.is_ptr() {
                        let new = (w.raw() as i64 + delta) as u64;
                        heap.set_slot_raw(obj, i, Value::from_raw(new));
                    }
                }
            }
            match h.class_index() {
                CLASS_COMPILEDMETHOD | CLASS_COMPILEDBLOCK => {
                    let sites = heap.slot(obj, METHOD_SEND_SITES);
                    if sites.is_ptr() && heap.header(sites.as_ptr()).format() == FMT_PTRS {
                        site_registry.push(sites);
                    }
                }
                CLASS_SYMBOL => {
                    symbols.push((heap.bytes(obj).to_vec(), Value::from_ptr(obj)));
                }
                _ => {}
            }
        }

        // Specials and class table from their image-side arrays.
        let sp = base + sp_off;
        let mut specials = Vec::with_capacity(SPECIAL_OBJECTS_COUNT);
        for i in 0..SPECIAL_OBJECTS_COUNT.min(heap.num_slots(sp) as usize) {
            specials.push(heap.slot(sp, i));
        }
        let cl = base + cl_off;
        let ncl = heap.num_slots(cl) as usize;
        let mut class_table = Vec::with_capacity(ncl);
        for i in 0..ncl {
            class_table.push(heap.slot(cl, i));
        }
        let active = Value::from_ptr(base + active_off);

        Ok(Vm {
            heap,
            class_table,
            specials,
            symbols,
            temp_roots: Vec::new(),
            active_process: active,
            site_registry,
            lookup_cache: vec![LookupEntry::default(); LOOKUP_CACHE_SIZE],
            hash_counter: 0,
            safepoint: crate::profile::SafepointShared::new(),
            gc_epoch: 0,
            max_stack_bytes: config.max_stack_bytes,
            stdout_capture: Vec::new(),
            scavenge_count: 0,
            compact_count: 0,
            stack_grow_count: 0,
            tenure_threshold: TENURE_AGE,
            files: Vec::new(),
            start_instant: std::time::Instant::now(),
            timer_requests: Vec::new(),
            snapshot_after_sends: None,
            sends_seen: 0,
            snapshot_fired_at_capture_len: None,
            counters: crate::counters::Counters::new(),
            profiler: crate::profile::Profiler::default(),
            host: crate::host_ui::HostUi::new(),
        })
    }
}
