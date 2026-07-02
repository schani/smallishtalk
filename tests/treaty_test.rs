//! Phase 0 tests: the Treaty as executable data (SPEC.md §20 Phase 0).
//! treaty.json is canonical; these tests assert src/treaty.rs agrees with it
//! exactly (both directions), plus the spec's exact-encoding examples.

use smallishtalk::treaty::*;
use std::collections::BTreeMap;

/// Minimal parser for the JSON subset treaty.json uses:
/// one top-level object of {group: {NAME: integer|string, ...}} plus
/// top-level "comment"/"version" scalars. Returns group -> name -> u64,
/// skipping string-valued entries.
fn parse_treaty_json(src: &str) -> BTreeMap<String, BTreeMap<String, u64>> {
    let mut chars = src.chars().peekable();
    let mut out = BTreeMap::new();

    fn skip_ws(c: &mut std::iter::Peekable<std::str::Chars>) {
        while matches!(c.peek(), Some(' ' | '\n' | '\t' | '\r' | ',')) {
            c.next();
        }
    }
    fn parse_string(c: &mut std::iter::Peekable<std::str::Chars>) -> String {
        assert_eq!(c.next(), Some('"'), "expected string");
        let mut s = String::new();
        loop {
            match c.next().expect("unterminated string") {
                '"' => return s,
                '\\' => s.push(c.next().expect("escape")),
                ch => s.push(ch),
            }
        }
    }
    fn parse_number(c: &mut std::iter::Peekable<std::str::Chars>) -> u64 {
        let mut s = String::new();
        while matches!(c.peek(), Some('0'..='9')) {
            s.push(c.next().unwrap());
        }
        s.parse().expect("number")
    }

    skip_ws(&mut chars);
    assert_eq!(chars.next(), Some('{'), "expected top-level object");
    loop {
        skip_ws(&mut chars);
        if chars.peek() == Some(&'}') {
            break;
        }
        let key = parse_string(&mut chars);
        skip_ws(&mut chars);
        assert_eq!(chars.next(), Some(':'));
        skip_ws(&mut chars);
        match chars.peek() {
            Some('{') => {
                chars.next();
                let mut group = BTreeMap::new();
                loop {
                    skip_ws(&mut chars);
                    if chars.peek() == Some(&'}') {
                        chars.next();
                        break;
                    }
                    let name = parse_string(&mut chars);
                    skip_ws(&mut chars);
                    assert_eq!(chars.next(), Some(':'));
                    skip_ws(&mut chars);
                    if chars.peek() == Some(&'"') {
                        parse_string(&mut chars); // string-valued: ignore
                    } else {
                        group.insert(name, parse_number(&mut chars));
                    }
                }
                out.insert(key, group);
            }
            Some('"') => {
                parse_string(&mut chars); // top-level comment: ignore
            }
            _ => {
                parse_number(&mut chars); // top-level version: ignore
            }
        }
    }
    out
}

fn load_json() -> BTreeMap<String, BTreeMap<String, u64>> {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/treaty.json"))
        .expect("treaty.json must exist at the crate root");
    parse_treaty_json(&src)
}

/// Every constant in treaty.rs matches treaty.json, and treaty.json contains
/// nothing treaty.rs doesn't (so neither side can drift).
#[test]
fn treaty_agrees_with_json() {
    let json = load_json();
    let rust = all_constants();

    let mut seen: BTreeMap<(String, String), bool> = BTreeMap::new();
    for (group, name, value) in &rust {
        let jgroup = json
            .get(*group)
            .unwrap_or_else(|| panic!("group {group:?} missing from treaty.json"));
        let jval = jgroup
            .get(*name)
            .unwrap_or_else(|| panic!("{group}.{name} missing from treaty.json"));
        assert_eq!(
            jval, value,
            "{group}.{name}: treaty.json says {jval}, treaty.rs says {value}"
        );
        seen.insert((group.to_string(), name.to_string()), true);
    }
    for (group, entries) in &json {
        for name in entries.keys() {
            assert!(
                seen.contains_key(&(group.clone(), name.clone())),
                "{group}.{name} is in treaty.json but not mirrored in treaty.rs"
            );
        }
    }
}

/// SPEC §20 Phase 0: "ADD with d=4,a=0,b=1 encodes to this exact u32".
/// ABC encoding: [opcode:8 | A:8 | B:8 | C:8], u32 little-endian in memory,
/// so as an integer: opcode | A<<8 | B<<16 | C<<24.
#[test]
fn add_encoding_example() {
    let insn: u32 = (OP_ADD as u32) | (4 << 8) | (0 << 16) | (1 << 24);
    assert_eq!(insn, 0x0100_0430);
}

