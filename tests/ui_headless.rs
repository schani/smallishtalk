//! M0 — the UI host seam, exercised entirely headlessly (UI.md §4, §4A, §12).
//!
//! Everything here runs with no `ui` feature and no window: the in-memory
//! ARGB present buffer, the scripted event queue, the deterministic virtual
//! clock, the BitBlt workhorse, and the zero-dependency PNG screenshot writer.
//! These are the operability/testing/profiling pillars proving themselves on
//! the very first milestone.

use smallishtalk::asm::Insn::*;
use smallishtalk::fixture::MethodBuilder;
use smallishtalk::host_ui;
use smallishtalk::png;
use smallishtalk::treaty::*;
use smallishtalk::value::Value;
use smallishtalk::vm::Vm;

fn int(n: i64) -> Value {
    Value::from_int(n)
}

/// Install `selector` on `class` as primitive `prim`; the fallback body
/// answers the (positive) failure code, so tests see both paths.
fn install_prim(vm: &mut Vm, class: Value, selector: &str, prim: u16, argc: u8) {
    let m = MethodBuilder::new(argc, argc + 3)
        .primitive(prim)
        .insns(vec![Ret { a: argc + 1 }])
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

/// Install `selector` as primitive `prim` on Object (receiver-agnostic prims).
fn install_on_object(vm: &mut Vm, selector: &str, prim: u16, argc: u8) {
    let obj = object_class(vm);
    install_prim(vm, obj, selector, prim, argc);
}

/// Build a 1-bit Form value: slots [width, height, depth, bits].
/// `rows` are MSB-first packed bytes, `stride = ceil(width/8)` per row.
fn make_form(vm: &mut Vm, width: i64, height: i64, rows: &[u8]) -> Value {
    let cls = vm.new_test_class(FMT_FIXED, 4);
    let form = vm.make_instance(cls).unwrap();
    let ba_cls = vm.class_table_at(CLASS_BYTEARRAY);
    let bits = vm.make_instance_sized(ba_cls, rows.len()).unwrap();
    vm.heap.write_bytes(bits.as_ptr(), rows);
    vm.store_slot(form.as_ptr(), 0, int(width));
    vm.store_slot(form.as_ptr(), 1, int(height));
    vm.store_slot(form.as_ptr(), 2, int(1));
    vm.store_slot(form.as_ptr(), 3, bits);
    form
}

fn form_bits(vm: &Vm, form: Value) -> Vec<u8> {
    let bits = vm.heap.slot(form.as_ptr(), 3);
    vm.heap.bytes(bits.as_ptr()).to_vec()
}

/// Build a BitBlt setup value (14 slots, field order per UI.md §4.3).
#[allow(clippy::too_many_arguments)]
fn make_bitblt(
    vm: &mut Vm,
    dest: Value,
    source: Value,
    rule: i64,
    dx: i64,
    dy: i64,
    w: i64,
    h: i64,
    sx: i64,
    sy: i64,
    clip: (i64, i64, i64, i64),
) -> Value {
    let cls = vm.new_test_class(FMT_FIXED, 14);
    let bb = vm.make_instance(cls).unwrap();
    let nil = vm.nil();
    let fields = [
        dest,
        source,
        nil, // halftone
        int(rule),
        int(dx),
        int(dy),
        int(w),
        int(h),
        int(sx),
        int(sy),
        int(clip.0),
        int(clip.1),
        int(clip.2),
        int(clip.3),
    ];
    for (i, v) in fields.iter().enumerate() {
        vm.store_slot(bb.as_ptr(), i, *v);
    }
    bb
}

// --- PNG encoder (UI.md §4.4) ------------------------------------------------

/// Walk PNG chunks; return (name, data) pairs and verify each chunk's CRC.
fn parse_png_chunks(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10], "PNG signature");
    let mut out = Vec::new();
    let mut i = 8;
    while i < bytes.len() {
        let len = u32::from_be_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
        let name = String::from_utf8(bytes[i + 4..i + 8].to_vec()).unwrap();
        let data = bytes[i + 8..i + 8 + len].to_vec();
        let stored_crc = u32::from_be_bytes(bytes[i + 8 + len..i + 12 + len].try_into().unwrap());
        assert_eq!(stored_crc, png::crc32(&bytes[i + 4..i + 8 + len]), "CRC of {name}");
        out.push((name, data));
        i += 12 + len;
    }
    out
}

