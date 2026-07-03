# JIT Implementation Status

Implementation of JIT.md, milestones M0–M6, AMD64 only (M7/ARM64 not
started). All milestones landed with their test batteries green:
`cargo test` (30 targets, including the in-image differential selftest,
the four Phase 6 corpus modes, and the M0 blob harness) and
`./run-st-tests.sh` (120 SUnit tests).

## What exists

**Rust side (src/jit.rs + integration, ~1.6k lines).** Contiguous mmap
code cache with guard pages and W^X flipping; handle table with runtime
mirror for compiled code; globals page (GBL) and linkage table (LNK);
hand-written entry trampoline and in-cache glue (exit path, CALL_INTERP);
runtime stubs per Annex J.3 (ALLOC, BARRIER_REMEMBER, STACK_GROW,
SAFEPOINT, SEND_MISS, SEND_SLOW, PRIM_CALL, NLR, RESUME_AT) with the
leaf/allocating/exiting disposition ABI; mechanical patch routines for the
two frozen site shapes; counter trip + request queue + jitSemaphore;
the flat-loop transitions (J5) with a guaranteed-progress rule; loader
vmState reset; GC roots for queue and handles.

**Smalltalk side (st/jit/, ~2.5k lines).** StAMD64Assembler +
strict-subset disassembler (golden-tested in-image, under GST, and
against objdump); StAMD64MacroAssembler (coarse ops, the portability
boundary); StJITMethodCompiler (two passes, templates for every opcode —
unsupported forms compile to a sound exit-to-interpreter re-execution
fallback); StJITCompiler + the background JIT process; StJITProfiler
(tier residency, per-site cache stats, megamorphic finder).

**Framed primitives.** Compiled sends push the callee frame before the
primitive runs, so primitives support the framed convention (r == 0):
block-value primitives tail-replace the frame, suspending primitives
retire via the no-re-entry return (retire-then-suspend, JIT.md §10),
signalContext skips its own frame, and unframeable primitives (perform:,
snapshot) are do-not-compile and never patched into.

## Benchmarks (release, median of 5, warmed; bench/run_jit.sh)

| workload            | interp | JIT   | speedup |
|---------------------|--------|-------|---------|
| send_loop           | 23 ms  | 4 ms  | 5.8×    |
| string_build        | 100 ms | 20 ms | 5.0×    |
| arith_loop          | 30 ms  | 9 ms  | 3.3×    |
| ordered_collection  | 17 ms  | 7 ms  | 2.4×    |
| block_value         | 26 ms  | 14 ms | 1.9×    |
| dictionary          | 414 ms | 286 ms| 1.4×    |
| exceptions          | 10 ms  | 7 ms  | 1.4×    |
| process_pingpong    | 3 ms   | 5 ms  | 0.6×    |

Compile speed: **~0.6 ms/method** with the compiler hot (M6 target < 1 ms ✓).

### Self-compile: the JIT is ~18× *slower* here (important finding)

