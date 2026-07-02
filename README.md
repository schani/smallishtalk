# smallishtalk

A complete implementation of [SPEC.md](SPEC.md): the Rust VM (**interpreter
only, no JIT**) *and* the Smalltalk-side compiler with its GNU Smalltalk
bootstrap, through the spec's Phase 5 — the compiler self-hosts on the VM
and its output is bit-identical to the GST cross-compile.

## The two codebases

**The VM (Rust, `src/`)** and **the compiler (portable Smalltalk, `st/`)**
share one binary contract: `treaty.json`, mirrored as `src/treaty.rs` and
the generated `st/compiler/Treaty.st` (`cargo run --bin gen_treaty_st`),
with tests holding all three together.

## Bootstrap pipeline

1. **Phase 2** — the compiler (lexer, parser, chunk reader, codegen with
   capture analysis and control-flow macros, encoder, STIM heap writer)
   runs under GNU Smalltalk 3.2.5 with SUnit suites
   (`./run-st-tests.sh`).
2. **Phase 3** — the handshake: `corpus/*.st` programs are cross-compiled
   to images and executed by the VM; stdout is diffed against
   `corpus/*.expected` (`cargo test --test corpus_test`).
3. **Phase 4** — the kernel (`st/kernel/kernel.st`) grows corpus-first:
   collections, streams, closures/NLR, the full in-image exception system
   over the §11 primitives, processes/semaphores/Delay/terminate. The
   corpus also runs under GC stress (64 KB young space) and through
   snapshot/reload round-trips at an arbitrary send boundary.
4. **Phase 5** — self-hosting (`cargo test --release --test phase5_test`):
   a self-host image contains the kernel plus the compiler's own source
   (compiled by our own codegen). Running it, the in-image compiler
   compiles the entire corpus and the kernel **bit-identically** to GST's
   output — and compiles *itself* bit-identically, with the resulting
   third-generation image running correctly.

The portable-dialect divergences between GST and the self-hosted kernel
are confined to `st/compiler/Compat.st` (GST side: `charAt:`,
`Platform bytesOf:`) and documented conventions (`String at:` answers
bytes per §15; Symbol equality is identity, so `asString` copies;
`Array =` is elementwise; class-side `super` sites bind the metaclass).

## What's here

| Module | Spec | Contents |
|---|---|---|
| `treaty.json` / `src/treaty.rs` | App. A | The binary contract as executable data; a test asserts the two agree in both directions |
| `src/value.rs` | §1 | 63-bit SmallInteger / pointer tagging |
| `src/heap.rs` | §2–3, §14 | Headers, three formats, overflow size word, young semispaces + old space, linear heap walking |
| `src/asm.rs` | §6 | Assembler/disassembler for the 32-bit register bytecode (test tool / debugging tool) |
| `src/vm.rs` | §4–5, §7 | Bare bootstrap (nil/true/false, Treaty classes, specials), lookup, write barrier, method install, processes |
| `src/interp.rs` | §6–8, §12–13 | The register interpreter: overlapping frames, stack growth, sends with inline + global caches, specialized sends, closures, NLR, the re-entrant unwinder, safepoints |
| `src/prims.rs` | §16 | The numbered primitive table (object essentials, SmallInteger/Float, blocks, exceptions, processes/semaphores, files/stdio/clocks, system) |
| `src/gc.rs` | §14 | Copying scavenger with age-based tenuring + SSB, and Lisp-2 sliding mark-compact for old space |
| `src/image.rs` | §17 | STIM snapshot write / load with one-pass pointer relocation |
| `src/fixture.rs` | §20 | The heap-builder fixture (`MethodBuilder`) for hand-assembled-bytecode tests |

`cargo test` runs the whole Phase-1-style suite (137 tests): per-opcode
interpreter tests, GC unit + stress tests, closure/NLR batteries, an
exception battery driven by a hand-assembled in-image exception kernel,
process/scheduler tests, and snapshot round-trips that resume mid-method.

The binary runs an image: `smallishtalk <image.im>`.

## Documented v1 concretizations / deviations

Decisions the spec leaves open (or that this implementation makes
differently, noted here and in code comments):

- **Stack scanning.** Instead of pinning the running stack, the interpreter
  re-derives its cached registers after every collection (the Invariant's
  sanctioned alternative). Stacks maintain the invariant that *every* word
  is a valid tagged value (temps nil-filled on push, frames nil-filled on
  pop), so the GC scans whole stack objects with no live-extent computation.
- **Frame slots.** The method header's `frameSlots` counts bytecode-visible
  slots (receiver + args + temps + scratch); the frame footprint is
  `4 + frameSlots`. A send staged at bytecode slot `r` puts the callee's
  four control words in caller slots `r-4..r-1`, so `r ≥ 4` and anything
  live across the send must sit below `r-4`.
- **Specialized-send slow paths** stage receiver+args in the free area
  above the current frame and dispatch through the ordinary send machinery
  using the Treaty `specializedSelectors` array (the ABC encodings carry no
  send-site index).
- **Exception helpers.** Three small extra primitives (222–224:
  `handlerInfoAt:`, `setHandlerState:to:`, `signalContext`) let the
  in-image `signal` loop read handler frames without bit-twiddling method
  headers in bytecode. `primFindHandler` applies the plain is-kind-of test
  VM-side for v1 (the image-side `handles:` loop can take over later).
- **Ensure interception.** Pending unwind state (target, serial, value)
  lives in the ensure frame's reserved slots; the unwinder activates the
  ensure block with a continuation-marker frame flag (`FLAG_UNWINDCONT`).
  Fully re-entrant; an NLR out of an ensure block abandons the pending
  unwind naturally.
- **Termination.** `terminate` on self unwinds to the base frame (running
  ensures) and marks the process terminated; on another process it pushes
  the Treaty terminate trampoline onto *that* process's stack — the
  trampoline is a method whose primitive is `terminate` with the process as
  receiver, so the target performs the same self-termination in its own
  context.
- **Old-space walking.** A storage word with zero class bits is an overflow
  size word (real headers always have a nonzero classIndex); this makes the
  heap linearly walkable with no side tables.
- **StackOverflow** beyond the image limit is a Rust-level error for now
  (the in-image signal arrives with the image kernel).
- **MethodDictionary** is Treaty-fixed as parallel keys/values Arrays with
  linear identity scan (behind the global lookup cache).