#[test]
fn png_encodes_valid_chunks_and_dimensions() {
    // 2x1 image: black then white.
    let rgb = [0, 0, 0, 255, 255, 255];
    let bytes = png::encode_rgb(2, 1, &rgb);
    let chunks = parse_png_chunks(&bytes);
    assert_eq!(chunks[0].0, "IHDR");
    assert_eq!(
        u32::from_be_bytes(chunks[0].1[0..4].try_into().unwrap()),
        2,
        "IHDR width"
    );
    assert_eq!(
        u32::from_be_bytes(chunks[0].1[4..8].try_into().unwrap()),
        1,
        "IHDR height"
    );
    assert_eq!(chunks[0].1[8], 8, "bit depth");
    assert_eq!(chunks[0].1[9], 2, "color type = truecolor");
    assert!(chunks.iter().any(|(n, _)| n == "IDAT"));
    assert_eq!(chunks.last().unwrap().0, "IEND");
}

#[test]
fn png_zlib_roundtrips_via_stored_blocks() {
    // The stored-block zlib stream must inflate back to the raw filtered
    // scanlines: every row prefixed with a 0 filter byte.
    let rgb: Vec<u8> = (0..300u32 * 3).map(|b| (b % 256) as u8).collect();
    let bytes = png::encode_rgb(300, 1, &rgb);
    let chunks = parse_png_chunks(&bytes);
    let idat: Vec<u8> = chunks
        .iter()
        .filter(|(n, _)| n == "IDAT")
        .flat_map(|(_, d)| d.clone())
        .collect();
    let raw = inflate_stored(&idat);
    let mut expected = vec![0u8]; // one scanline, filter byte 0
    expected.extend_from_slice(&rgb);
    assert_eq!(raw, expected);
}

/// Minimal inflate for zlib streams made only of stored (BTYPE=00) blocks.
fn inflate_stored(zlib: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 2; // skip 2-byte zlib header
    loop {
        let hdr = zlib[i];
        let bfinal = hdr & 1;
        i += 1;
        let len = u16::from_le_bytes([zlib[i], zlib[i + 1]]) as usize;
        i += 4; // LEN + NLEN
        out.extend_from_slice(&zlib[i..i + len]);
        i += len;
        if bfinal == 1 {
            break;
        }
    }
    out
}

// --- BitBlt (UI.md §4.3) -----------------------------------------------------

#[test]
fn bitblt_store_copies_source_over_dest() {
    let mut vm = Vm::bare_test();
    install_on_object(&mut vm, "copyBits", PRIM_BITBLT, 0);

    // 8x2 dest all zero; 8x2 source: row0 = 0b10101010, row1 = 0b11110000.
    let dest = make_form(&mut vm, 8, 2, &[0x00, 0x00]);
    let src = make_form(&mut vm, 8, 2, &[0xAA, 0xF0]);
    let bb = make_bitblt(&mut vm, dest, src, 3, 0, 0, 8, 2, 0, 0, (0, 0, 8, 2));
    let r = send(&mut vm, "copyBits", bb, &[]);
    assert!(!r.is_int() || r.as_int() <= 0, "primitive should succeed, got {r:?}");
    assert_eq!(form_bits(&vm, dest), vec![0xAA, 0xF0]);
}

#[test]
fn bitblt_or_xor_and_clear_rules() {
    let mut vm = Vm::bare_test();
    install_on_object(&mut vm, "copyBits", PRIM_BITBLT, 0);
    let src = make_form(&mut vm, 8, 1, &[0b1100_1100]);

    // OR (rule 7): 0b10101010 | 0b11001100 = 0b11101110
    let dest = make_form(&mut vm, 8, 1, &[0b1010_1010]);
    let bb = make_bitblt(&mut vm, dest, src, 7, 0, 0, 8, 1, 0, 0, (0, 0, 8, 1));
    send(&mut vm, "copyBits", bb, &[]);
    assert_eq!(form_bits(&vm, dest), vec![0b1110_1110]);

    // XOR (rule 6): 0b10101010 ^ 0b11001100 = 0b01100110
    let dest = make_form(&mut vm, 8, 1, &[0b1010_1010]);
    let bb = make_bitblt(&mut vm, dest, src, 6, 0, 0, 8, 1, 0, 0, (0, 0, 8, 1));
    send(&mut vm, "copyBits", bb, &[]);
    assert_eq!(form_bits(&vm, dest), vec![0b0110_0110]);

    // Clear (rule 0): ignores source, zeroes the dest rect.
    let dest = make_form(&mut vm, 8, 1, &[0b1111_1111]);
    let bb = make_bitblt(&mut vm, dest, src, 0, 0, 0, 8, 1, 0, 0, (0, 0, 8, 1));
    send(&mut vm, "copyBits", bb, &[]);
    assert_eq!(form_bits(&vm, dest), vec![0b0000_0000]);
}

