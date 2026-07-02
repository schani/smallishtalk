//! Assembler and disassembler for the 32-bit register bytecode (SPEC §6).
//!
//! The assembler exists for tests (hand-assembled methods); the disassembler
//! is a keeper debugging tool. They are round-trip-tested against each other.
//!
//! Encodings:
//! ```text
//! ABC:  [ opcode:8 | A:8 | B:8 | C:8 ]
//! AD:   [ opcode:8 | A:8 |   D:16    ]   ; D unsigned, or signed for jumps
//! ```

use crate::treaty::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Insn {
    Nop,
    Break,
    Move { d: u8, a: u8 },
    LoadK { d: u8, k: u16 },
    LoadInt { d: u8, imm: i16 },
    LoadNil { d: u8 },
    LoadTrue { d: u8 },
    LoadFalse { d: u8 },
    LoadSelf { d: u8 },
    GetIvar { d: u8, i: u8 },
    SetIvar { i: u8, a: u8 },
    GetBox { d: u8, a: u8 },
    SetBox { a: u8, b: u8 },
    MkBox { d: u8, a: u8 },
    Send { d: u8, r: u8, site: u8 },
    SendSuper { d: u8, r: u8, site: u8 },
    Ret { a: u8 },
    RetSelf,
    Nlr { a: u8 },
    Prim { n: u16 },
    MkClosure { d: u8, b: u16 },
    Capture { c: u8, a: u8 },
    Jump { off: i16 },
    JumpTrue { a: u8, off: i16 },
    JumpFalse { a: u8, off: i16 },
    Add { d: u8, a: u8, b: u8 },
    Sub { d: u8, a: u8, b: u8 },
    Mul { d: u8, a: u8, b: u8 },
    Div { d: u8, a: u8, b: u8 },
    Mod { d: u8, a: u8, b: u8 },
    Lt { d: u8, a: u8, b: u8 },
    Gt { d: u8, a: u8, b: u8 },
    Le { d: u8, a: u8, b: u8 },
    Ge { d: u8, a: u8, b: u8 },
    EqNum { d: u8, a: u8, b: u8 },
    At { d: u8, a: u8, b: u8 },
    AtPut { d: u8, a: u8, b: u8 },
    Size { d: u8, a: u8 },
    ClassOf { d: u8, a: u8 },
    Not { d: u8, a: u8 },
    IdEq { d: u8, a: u8, b: u8 },
}

fn abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
    (op as u32) | ((a as u32) << 8) | ((b as u32) << 16) | ((c as u32) << 24)
}

fn ad(op: u8, a: u8, d: u16) -> u32 {
    (op as u32) | ((a as u32) << 8) | ((d as u32) << 16)
}

