//! smallishtalk: a Smalltalk VM per SPEC.md — interpreter, generational GC,
//! primitives, image snapshots. Interpreter only (no JIT).

pub mod asm;
pub mod counters;
pub mod fixture;
pub mod gc;
pub mod heap;
pub mod host_ui;
pub mod image;
pub mod interp;
pub mod png;
pub mod prims;
pub mod profile;
pub mod treaty;
pub mod value;
pub mod vm;