/// SPEC §20 Phase 0: "header for a 3-slot fixed instance of class 17 with
/// hash 0 is this exact u64".
#[test]
fn header_encoding_example() {
    let header: u64 = (17u64 << HDR_CLASS_SHIFT)
        | (0 << HDR_HASH_SHIFT)
        | (3 << HDR_NSLOTS_SHIFT)
        | (FMT_FIXED << HDR_FORMAT_SHIFT)
        | 0;
    assert_eq!(header, 0x0000_4400_0000_3000);
}

/// SPEC §20 Phase 0: "frame slot RECEIVER is 4".
#[test]
fn frame_slot_receiver() {
    assert_eq!(FRAME_RECEIVER, 4);
}

/// Header fields must tile the 64-bit word exactly.
#[test]
fn header_fields_tile_the_word() {
    assert_eq!(HDR_GC_SHIFT + HDR_GC_BITS, HDR_FORMAT_SHIFT);
    assert_eq!(HDR_FORMAT_SHIFT + HDR_FORMAT_BITS, HDR_NSLOTS_SHIFT);
    assert_eq!(HDR_NSLOTS_SHIFT + HDR_NSLOTS_BITS, HDR_HASH_SHIFT);
    assert_eq!(HDR_HASH_SHIFT + HDR_HASH_BITS, HDR_CLASS_SHIFT);
    assert_eq!(HDR_CLASS_SHIFT + HDR_CLASS_BITS, 64);
}

/// Spot-check opcode numbers against the normative A.2 table.
#[test]
fn opcode_numbers_match_a2() {
    assert_eq!(OP_NOP, 0x00);
    assert_eq!(OP_BREAK, 0x01);
    assert_eq!(OP_MOVE, 0x10);
    assert_eq!(OP_LOADK, 0x11);
    assert_eq!(OP_MKBOX, 0x1B);
    assert_eq!(OP_SEND, 0x20);
    assert_eq!(OP_PRIM, 0x25);
    assert_eq!(OP_JUMPFALSE, 0x2A);
    assert_eq!(OP_ADD, 0x30);
    assert_eq!(OP_EQNUM, 0x39);
    assert_eq!(OP_AT, 0x40);
    assert_eq!(OP_IDEQ, 0x45);
}

/// Spot-check A.3's explicitly listed class indices and primitive numbers.
#[test]
fn class_indices_match_a3() {
    assert_eq!(CLASS_UNDEFINED_OBJECT, 5);
    assert_eq!(CLASS_TRUE, 6);
    assert_eq!(CLASS_FALSE, 7);
    assert_eq!(CLASS_SMALLINTEGER, 8);
    assert_eq!(CLASS_FLOAT, 9);
    assert_eq!(CLASS_BYTESTRING, 12);
    assert_eq!(CLASS_SYMBOL, 13);
    assert_eq!(CLASS_ARRAY, 16);
    assert_eq!(CLASS_BYTEARRAY, 17);
    assert_eq!(CLASS_BOX, 20);
    assert_eq!(CLASS_BLOCKCLOSURE, 21);
    assert_eq!(CLASS_COMPILEDMETHOD, 22);
    assert_eq!(CLASS_COMPILEDBLOCK, 23);
    assert_eq!(CLASS_PROCESS, 24);
    assert_eq!(CLASS_SEMAPHORE, 25);
    assert_eq!(CLASS_METHODDICTIONARY, 26);
    assert_eq!(PRIM_SNAPSHOT, 400);
    assert_eq!(PRIM_REGISTER_CLASS, 401);
    assert_eq!(PRIM_METHOD_INSTALL, 402);
    assert_eq!(PRIM_FRAME_INFO, 410);
}

/// "STIM" magic reads as this u32 little-endian.
#[test]
fn image_magic() {
    assert_eq!(IMG_MAGIC, u32::from_le_bytes(*b"STIM"));
}

/// The Smalltalk mirror of the Treaty must be regenerated whenever the
/// constants change (`cargo run --bin gen_treaty_st`).
#[test]
fn treaty_st_is_current() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/st/compiler/Treaty.st");
    let on_disk = std::fs::read_to_string(path)
        .expect("st/compiler/Treaty.st missing — run `cargo run --bin gen_treaty_st`");
    assert_eq!(
        on_disk,
        treaty_st_source(),
        "st/compiler/Treaty.st is stale — run `cargo run --bin gen_treaty_st`"
    );
}