impl Insn {
    pub fn encode(self) -> u32 {
        use Insn::*;
        match self {
            Nop => abc(OP_NOP, 0, 0, 0),
            Break => abc(OP_BREAK, 0, 0, 0),
            Move { d, a } => abc(OP_MOVE, d, a, 0),
            LoadK { d, k } => ad(OP_LOADK, d, k),
            LoadInt { d, imm } => ad(OP_LOADINT, d, imm as u16),
            LoadNil { d } => ad(OP_LOADNIL, d, 0),
            LoadTrue { d } => ad(OP_LOADTRUE, d, 0),
            LoadFalse { d } => ad(OP_LOADFALSE, d, 0),
            LoadSelf { d } => ad(OP_LOADSELF, d, 0),
            GetIvar { d, i } => abc(OP_GETIVAR, d, i, 0),
            SetIvar { i, a } => abc(OP_SETIVAR, i, a, 0),
            GetBox { d, a } => abc(OP_GETBOX, d, a, 0),
            SetBox { a, b } => abc(OP_SETBOX, a, b, 0),
            MkBox { d, a } => abc(OP_MKBOX, d, a, 0),
            Send { d, r, site } => abc(OP_SEND, d, r, site),
            SendSuper { d, r, site } => abc(OP_SENDSUPER, d, r, site),
            Ret { a } => ad(OP_RET, a, 0),
            RetSelf => ad(OP_RETSELF, 0, 0),
            Nlr { a } => ad(OP_NLR, a, 0),
            Prim { n } => ad(OP_PRIM, 0, n),
            MkClosure { d, b } => ad(OP_MKCLOSURE, d, b),
            Capture { c, a } => abc(OP_CAPTURE, c, a, 0),
            Jump { off } => ad(OP_JUMP, 0, off as u16),
            JumpTrue { a, off } => ad(OP_JUMPTRUE, a, off as u16),
            JumpFalse { a, off } => ad(OP_JUMPFALSE, a, off as u16),
            Add { d, a, b } => abc(OP_ADD, d, a, b),
            Sub { d, a, b } => abc(OP_SUB, d, a, b),
            Mul { d, a, b } => abc(OP_MUL, d, a, b),
            Div { d, a, b } => abc(OP_DIV, d, a, b),
            Mod { d, a, b } => abc(OP_MOD, d, a, b),
            Lt { d, a, b } => abc(OP_LT, d, a, b),
            Gt { d, a, b } => abc(OP_GT, d, a, b),
            Le { d, a, b } => abc(OP_LE, d, a, b),
            Ge { d, a, b } => abc(OP_GE, d, a, b),
            EqNum { d, a, b } => abc(OP_EQNUM, d, a, b),
            At { d, a, b } => abc(OP_AT, d, a, b),
            AtPut { d, a, b } => abc(OP_ATPUT, d, a, b),
            Size { d, a } => abc(OP_SIZE, d, a, 0),
            ClassOf { d, a } => abc(OP_CLASSOF, d, a, 0),
            Not { d, a } => abc(OP_NOT, d, a, 0),
            IdEq { d, a, b } => abc(OP_IDEQ, d, a, b),
        }
    }

    pub fn decode(word: u32) -> Option<Insn> {
        use Insn::*;
        let op = (word & 0xFF) as u8;
        let a = ((word >> 8) & 0xFF) as u8;
        let b = ((word >> 16) & 0xFF) as u8;
        let c = ((word >> 24) & 0xFF) as u8;
        let d16 = (word >> 16) as u16;
        Some(match op {
            OP_NOP => Nop,
            OP_BREAK => Break,
            OP_MOVE => Move { d: a, a: b },
            OP_LOADK => LoadK { d: a, k: d16 },
            OP_LOADINT => LoadInt { d: a, imm: d16 as i16 },
            OP_LOADNIL => LoadNil { d: a },
            OP_LOADTRUE => LoadTrue { d: a },
            OP_LOADFALSE => LoadFalse { d: a },
            OP_LOADSELF => LoadSelf { d: a },
            OP_GETIVAR => GetIvar { d: a, i: b },
            OP_SETIVAR => SetIvar { i: a, a: b },
            OP_GETBOX => GetBox { d: a, a: b },
            OP_SETBOX => SetBox { a, b },
            OP_MKBOX => MkBox { d: a, a: b },
            OP_SEND => Send { d: a, r: b, site: c },
            OP_SENDSUPER => SendSuper { d: a, r: b, site: c },
            OP_RET => Ret { a },
            OP_RETSELF => RetSelf,
            OP_NLR => Nlr { a },
            OP_PRIM => Prim { n: d16 },
            OP_MKCLOSURE => MkClosure { d: a, b: d16 },
            OP_CAPTURE => Capture { c: a, a: b },
            OP_JUMP => Jump { off: d16 as i16 },
            OP_JUMPTRUE => JumpTrue { a, off: d16 as i16 },
            OP_JUMPFALSE => JumpFalse { a, off: d16 as i16 },
            OP_ADD => Add { d: a, a: b, b: c },
            OP_SUB => Sub { d: a, a: b, b: c },
            OP_MUL => Mul { d: a, a: b, b: c },
            OP_DIV => Div { d: a, a: b, b: c },
            OP_MOD => Mod { d: a, a: b, b: c },
            OP_LT => Lt { d: a, a: b, b: c },
            OP_GT => Gt { d: a, a: b, b: c },
            OP_LE => Le { d: a, a: b, b: c },
            OP_GE => Ge { d: a, a: b, b: c },
            OP_EQNUM => EqNum { d: a, a: b, b: c },
            OP_AT => At { d: a, a: b, b: c },
            OP_ATPUT => AtPut { d: a, a: b, b: c },
            OP_SIZE => Size { d: a, a: b },
            OP_CLASSOF => ClassOf { d: a, a: b },
            OP_NOT => Not { d: a, a: b },
            OP_IDEQ => IdEq { d: a, a: b, b: c },
            _ => return None,
        })
    }
}