#[test]
fn bitblt_clips_to_rectangle_and_offsets_source() {
    let mut vm = Vm::bare_test();
    install_on_object(&mut vm, "copyBits", PRIM_BITBLT, 0);

    // Store a fully-set 8x1 source into a zero dest, but clip to x in [2,6).
    let dest = make_form(&mut vm, 8, 1, &[0x00]);
    let src = make_form(&mut vm, 8, 1, &[0xFF]);
    let bb = make_bitblt(&mut vm, dest, src, 3, 0, 0, 8, 1, 0, 0, (2, 0, 4, 1));
    send(&mut vm, "copyBits", bb, &[]);
    // Only columns 2,3,4,5 written → 0b00111100.
    assert_eq!(form_bits(&vm, dest), vec![0b0011_1100]);
}

#[test]
fn bitblt_rejects_malformed_source_and_undersized_dest() {
    let mut vm = Vm::bare_test();
    install_on_object(&mut vm, "copyBits", PRIM_BITBLT, 0);
    let dest = make_form(&mut vm, 8, 1, &[0x00]);

    // Non-nil, non-Form source → clean failure (not silently "no source").
    let bb = make_bitblt(&mut vm, dest, int(42), 3, 0, 0, 8, 1, 0, 0, (0, 0, 8, 1));
    assert!(send(&mut vm, "copyBits", bb, &[]).as_int() > 0, "bad source must fail");
    assert_eq!(form_bits(&vm, dest), vec![0x00], "dest untouched on failure");

    // 8-bit-deep source is rejected (only 1-bit forms supported).
    let deep = make_form(&mut vm, 8, 1, &[0xFF]);
    vm.store_slot(deep.as_ptr(), 2, int(8));
    let bb = make_bitblt(&mut vm, dest, deep, 3, 0, 0, 8, 1, 0, 0, (0, 0, 8, 1));
    assert!(send(&mut vm, "copyBits", bb, &[]).as_int() > 0, "non-1-bit source must fail");

    // A nil source is fine (rule 3 with no source clears the rect).
    let nil = vm.nil();
    let d2 = make_form(&mut vm, 8, 1, &[0xFF]);
    let bb = make_bitblt(&mut vm, d2, nil, 3, 0, 0, 8, 1, 0, 0, (0, 0, 8, 1));
    send(&mut vm, "copyBits", bb, &[]);
    assert_eq!(form_bits(&vm, d2), vec![0x00]);
}

#[test]
fn present_rejects_undersized_bits() {
    let mut vm = Vm::bare_test();
    install_on_object(&mut vm, "flush", PRIM_PIXEL_BLIT, 0);
    // Declares 8x2 (needs 2 bytes) but supplies only 1 → must fail, not pad.
    let form = make_form(&mut vm, 8, 2, &[0xFF]);
    let r = send(&mut vm, "flush", form, &[]);
    assert!(r.as_int() > 0, "undersized bits must fail cleanly");
    assert_eq!(vm.counters.frames_presented, 0, "nothing presented");
}

// --- Events (UI.md §4.2) -----------------------------------------------------

#[test]
fn next_event_pops_injected_events_in_order() {
    let mut vm = Vm::bare_test();
    let nil = vm.nil();
    install_on_object(&mut vm, "nextEvent", PRIM_NEXT_EVENT, 0);

    // Empty queue → nil (preserves the pre-UI stub contract).
    assert_eq!(send(&mut vm, "nextEvent", nil, &[]), vm.nil());

    vm.host.push_event([host_ui::EV_MOUSE_MOVE, 12, 34, 0, 0]);
    vm.host.push_event([host_ui::EV_KEY_DOWN, 65, 0, 'A' as i64, 0]);

    let e1 = send(&mut vm, "nextEvent", nil, &[]);
    assert!(e1.is_ptr());
    assert_eq!(vm.heap.num_slots(e1.as_ptr()), 5);
    assert_eq!(vm.heap.slot(e1.as_ptr(), 0), int(host_ui::EV_MOUSE_MOVE));
    assert_eq!(vm.heap.slot(e1.as_ptr(), 1), int(12));
    assert_eq!(vm.heap.slot(e1.as_ptr(), 2), int(34));

    let e2 = send(&mut vm, "nextEvent", nil, &[]);
    assert_eq!(vm.heap.slot(e2.as_ptr(), 0), int(host_ui::EV_KEY_DOWN));
    assert_eq!(vm.heap.slot(e2.as_ptr(), 3), int('A' as i64));

    // Drained → nil again.
    assert_eq!(send(&mut vm, "nextEvent", nil, &[]), vm.nil());
    assert_eq!(vm.counters.events_processed, 2);
}

// --- Present to the ARGB buffer (UI.md §4.1) ---------------------------------