The compiler compiling itself (the project's headline macro workload) runs
in **850 ms interpreted** and **~15.0 s fully JIT-warmed** (4 warmup passes,
tiering then disabled so the timed pass includes no compile overhead; gen2
image bit-identical; 881 methods installed, zero request drops). That is an
~18× *regression*, not a speedup.

Cause (measured with per-activation and per-site counters, timed pass
isolated, request queue raised to 64K so no compilation requests are
dropped):

- **Only 881 methods ever compile.** Every activation calls
  `bump_invocation`, so any method run twice past the threshold trips —
  but only 881 distinct methods reach that state. The rest of the
  workload is blocks, primitive-bodied methods, and methods run too few
  times, none of which become native.
- **83% of activations run interpreted anyway**: `jit.act_interp`
  18.4M vs `jit.act_native` 3.7M. So the compiled 881 are a minority of
  what actually executes; the workload is overwhelmingly interpreted with
  the JIT machinery layered on top.
- **Stale routing is NOT the cause.** `jit.sites_direct` 5.4M vs
  `jit.sites_interp` 3.5K — when a call site was wired, the callee was
  already compiled 99.9% of the time, so the missing lazy-CALL_INTERP
  upgrade is negligible here. (My first write-up blamed this; the
  counters refute it.)
- **The real per-crossing cost is specialized-send slow paths.** The
  compiler is comparison/accessor-dense over *heterogeneous* objects:
  2.9M `=` fall-throughs, plus `at:`/`size`/`at:put:` on non-array
  receivers. Each is an EQNUM/AT/… template whose fast path misses (the
  operand isn't a SmallInteger / the receiver isn't array-format) and
  takes SEND_SLOW → exit trampoline → interpreter runs the real method →
  RESUME → re-enter. That is a full round-trip per operation, far more
  expensive than the interpreter's one inline `spec_slow`. The template
  JIT's specialization is tuned for SmallInteger arithmetic; on
  symbol/string/node comparisons it is pure overhead.

So the flat-loop cross-tier cost (JIT.md §20's accepted risk) is real,
but the dominant contributor is specialized sends on non-fast-path types
bouncing through SEND_SLOW, compounded by 83% of activations never being
compiled at all. The JIT wins only when code *stays* native across a
tight, SmallInteger/array-typed loop (send_loop 5.8×, arith 3.3×,
string_build 5.0×); a comparison-dense, heterogeneous, many-method sweep
like self-compile is its worst case.

The levers (all v2, not template tweaks): (1) synthesize real inline-cache
send sites for specialized-send slow paths so a repeated `=` on symbols
caches and stays native (JIT.md §20's PIC hook); (2) compile the whole
transitive callee set so boundaries disappear; (3) lazy CALL_INTERP
upgrade. Until then, enable the JIT selectively for hot numeric/array
loops, not as a blanket over a broad workload.

Tuning fixes made while diagnosing this (all correct regardless):
the tiering trip used `==` threshold so a request dropped on queue
overflow never re-tripped (now `>=`); a higher-priority wake (the
background compiler) didn't arm the safepoint, so a CPU-bound process
could starve it (now it does); `drain` chased its own tail forever since
compiling a method trips the compiler's own methods (now a bounded
snapshot); and the compilation queue was 256 entries — absurdly small,
causing 99.98% request drops under aggressive tiering (now 64K).

Against the M6 targets: sends ≥5× ✓; the arithmetic micro lands at 3.3×
(the loop's `bitXor:` is a full send per iteration — inline int-prim fast
paths brought it from 1.9×); the macro target is met by string_build but
not by dictionary/exceptions. The M5 profiler locates the residue
precisely: dictionary is dominated by staged specialized sends (`at:` and
`size` on fixed-format receivers — kernel Dictionary/OrderedCollection),
~14M per run, each a SEND_SLOW round-trip through Rust. The v2 lever is
synthesizing real send sites (with inline caches) for specialized-send
slow paths, per JIT.md §20's PIC hook. process_pingpong is
switch-dominated; each compiled wait/signal costs a trampoline round-trip
the interpreter doesn't pay — visible only on µs-scale switch loops.

## Deviations from JIT.md

- **primReadSamples** (§17's raw ring-buffer stream) is deferred to v1.x:
  tier residency and flat reports flow through the existing profiler
  primitives (primProfilerReport, primVmCounters) instead; the in-image
  StJITProfiler consumes those. primJITCodeInfo is implemented.
- **JIT primitives** live at 430–435 (Annex J.6 said "420–429" but that
  band was already taken by the profiling primitives).
- **CompiledBlock grew a pad slot** (BLOCK_PAD = 6, vmState = 7) so
  vmState sits at the same index in methods and blocks — the return
  template reads it without a class check.
- **CALL_INTERP glue** does not yet do the lazy tail-jump upgrade; the
  equivalent check happens in enter_native's exit path (one extra
  trampoline round-trip on the first post-compile call of a site).
- **DIV/MOD** are templated (floored idiv), beyond the plan's v1 scope.
- **The exit-to-interpreter re-execution fallback** (not in the plan)
  makes the compiler total from M2 onward: any bytecode without a
  template compiles to "park at this pc and let the interpreter
  re-execute it", which is always sound because pcs are bytecode offsets
  and all values live in frame slots (J2/J4). Back-edge re-entry in the
  interpreter and the RESUME_AT stub keep this from de-tiering loops.

## Latent VM bugs found by the JIT batteries (fixed)

1. **Old-space compaction cursor drift** (gc.rs): the slide advanced by
   `extra + footprint`, but footprint already includes the overflow word —
   8 bytes of drift per live overflow-header object, eventually sliding
   objects *forward* over unread neighbours. Found by JIT × GC-stress.
2. **Large-object routing vs tiny nurseries** (heap.rs): allocations under
   LARGE_OBJECT_BYTES but bigger than the (stress-shrunken) young space
   could never succeed; they now tenure at birth (remembered-at-birth to
   keep the templates' fresh-object barrier exemption sound).
3. **Send-arm safepoint preemption** (interp.rs): the poll ran with pc
   already past the SEND, so a cross-priority preemption parked the old
   process *past* the send and let the new process run the stale decode.
   Unreachable before the priority-7 JIT process existed.

## Not done

- M7 (ARM64): the macro-op catalog and per-arch Annex tables are in
  place; no ARM64 assembler/backend yet.
- v2 hooks from JIT.md §20 (PICs, literals register, inline young-space
  bump allocation, per-code eviction) — untouched, as planned.