impl std::fmt::Display for Insn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Insn::*;
        match *self {
            Nop => write!(f, "NOP"),
            Break => write!(f, "BREAK"),
            Move { d, a } => write!(f, "MOVE {d}, {a}"),
            LoadK { d, k } => write!(f, "LOADK {d}, {k}"),
            LoadInt { d, imm } => write!(f, "LOADINT {d}, {imm}"),
            LoadNil { d } => write!(f, "LOADNIL {d}"),
            LoadTrue { d } => write!(f, "LOADTRUE {d}"),
            LoadFalse { d } => write!(f, "LOADFALSE {d}"),
            LoadSelf { d } => write!(f, "LOADSELF {d}"),
            GetIvar { d, i } => write!(f, "GETIVAR {d}, {i}"),
            SetIvar { i, a } => write!(f, "SETIVAR {i}, {a}"),
            GetBox { d, a } => write!(f, "GETBOX {d}, {a}"),
            SetBox { a, b } => write!(f, "SETBOX {a}, {b}"),
            MkBox { d, a } => write!(f, "MKBOX {d}, {a}"),
            Send { d, r, site } => write!(f, "SEND {d}, {r}, {site}"),
            SendSuper { d, r, site } => write!(f, "SENDSUPER {d}, {r}, {site}"),
            Ret { a } => write!(f, "RET {a}"),
            RetSelf => write!(f, "RETSELF"),
            Nlr { a } => write!(f, "NLR {a}"),
            Prim { n } => write!(f, "PRIM {n}"),
            MkClosure { d, b } => write!(f, "MKCLOSURE {d}, {b}"),
            Capture { c, a } => write!(f, "CAPTURE {c}, {a}"),
            Jump { off } => write!(f, "JUMP {off}"),
            JumpTrue { a, off } => write!(f, "JUMPTRUE {a}, {off}"),
            JumpFalse { a, off } => write!(f, "JUMPFALSE {a}, {off}"),
            Add { d, a, b } => write!(f, "ADD {d}, {a}, {b}"),
            Sub { d, a, b } => write!(f, "SUB {d}, {a}, {b}"),
            Mul { d, a, b } => write!(f, "MUL {d}, {a}, {b}"),
            Div { d, a, b } => write!(f, "DIV {d}, {a}, {b}"),
            Mod { d, a, b } => write!(f, "MOD {d}, {a}, {b}"),
            Lt { d, a, b } => write!(f, "LT {d}, {a}, {b}"),
            Gt { d, a, b } => write!(f, "GT {d}, {a}, {b}"),
            Le { d, a, b } => write!(f, "LE {d}, {a}, {b}"),
            Ge { d, a, b } => write!(f, "GE {d}, {a}, {b}"),
            EqNum { d, a, b } => write!(f, "EQNUM {d}, {a}, {b}"),
            At { d, a, b } => write!(f, "AT {d}, {a}, {b}"),
            AtPut { d, a, b } => write!(f, "ATPUT {d}, {a}, {b}"),
            Size { d, a } => write!(f, "SIZE {d}, {a}"),
            ClassOf { d, a } => write!(f, "CLASSOF {d}, {a}"),
            Not { d, a } => write!(f, "NOT {d}, {a}"),
            IdEq { d, a, b } => write!(f, "IDEQ {d}, {a}, {b}"),
        }
    }
}

pub fn assemble(insns: &[Insn]) -> Vec<u32> {
    insns.iter().map(|i| i.encode()).collect()
}

pub fn disassemble(words: &[u32]) -> Vec<Insn> {
    words
        .iter()
        .map(|w| Insn::decode(*w).unwrap_or_else(|| panic!("bad instruction {w:#010x}")))
        .collect()
}