#[test]
fn pixel_blit_expands_monochrome_into_argb_buffer() {
    let mut vm = Vm::bare_test();
    install_on_object(&mut vm, "flush", PRIM_PIXEL_BLIT, 0);

    // 8x1: 0b10000001 → black,white,white,white,white,white,white,black.
    let form = make_form(&mut vm, 8, 1, &[0b1000_0001]);
    send(&mut vm, "flush", form, &[]);

    assert_eq!(vm.host.buf_width, 8);
    assert_eq!(vm.host.buf_height, 1);
    assert_eq!(vm.host.buffer.len(), 8);
    assert_eq!(vm.host.buffer[0], 0xFF00_0000, "bit set → black");
    assert_eq!(vm.host.buffer[1], 0xFFFF_FFFF, "bit clear → white");
    assert_eq!(vm.host.buffer[7], 0xFF00_0000);
    assert_eq!(vm.counters.frames_presented, 1);
}

// --- Screenshot to PNG (UI.md §4A.3) -----------------------------------------

#[test]
fn save_form_writes_a_viewable_png() {
    let mut vm = Vm::bare_test();
    let nil = vm.nil();
    install_on_object(&mut vm, "saveForm:toFile:", PRIM_SAVE_FORM, 2);

    let form = make_form(&mut vm, 16, 2, &[0xFF, 0x00, 0x0F, 0xF0]);
    let dir = std::env::temp_dir().join(format!("smallishtalk-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("shot.png");
    let path_v = vm.make_string(path.to_str().unwrap()).unwrap();

    let r = send(&mut vm, "saveForm:toFile:", nil, &[form, path_v]);
    assert!(!r.is_int() || r.as_int() <= 0, "should succeed, got {r:?}");

    let bytes = std::fs::read(&path).unwrap();
    let chunks = parse_png_chunks(&bytes);
    assert_eq!(u32::from_be_bytes(chunks[0].1[0..4].try_into().unwrap()), 16);
    assert_eq!(u32::from_be_bytes(chunks[0].1[4..8].try_into().unwrap()), 2);
    std::fs::remove_dir_all(&dir).ok();
}

// --- Clocks & the virtual clock (UI.md §4A.1) --------------------------------

#[test]
fn monotonic_ns_clock_advances() {
    let mut vm = Vm::bare_test();
    let nil = vm.nil();
    install_on_object(&mut vm, "nowNs", PRIM_CLOCK_MONOTONIC_NS, 0);
    let t1 = send(&mut vm, "nowNs", nil, &[]);
    let t2 = send(&mut vm, "nowNs", nil, &[]);
    assert!(t1.is_int() && t2.is_int() && t2.as_int() >= t1.as_int());
}

#[test]
fn virtual_clock_is_deterministic_and_driver_advanced() {
    let mut vm = Vm::bare_test();
    let nil = vm.nil();
    install_on_object(&mut vm, "nowNs", PRIM_CLOCK_MONOTONIC_NS, 0);
    install_on_object(&mut vm, "nowMs", PRIM_CLOCK_MONOTONIC_MS, 0);

    vm.host.use_virtual_clock();
    assert_eq!(send(&mut vm, "nowNs", nil, &[]), int(0));
    vm.host.advance_virtual_ns(16_000_000); // one ~60Hz frame
    assert_eq!(send(&mut vm, "nowNs", nil, &[]), int(16_000_000));
    assert_eq!(send(&mut vm, "nowMs", nil, &[]), int(16));
    vm.host.advance_virtual_ns(16_000_000);
    assert_eq!(send(&mut vm, "nowNs", nil, &[]), int(32_000_000));
}

// --- Golden screenshot (UI.md §4A.3 / §12 item 4) ----------------------------

/// A hand-built Form presented headlessly hashes to a stable value. Monochrome
/// + deterministic present ⇒ zero flakiness; this is the seed of the golden
/// screenshot test substrate. If the present path changes meaning, this breaks.
#[test]
fn golden_hand_built_form_hashes_stably() {
    let mut vm = Vm::bare_test();
    install_on_object(&mut vm, "flush", PRIM_PIXEL_BLIT, 0);

    // A 16x4 diagonal: pixel (x,y) set iff x==y or x==(15-y).
    let w = 16usize;
    let h = 4usize;
    let stride = w.div_ceil(8);
    let mut rows = vec![0u8; stride * h];
    for y in 0..h {
        for x in 0..w {
            if x == y || x == (w - 1 - y) {
                rows[y * stride + (x >> 3)] |= 0x80 >> (x & 7);
            }
        }
    }
    let form = make_form(&mut vm, w as i64, h as i64, &rows);
    send(&mut vm, "flush", form, &[]);

    // FNV-1a over the ARGB present buffer.
    let mut hash = 0xcbf29ce484222325u64;
    for px in &vm.host.buffer {
        for b in px.to_le_bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    assert_eq!(hash, GOLDEN_DIAGONAL_16X4, "present buffer changed unexpectedly");
}

/// Golden: FNV-1a of the 16x4 diagonal's ARGB present buffer.
const GOLDEN_DIAGONAL_16X4: u64 = 15889629467470993573;
