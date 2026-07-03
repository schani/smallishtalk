//! smallishtalk: a Smalltalk VM per SPEC.md — interpreter, generational GC,
//! primitives, image snapshots, and the VM half of the template JIT
//! (JIT.md; the compiler half is Smalltalk, in-image).

pub mod asm;
pub mod counters;
pub mod fixture;
pub mod gc;
pub mod heap;
pub mod image;
pub mod interp;
pub mod jit;
pub mod prims;
pub mod profile;
pub mod treaty;
pub mod value;
pub mod vm;
