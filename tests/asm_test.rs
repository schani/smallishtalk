//! Phase 1 tests: the assembler and its inverse, round-trip-tested against
//! each other (SPEC §20 Phase 1).

use smallishtalk::asm::{assemble, disassemble, Insn};
use smallishtalk::treaty::*;

#[test]
fn abc_encoding_exact() {
    // [opcode:8 | A:8 | B:8 | C:8] as a little-endian u32 integer.
    assert_eq!(Insn::Add { d: 4, a: 0, b: 1 }.encode(), 0x0100_0430);
    assert_eq!(Insn::Move { d: 2, a: 7 }.encode(), 0x0007_0210);
    assert_eq!(
        Insn::Send { d: 1, r: 2, site: 3 }.encode(),
        (OP_SEND as u32) | (1 << 8) | (2 << 16) | (3 << 24)
    );
}

#[test]
fn ad_encoding_exact() {
    // [opcode:8 | A:8 | D:16]
    assert_eq!(
        Insn::LoadK { d: 3, k: 0x1234 }.encode(),
        (OP_LOADK as u32) | (3 << 8) | (0x1234 << 16)
    );
    // D is signed for jumps: offset -1 encodes as 0xFFFF.
    assert_eq!(
        Insn::Jump { off: -1 }.encode(),
        (OP_JUMP as u32) | (0xFFFF << 16)
    );
    assert_eq!(
        Insn::JumpTrue { a: 5, off: -2 }.encode(),
        (OP_JUMPTRUE as u32) | (5 << 8) | (0xFFFE << 16)
    );
    // LOADINT's immediate is signed.
    assert_eq!(
        Insn::LoadInt { d: 0, imm: -7 }.encode(),
        (OP_LOADINT as u32) | ((-7i16 as u16 as u32) << 16)
    );
}

#[test]
fn round_trip_every_instruction() {
    let all = vec![
        Insn::Nop,
        Insn::Break,
        Insn::Move { d: 1, a: 2 },
        Insn::LoadK { d: 3, k: 65535 },
        Insn::LoadInt { d: 4, imm: -32768 },
        Insn::LoadInt { d: 4, imm: 32767 },
        Insn::LoadNil { d: 5 },
        Insn::LoadTrue { d: 6 },
        Insn::LoadFalse { d: 7 },
        Insn::LoadSelf { d: 8 },
        Insn::GetIvar { d: 9, i: 10 },
        Insn::SetIvar { i: 11, a: 12 },
        Insn::GetBox { d: 13, a: 14 },
        Insn::SetBox { a: 15, b: 16 },
        Insn::MkBox { d: 17, a: 18 },
        Insn::Send { d: 19, r: 20, site: 21 },
        Insn::SendSuper { d: 22, r: 23, site: 24 },
        Insn::Ret { a: 25 },
        Insn::RetSelf,
        Insn::Nlr { a: 26 },
        Insn::Prim { n: 4095 },
        Insn::MkClosure { d: 27, b: 28 },
        Insn::Capture { c: 29, a: 30 },
        Insn::Jump { off: -300 },
        Insn::JumpTrue { a: 31, off: 300 },
        Insn::JumpFalse { a: 32, off: 0 },
        Insn::Add { d: 1, a: 2, b: 3 },
        Insn::Sub { d: 1, a: 2, b: 3 },
        Insn::Mul { d: 1, a: 2, b: 3 },
        Insn::Div { d: 1, a: 2, b: 3 },
        Insn::Mod { d: 1, a: 2, b: 3 },
        Insn::Lt { d: 1, a: 2, b: 3 },
        Insn::Gt { d: 1, a: 2, b: 3 },
        Insn::Le { d: 1, a: 2, b: 3 },
        Insn::Ge { d: 1, a: 2, b: 3 },
        Insn::EqNum { d: 1, a: 2, b: 3 },
        Insn::At { d: 1, a: 2, b: 3 },
        Insn::AtPut { d: 1, a: 2, b: 3 },
        Insn::Size { d: 1, a: 2 },
        Insn::ClassOf { d: 1, a: 2 },
        Insn::Not { d: 1, a: 2 },
        Insn::IdEq { d: 1, a: 2, b: 3 },
    ];
    for insn in &all {
        let word = insn.encode();
        let back = Insn::decode(word).unwrap_or_else(|| panic!("decode failed for {insn:?}"));
        assert_eq!(&back, insn, "round-trip of {insn:?} (word {word:#010x})");
    }

    let words = assemble(&all);
    assert_eq!(words.len(), all.len());
    assert_eq!(disassemble(&words), all);
}

#[test]
fn decode_rejects_unknown_opcodes() {
    assert!(Insn::decode(0xFF).is_none());
    assert!(Insn::decode(0x02).is_none()); // reserved gap
    assert!(Insn::decode(0x2B).is_none()); // gap after JUMPFALSE
}

#[test]
fn display_is_readable() {
    assert_eq!(format!("{}", Insn::Add { d: 4, a: 0, b: 1 }), "ADD 4, 0, 1");
    assert_eq!(format!("{}", Insn::Jump { off: -1 }), "JUMP -1");
    assert_eq!(format!("{}", Insn::RetSelf), "RETSELF");
    assert_eq!(
        format!("{}", Insn::Send { d: 1, r: 2, site: 0 }),
        "SEND 1, 2, 0"
    );
}
