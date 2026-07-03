# The JIT Plan

**Version 0.1 — Design & Implementation Plan**

This document plans the template JIT sketched as constraints in SPEC.md §19 and Phase 6. It binds four decisions made up front:

1. **The JIT compiler is written in Smalltalk and lives in the image.** The VM's contribution is a small, enumerated set of primitives, runtime stubs, and patch routines — "minimal VM support" is quantified in Part V.
2. **Targets are AMD64 and ARM64 only**, AMD64 first. Portability machinery exists exactly to the degree those two targets differ, and no further.
3. **It is a template JIT**: one fixed machine-code sequence per bytecode, no IR, no register allocation across bytecodes, no speculation, therefore **no deoptimization**. Compile time is linear in bytecode length. Expected payoff is the classic template-JIT range (3–8× over the interpreter on send/arithmetic-heavy code) for a compiler that stays small enough to hold in one head.
4. **Profiling is a day-one requirement, not a retrofit**: tiering counters, inline-cache statistics, and a safepoint sampler are part of the initial design, with an in-image profiler consuming them.

Development happens **in-image, after Phase 5 self-hosting** (decided; the GST cross-host is not used for JIT work). The JIT is the first major subsystem developed entirely in the live system, and is designed to be debuggable by that system: the compiler is an ordinary background process, its output is inspectable ByteArrays, and a Smalltalk disassembler ships alongside the assembler.

Like SPEC.md, this document has a hard-invariant section (§1) from which most other choices follow, and a binary-contract appendix (the **JIT Annex** to the Treaty) that both codebases must agree on.

---

## Part I — Architecture

### 1. The JIT Invariants

Seven rules. Everything in Parts II–IV is a consequence of these plus SPEC.md's Stack Invariant.

> **J1 — No heap pointers in machine code.** Generated code may embed class indices, slot offsets, bytecode pcs, site indices, linkage-table indices, and tagged SmallInteger immediates — never an object address. Literals are reached at runtime through the frame's method slot. Consequence: the moving GC never scans, patches, or even knows about machine code.
>
> **J2 — All values live in frame slots at every call and safepoint.** Registers hold tagged values only *within* a single bytecode's template. Consequence: the GC needs no register maps or stack maps for compiled code; scanning a frame of compiled code is identical to scanning an interpreted frame, because it *is* one.
>
> **J3 — Frames are byte-identical across tiers.** Nothing stored in any heap object — frames, Processes, methods' Treaty slots 0–6 — distinguishes interpreted from compiled execution. The only tier-varying state is the VM-transient `vmState` slot (§5) and the VM's code tables. Consequence: snapshots, the debugger, the unwinder, and the scheduler are tier-oblivious by construction, as SPEC.md §19 requires.
>
> **J4 — Native code is entered at exactly two kinds of points**: a method's entry, and a *re-entry point* recorded in the method's re-entry map (send-return points and loop back-edge safepoints). Any other resumption — debugger single-step, restarted frame at pc 0 with a stale map, suspension inside a primitive, anything unmapped — falls back to the interpreter until the next call. The interpreter is always a correct executor of any frame.
>
> **J5 — Compiled execution never captures a native continuation.** The interpreter loop is the single dispatcher; native execution is always exactly one trampoline invocation deep, and every path out of compiled code — return to an interpreted caller, send to an interpreted callee, process switch, suspension inside a primitive, any fallback — unwinds the native stack completely back to the loop. There is no recursion between the interpreter and compiled code in either direction. Consequence: a process that blocks, is preempted, snapshots, or is terminated while "in" compiled code is, at that instant, exactly what SPEC.md §7 says a process is — heap frames plus `(pc, frameOffset)`; the native stack holds nothing. §4 gives the mechanics.
>
> **J6 — All code patching is done by the VM, at VM-time, at sites described by compiler-emitted metadata.** *VM-time* means the OS thread is executing VM (Rust) code — a stub, a primitive, the interpreter loop. One OS thread executes all Smalltalk, so at VM-time no compiled instruction is in flight anywhere, and patching is plain memory writes — no atomicity, no cross-modifying-code protocols (ARM64 adds an i-cache flush after the writes; that is the entire concurrency story). Miss-path cache fills, install-time invalidation, and flush-all are all VM-time by construction. Freshly patched code may be *returned into* (its memory is intact per J7); rewritten sites simply take their new path when next executed.
>
> **J7 — Machine code is never saved and never moves.** Snapshots are tier-free (SPEC.md Phase 6); code addresses are stable for the VM process's lifetime; invalidated code is *unlinked* immediately (never re-entered) but its memory is reclaimed only at whole-cache flush points (§12). Consequence: direct call patching is safe, and there is no code-GC to write.

### 2. Division of Labor

**Smalltalk owns everything that decides what instructions to emit**: the assemblers, the per-opcode templates, the method compiler, all metadata construction (re-entry maps, patch-site descriptors), the tiering policy, and the profiler.

**The VM (Rust) owns everything that touches executable memory or runs when no Smalltalk can**: the code cache (mmap, W^X), the install/unlink/flush primitives, the patch routines (mechanical byte-writers driven by the compiler's metadata), the runtime stubs compiled code calls into, the tier-transition trampolines, the counter-trip path, and the sampler.

The boundary rule: **the VM never selects or encodes an instruction** (its patch routines overwrite immediate fields inside instructions the compiler emitted, at shapes frozen in the Annex; its trampolines/glue are fixed hand-written sequences), **and the image never sees a raw address** (code is referred to by handle index; VM services are referred to by linkage-table index).

To be precise about what "minimal" claims: the VM's share is small (Part V is exhaustive) and contains no instruction selection, but it is not semantically trivial — the VM *owns* the execution-state machinery: tier transitions, code lifetime, invalidation, resume mapping, patch safety. Those must be correct when no Smalltalk can run, so they cannot live anywhere else. Minimal VM support means "the compiler is Smalltalk," not "the Rust side is dumb plumbing."

Trust model: the image is the system compiler and is trusted exactly as much as it already is for method installation — `primJITInstall` validates structurally (bounds, map sanity) but does not verify the machine code. A miscompile is a VM crash; the test strategy (Part VI) is the mitigation, plus guard pages around the code cache to make wild jumps fail fast and loud.

### 3. The Compilation Pipeline

**Counters.** Every CompiledMethod and CompiledBlock gains one Treaty slot, `vmState` (a SmallInteger packing: invocation counter, flag bits *queued / compiled / do-not-compile*, and a code-handle index; packing in Annex J.1). The interpreter's activation path increments the counter and, on crossing the tier threshold (VM flag, default 100; 0 = JIT-always mode), *trips*.

**The trip is a few instructions and runs no image code**: set the *queued* bit, append the method pointer to a small VM-side ring buffer (a GC root, 256 entries; if full, drop the request and clear *queued* — the method will simply trip again), and signal the Treaty-known **jitSemaphore**. This is the entire VM-side tiering mechanism.

**The JIT process** is an ordinary green-thread Smalltalk process at high priority, blocked on jitSemaphore:

```
[[true] whileTrue:
    [jitSemaphore wait.
     [method := VM nextCompilationRequest. method notNil] whileTrue:
        [JITCompiler compile: method]]] forkAt: Processor jitPriority
```

`compile:` translates bytecode to a ByteArray of machine code plus a metadata ByteArray (§14), then calls `primJITInstall:code:maps:`, which copies into the code cache, records the handle, stores the handle into `vmState`, sets *compiled*, clears *queued*. From the next activation on, the send machinery enters native code.

**Reentrancy is solved structurally.** The counter trip never calls image code, so nothing recursive can happen at a trip site. The JIT process's own methods trip counters like any others and get queued; the single compilation loop eventually compiles them (the compiler tiers itself up — the system's first benchmark). Compiling a method that is currently executing somewhere is fine: existing activations continue interpreted (J4 — they re-enter native code only at mapped points, and a frame activated under the interpreter simply finishes under it, entering code at its next *call*).

**Blocks** tier independently: `value` primitives increment the CompiledBlock's counter and trip identically.

**What is never compiled**: methods with the *do-not-compile* flag (set by the image for the handful of pathological cases: the terminate trampoline, methods under active debugger breakpointing). Everything else, including primitives-with-fallback and handler/ensure methods, compiles normally.

**Warm-up semantics** (accepted, per the background design): after a trip, a method runs interpreted until the JIT process gets scheduled and finishes — with green threads and high priority, typically one additional activation. Differential tests never depend on *when* tier-up happens, only that output is tier-independent; tests that must assert tier ("this ran compiled") drain the queue first via the image-side barrier `JITCompiler drain` (loops until `primJITQueueSize` is 0 and the last install completed).

### 4. Execution Model: One Flat Loop

Compiled code executes on the OS thread's native stack, but **Smalltalk frames stay in the heap stack object** exactly as interpreted (J3), and per J5 the native stack never holds a Smalltalk continuation. The interpreter loop is the sole dispatcher; native execution is one trampoline invocation that runs until compiled code has nothing left to do without the interpreter, then unwinds completely:

- **Loop → native**: the interpreter's activation path (send resolving to a compiled callee) and its return path (RET delivering into a frame whose method has live code) call the *entry trampoline*: save the C callee-saved registers, load the dedicated registers (§6), jump to the method entry or the mapped re-entry point (`returnPoints[siteIndex]` for returns).
- **Native → native**: compiled-to-compiled sends and returns store the frame control words and *jump*. No native frames, no loop involvement — this is the fast path the whole design serves.
- **Native → loop**: the *exit path* (in-cache glue) restores registers and returns from the entry trampoline; the loop continues from `Process.{frameOffset, pc}` and the frames themselves. **The loop's resume contract is that `Process.{frameOffset, pc}` are current at every entry to the loop, and each exit route is explicitly responsible for making them so**: the return template passes the caller's resume pc (unpacked from `returnInfo`) into the exit path, which stores it; `CALL_INTERP` stores pc 0 for the freshly handed-off callee frame; exiting stubs store the pc they were passed — or retire the current activation entirely (suspending primitives, §10) — before reporting *exit*. Exits happen at: a return whose caller frame is interpreted or unlinked; a send whose target is interpreted (the patched send target is then the in-cache `CALL_INTERP` glue, which completes the frame handoff and exits with the callee as the current frame); and any stub that reports an *exit* disposition (below). If the loop immediately finds another compiled boundary, it re-enters native code — cross-tier chatter costs a trampoline round-trip per transition, which lazy target upgrading (§8) removes where it repeats and steady-state compilation makes rare.
- **Stub dispositions**: every runtime stub is classified in Annex J.3 as *leaf* (cannot GC, cannot exit — e.g. `BARRIER_REMEMBER`), *allocating* (may run GC, always returns — `ALLOC`, `STACK_GROW`), or *exiting* (may need to relinquish native execution — `SAFEPOINT` on a process switch, `PRIM_CALL` on a suspending primitive, `SEND_MISS` when the resolved callee is interpreted or is DNU, `SEND_SLOW`, `NLR`, `MUST_BE_BOOLEAN`). Exiting stubs return a disposition word; the template's call sequence tests it and either continues in-line or jumps to the exit path. Every exiting-stub call site passes the current **bytecode pc as a template-emitted immediate**, so the stub stores a correct `Process.pc` before the process can become suspendable (§9 gives the full state discipline).

Suspension is thereby unremarkable anywhere it occurs: `Semaphore>>wait` reached from compiled code retires its activation and suspends inside `PRIM_CALL` (§10), reports *exit*, the native stack unwinds to the loop, the loop switches processes. The suspended process is pure heap state, parked at a send-return point; on resume, the re-entry map or the interpreter (J4) continues from the stored bytecode pc. Native stack depth is O(1) at all times — there is no ping-pong recursion to guard against.

**Calling convention across tiers is the frame itself.** Caller responsibilities at any send, both tiers, identical: write `callerFrameOffset`, `returnInfo`, `method`, `flagsAndSerial` (incrementing `Process.serialCounter`) into the callee frame area; set `Process.frameOffset` to the callee frame; enter the callee. `returnInfo` packs — this is a Treaty amendment, Annex J.1 — `resume pc:16 | dest slot:8 | send-site index:8`. The interpreter ignores the site index; compiled returns use it for O(1) re-entry (§8).

---

## Part II — The Generated Code

### 5. Code Objects, Handles, and Metadata

`primJITInstall: method code: aByteArray maps: aByteArray` copies code into the cache, parses/copies the metadata, and returns a **handle index** into the VM's handle table:

```
handle: { codeAddr, codeSize, method*,             ; method* keeps the method alive (VM root)
          reentryMap,                              ; sorted (bytecodePc:u16 → nativeOffset:u32) pairs
          returnPoints,                            ; dense u32 array indexed by send-site index
          patchSites,                              ; (nativeOffset:u32, kind:u8, siteIndex:u8) list
          state }                                  ; live | unlinked
```

Metadata formats are frozen in Annex J.4. The `returnPoints` array gives O(1) same-tier returns; the `reentryMap` covers loop back-edges (and doubles as the sampler's and resumer's pc→native map). `patchSites` registers every inline cache with the existing §8 (SPEC.md) invalidation registry: method installation clears compiled sites by patching, exactly as it clears interpreter send-site entries.

On image load, `vmState` slots are reset (handle bits zeroed, *compiled/queued* cleared) during the loader's existing walk of compiled methods — images are tier-free (J7).

### 6. Register Conventions (Annex J.2)

Dedicated registers are chosen from the C-ABI **callee-saved** set on each platform, so calls into Rust stubs (`extern "C"`) preserve them for free:

| Role | AMD64 | ARM64 |
|---|---|---|
| `FP` — frame pointer (→ frame slot 0) | `rbx` | `x19` |
| `LNK` — linkage table base | `r13` | `x20` |
| `GBL` — VM globals page (safepoint flag, active Process, young bounds) | `r14` | `x21` |
| reserved (future: literals pointer) | `r12` | `x22` |
| native stack | `rsp` | `sp` |
| scratch (templates only, dead at calls per J2) | rax rcx rdx rsi rdi r8–r11 | x0–x15 |

`FP` is materialized at entry/re-entry as `stackBase + frameOffset*8` and is valid **between safepoints** (Stack Invariant). `Process.frameOffset` is kept current at every call and return (it is part of the calling convention already), so after any stub call that could move the stack or run GC, templates re-derive `FP` from `GBL→activeProcess→{stack, frameOffset}` — a three-load sequence emitted by the macro-assembler only after *allocating* and *exiting* stubs (§4; Annex J.3 classifies every linkage entry).

Bytecode slot `i` is `[FP + 8*(4+i)]` (SPEC.md §19), a single addressing-mode operand on both targets.

### 7. The Linkage Table and Runtime Stubs

A VM-allocated table of stub addresses; compiled code calls `[LNK + 8*n]` with Annex-fixed indices `n`. The image never handles an address; the encoding is one indirect call on both architectures, and **indirect calls through `LNK` are the only way generated code reaches Rust text** — patched *direct* branches would face branch-range limits (ARM64 `b`/`bl` is ±128 MB; AMD64 rel32 likewise bounded, and Rust text lands wherever ASLR puts it). Stub inventory (Annex J.3, each entry classified leaf / allocating / exiting per §4; ~16 entries):

*Send machinery*: `SEND_MISS` (shared lookup: global cache → dictionaries → DNU; fills the heap send-site entry *and* patches the compiled site; exits if the resolved activation is interpreted), `SEND_SLOW` (specialized-send fallback: enters the ordinary send path for the site).
*Memory*: `ALLOC` (Box, closures, primitive `new` — v1 always calls out; inline bump allocation is a measured v2 change), `BARRIER_REMEMBER` (SSB append slow path; the young-bounds filter is inlined from `GBL`), `STACK_GROW`.
*Control*: `SAFEPOINT` (called with current bytecode pc when the flag is set), `NLR`, `PRIM_CALL` (invoke numbered primitive; on failure falls through to compiled fallback body; exits on suspension), `MUST_BE_BOOLEAN`.
*Profiling*: none — counters are inline memory ops, sampling rides `SAFEPOINT` (§17).

Stub calling convention: arguments in the platform C argument registers, so the stubs are plain `extern "C"` Rust functions. The exceptions are the **transition trampolines and in-cache glue** — the entry trampoline, the exit path, and the `CALL_INTERP` glue (§4) — which are short hand-written assembly sequences per platform. The glue is copied **into the code cache** at VM startup: patched direct branches (§8) must stay within the cache's contiguous reservation to be encodable on both architectures, so everything a patched branch can target lives in the cache by construction.

### 8. Sends, Returns, and Inline Caches

**Compiled send template** (monomorphic inline cache, per SPEC.md §8/§19):

```
  mov  siteReg, #siteIndex          ; site identity for miss/slow paths
  <load receiver from slot r>
  <classIndex := tag test ? SmallIntegerIndex : header extract>
  cmp  classIndexReg, #PATCH_CLASS  ; patchable 22-bit immediate, 0 when empty
  jne  →SEND_MISS call (cold section)
  <write 4 control words; method loaded from send-site entry's cacheMethod>
  <Process.frameOffset := callee offset; serial++>
  call/jmp PATCH_TARGET             ; patchable direct branch
```

The patched direct target is the callee's native entry if the callee is compiled, else the in-cache `CALL_INTERP` glue (which finds the callee method via the site's `cacheMethod`, checks its handle — tail-jumping into native code if the callee got compiled since the site was patched — and otherwise exits to the loop with the callee as current frame). **Patched targets always lie within the code cache** (compiled entries or glue; §7), so one contiguous ≤128 MB reservation keeps AMD64 rel32 and ARM64 imm26 encodable by construction. The VM's fill/clear routine updates `PATCH_CLASS`, `PATCH_TARGET`, and the heap send-site entry **together, at VM-time** (J6), keeping tiers coherent. When a *callee* gets compiled later, its callers' sites are *not* eagerly re-patched — `CALL_INTERP`'s handle check catches the upgrade dynamically, and the next fill re-patches the direct target lazily. Megamorphic sites behave as in the interpreter: perpetual `SEND_MISS` → global cache. (The send-site entry's spare slot remains reserved for a 2-entry PIC as v2.)

**Return template**: load `returnInfo` and `callerFrameOffset` from the frame; store result to caller's dest slot; set `Process.frameOffset`; load caller frame's method → `vmState` → handle. Compiled caller: indexed load `returnPoints[siteIndex]`, jump. Interpreted (or unlinked) caller: exit path (§4). Same-tier return is ~12 instructions with no search (this is what the `returnInfo` site-index amendment buys).

**Specialized sends** (SPEC.md §12) inline their fast paths exactly as specified — the tagged-arithmetic sequences, the format-dispatched `AT`/`ATPUT`/`SIZE`, the header-load `CLASSOF` — each with a conditional branch to a cold-section `SEND_SLOW` call carrying the site index. Slow paths and rare paths are grouped in a cold section at the end of the method's code so the hot line stays dense.

### 9. Safepoints, Loops, and Re-entry

The safepoint poll is SPEC.md §13's load-test-branch on the flag in `GBL`, emitted at: every method prologue (covering the send-side poll) and every backward jump. The poll's slow path calls `SAFEPOINT` with the current **bytecode pc as an immediate** (known at template-emission time); the stub stores it to `Process.pc`, making the process suspendable/scannable, then services the request. If the safepoint ran a GC or grew the stack, the return path re-derives `FP` (§6). If it *switched processes*, the process resumes later via `primTransferTo:`, which maps the stored bytecode pc through the handle's re-entry map — every back-edge poll site is a map entry, so **hot loops preempted by the timer resume in native code**, not the interpreter (without this, timer preemption would silently de-tier every long-running loop, since there is no OSR to recover).

The prologue also performs the frame-limit check (`STACK_GROW` on failure — the stub reallocates, memcpys, updates the Process, and returns the new base; sanctioned self-move per the Stack Invariant) and the invocation-counter increment (§16).

**State discipline** (what is current, when — this is the contract every stub and the GC rely on):

- `Process.frameOffset` is current at every call and return in both tiers (it is part of the calling convention). This alone is what lets a GC triggered inside an *allocating* stub scan the running process's stack correctly — live top is `frameOffset + method.frameSlots` — with no cooperation from compiled code.
- `Process.pc` follows SPEC.md §7: **stale while running**, and stored exactly at the points where the process may become suspendable or the loop may resume — by `SAFEPOINT` and every *exiting* stub (each receives the current bytecode pc as a template-emitted immediate; suspending primitives instead retire the activation first, §10, making the stored point a send-return pc), and by the exit routes themselves per §4's resume contract. No other stub needs a pc, and nothing reads a running process's pc (the sampler samples at `SAFEPOINT`, where it is fresh).
- Scratch registers never carry tagged values across any stub call (J2); templates re-load operands from frame slots after allocating calls and re-derive `FP` after allocating and exiting stubs (§6).

### 10. Blocks, Exceptions, NLR, Unwinding

Nothing here needs new machinery — this section exists to state *why*.

**Blocks**: a compiled CompiledBlock's code is entered by the `value` primitives' activation path exactly like a method (the closure is the receiver; captured-variable access is the ordinary `GETIVAR` template against the closure). `MKCLOSURE`/`CAPTURE`/`MKBOX` templates call `ALLOC` and do plain stores (Box and closure are young; no barrier needed at birth — the template still emits the filtered barrier for `SETBOX`/`SETIVAR` per SPEC.md §14).

**Exceptions and the unwinder** operate on frames, and frames are tier-identical (J3). `primFindHandler`/`primUnwindTo:` work unchanged. An ensure block activated mid-unwind runs through the normal activation path (interpreted or compiled per its own tier). The only JIT-aware point: when the unwinder or `resume:` re-enters a frame, the resume pc is a send-return point by construction (signals happen inside sends), so re-entry uses the same `returnPoints` path as an ordinary return; anything unmapped falls to the interpreter (J4).

**NLR**: the `NLR` template calls the `NLR` stub with the closure's home triple; liveness validation and the unwind are the existing VM logic. **Termination** (SPEC.md §11) needs nothing: the trampoline runs interpreted (*do-not-compile*).

**`PRIM`**: the template calls `PRIM_CALL` with the primitive number and the PRIM bytecode pc; success returns the value to the frame's dest and executes the return sequence; failure stores the fail code to the reserved slot and falls through to the compiled fallback body.

**The retire-then-suspend convention** (normative, and shared with the interpreter — both tiers' primitive machinery must implement it identically): a primitive that suspends the process (`wait`, `yield`, `transferTo:`, `primSnapshot:` on the load path) first **retires its own activation** — deliver the result to the caller's dest slot, pop the frame, set `Process.{frameOffset, pc}` to the caller and its `returnInfo` resume pc — and only then enqueues/switches, reporting *exit* (§4). Consequence: **a process is never suspended "inside" a primitive**; every suspension point in the entire system is a send-return point or a back-edge poll, both of which are re-entry-mapped for compiled frames and trivially resumable for interpreted ones. Nothing ever resumes *at* a PRIM pc, so PRIM pcs need no re-entry map entries and no re-execution semantics. (`primSnapshot:`'s dual result uses this directly: retire with result `true` — the state the image file captures — then, on the save path, un-retire and return `false` to continue running; the classic idiom, expressed in frame operations.)

### 11. GC Interaction

Restating the consequences of J1/J2 as the GC team's contract:

- The GC never reads, writes, or enumerates machine code or the code cache.
- Compiled frames are scanned as plain frames; no maps, no register scanning.
- The handle table's `method*` references are VM roots (they also keep compiled-but-uninstalled-from-class methods alive; acceptable v1 leak, mirroring the send-site registry).
- Literal access compiles to `method → literals → slot` loads through the frame (three dependent loads). This is the deliberate price of J1. Mitigation hook (v2, measured): dedicate the reserved register to the current method's literals pointer, reloaded at entry/re-entry/stub-return — noted, not built.
- Class indices in cache immediates are stable across GC and snapshot (SPEC.md §17), so patched sites survive collections untouched.

### 12. Stack Growth, Snapshots, Invalidation

**Snapshot**: `primSnapshot:` is reached via a send, so the running process's resume point is a send-return point. On save nothing tier-related is written (J3/J6). On load, `vmState` resets during the loader's method walk; first activations re-tier organically.

**Invalidation** (method install, class install): the existing eager-clear walk (SPEC.md §8) now also patches registered compiled sites to the miss path and, for a *replaced* method, unlinks its handle: `state := unlinked`, `vmState` cleared. **Unlink ≠ free.** Unlinked code is never *re-entered* — entries come only from patched sites (now cleared), `CALL_INTERP` handle checks (now empty), and re-entry maps reached via `vmState` (now cleared) — but it may still be *returned into* and run to completion, because its memory is intact (J7) and old code faithfully implements the old method, which is exactly what classic semantics prescribe for existing activations of a replaced method.

This makes **invalidation triggered from compiled code** safe with no special cases: the install primitive runs at VM-time inside `PRIM_CALL`; it may patch sites in — or unlink the handle of — the very code it will return into. Patched sites take their new path when next executed (J6); the unlinked code finishes its current activations; suspended frames of the old method resume under the interpreter via J4. Frames reference methods, not code, so nothing dangles.

**Reclamation**: unlinked code memory is leaked until a **flush-all** (image-triggered via `primJITControl`, or low-space): all handles unlinked, all sites patched to miss, cache reset to empty. This is where "returned into" must also be excluded, so flush-all requires its caller to be interpreted (the primitive checks; the image calls it from a *do-not-compile* method) — and that requirement is *sufficient* precisely because of J5: with the flush caller interpreted and no native continuations anywhere in the system, every other process is pure heap state, re-enterable only through the tables the flush just cleared. Per-code eviction/LRU is explicitly v2; the code cache reserve (default 64 MB, VM flag) makes "never free" a non-issue for v1 workloads.

---

## Part III — The Compiler (Smalltalk)

### 13. Layering

```
JITCompiler (policy: queue drain, per-method driver)
  └─ MethodCompiler (bytecode walk, labels, metadata assembly)
       └─ JITMacroAssembler (abstract; ~45 coarse operations)
            ├─ AMD64MacroAssembler ── AMD64Assembler ── + AMD64Disassembler
            └─ ARM64MacroAssembler ── ARM64Assembler ── + ARM64Disassembler
```

**Assemblers** (`AMD64Assembler`, `ARM64Assembler`): one method per instruction form (`movRR:with:`, `addImm:to:`, `b:cond:`…), emitting into a growing ByteArray, with label objects and fixup lists (rel32 / ARM64 imm19/imm26). No clever encoders: the subset of each ISA that the templates actually use (~30 instruction forms each) and nothing more. Each ships with a **disassembler** for its subset, round-trip-tested against the assembler (the SPEC.md Phase 1 assembler/disassembler discipline, applied to machine code); the disassembler is the debugging tool (`method jitDisassembly`).

**The macro-assembler is the portability boundary**, and its design rule is: **operations are coarse — whole semantic steps, not synthetic RISC ops.** `genSmallIntAdd:a:b:slow:`, `genInlineCacheSite:receiver:`, `genSafepointPoll:pc:`, `genFrameStores:...`, `genBarrierStore:into:` are single macro ops that each backend implements idiomatically (AMD64 uses flags and `jo`; ARM64 uses `adds`/`b.vs`). Fine-grained abstract ops (abstract `move`, abstract `cmp`) are what make two-target macro assemblers leak; coarse ops keep each backend honest and independently readable. The catalog closes over: the 40 opcodes' fast paths, the frame/call/return sequences, cache sites, polls, barrier, cold-section plumbing. Estimated ~45 operations (Annex J.5 lists them normatively, since patch-site *shapes* — the exact bytes around `PATCH_CLASS`/`PATCH_TARGET` — are frozen per-arch for the VM's patch routines).

**MethodCompiler**: two passes over the u32 instructions. Pass 1 collects jump targets and send-return pcs (label set). Pass 2 emits: per-opcode template dispatch, label binding, cold-section accumulation, and metadata: re-entry map entries at back-edge polls, `returnPoints[siteIndex]` at each send's continuation, patch-site records at each cache. Output: code ByteArray + maps ByteArray (Annex J.4 layouts) → `primJITInstall`.

No IR, no optimizer. The two permitted "optimizations" are peephole-local and template-internal: dead `MOVE` coalescing when a template's result feeds the next template's input within one bytecode pair is *not* attempted in v1 (bytecode is already register-form; the win is small), and constant-materialization choice (`LOADINT` vs `LOADK`) is the bytecode compiler's job, already done.

### 14. Compiler Correctness Discipline

- The compiler is pure: `compile:` reads the method (bytecodes, literals count, sites, header) and Treaty/Annex constants; it allocates only its own output. It never mutates the method — installation is the primitive's job. Compiling the same method twice yields identical bytes (the determinism rule of SPEC.md §18 extends to the JIT — enforced by a test, since it makes golden tests possible at all).
- Every template's *slow path exists and is tested*. The template catalog in Annex J.5 pairs each fast path with its fall-through contract (which stub, which arguments, what the stub may clobber).
- The compiler runs under the same tier rules as everything else. Bootstrapping note: on a fresh image the first compiles run fully interpreted (slow — tens of ms per method); the JIT compiles the hot compiler methods within the first seconds and compile latency drops an order of magnitude. No special casing.

### 15. AMD64 First, ARM64 Second

AMD64 is the development machine; ARM64 is kept honest from day one by three structural rules, then implemented as a discrete milestone (M7):

1. **The macro-assembler interface is written against both ISAs' constraints from the start**: no macro op assumes condition-flag survival across ops, x86-style two-operand destruction, or free 64-bit immediates (ARM64 materializes big immediates in pieces; the patchable class-index compare is designed as a 22-bit immediate precisely so it fits ARM64 `movz/movk`+`cmp` and AMD64 `cmp r,imm32` — Annex J.5 freezes both site shapes).
2. **All Annex constants are per-arch tables**, keyed `amd64`/`arm64`, populated for both from M0 even while only one backend exists.
3. **The differential test suite is arch-blind** (it compares against the interpreter), so bringing up ARM64 is: write `ARM64Assembler` + golden tests, write `ARM64MacroAssembler` (~45 methods), write the two Rust trampolines + patch routines + i-cache flush, run the same suites. No test is written twice.

---

## Part IV — Profiling

Decided scope: **counters + safepoint sampling**, always available, with template-level instrumentation controlled by a global profiling level.

### 16. Counters

- **Invocation counters** (`vmState`): incremented in the interpreter activation path *and* in compiled prologues. Below the tier threshold they drive tiering; above it they keep counting (saturating) as profile data. Image-readable by plain slot access.
- **Inline-cache statistics**: the send-site entry grows a sixth word (Treaty amendment, Annex J.1): a SmallInteger packing saturating hit/miss counts. Misses are counted in the shared Rust miss path (free). Hits are counted by an increment in the compiled send template **only at profiling level ≥ 1**; the interpreter counts hits unconditionally (it's off the fast-path critical budget there).
- **Profiling levels** (VM flag via `primJITControl`): 0 = counters only where free; 1 = cache-hit counting in compiled code; 2 (v1.x, template hook reserved) = basic-block/branch counters. **Changing the level triggers a flush-all + organic recompile** — the live system recompiles itself under new instrumentation in seconds, which is the payoff of an in-image JIT and cheap templates.

### 17. The Safepoint Sampler

Rides the existing timer tick with zero new instrumentation in generated code:

- On a *sampling* tick (sampling enabled via `primJITControl`, rate decoupled from the scheduler tick), the safepoint service routine records into a VM ring buffer: `(process, method, bytecode pc, tier bit, top-K caller methods)` — K settable 0–8; callers come from walking the frame chain, which is plain heap data at a safepoint. Both tiers report **bytecode pcs** (compiled code passes its pc immediate to `SAFEPOINT`, §9), so samples attribute to source lines through the existing `sourceInfo` machinery, tier-independently.
- Buffer half-full signals the Treaty-known **profilerSemaphore**; an in-image profiler process drains via `primReadSamples` into ordinary objects.
- **Known bias, stated**: samples land only at safepoints (sends, back-edges, prologues) — straight-line primitive-heavy code between polls is attributed to its next poll site. For a Smalltalk workload (send-dense) this is minor; the profiler's report marks pc attribution as "at or before."

**In-image tooling** (image code, no further VM support): `JITProfiler` — flat and top-K-caller-tree reports by method/class/line; per-site cache-hit/miss tables (megamorphic-site finder — the direct input for deciding v2 PICs); tier residency (% samples in compiled code — the JIT's own effectiveness meter); compile-queue latency stats from the JIT process. These reports are the acceptance criteria of milestone M5, not afterthoughts.

---

## Part V — VM Support (Exhaustive Inventory)

Everything the Rust VM adds for the JIT. Nothing else is permitted without amending this list.

**Mechanisms**
1. Code cache: one contiguous mmap reserve (default 64 MB — single reservation is also the branch-range guarantee, §8), W^X flipping around installs, guard pages, handle table.
2. Counter-trip path in the interpreter's activation: increment, threshold test, ring-buffer append, semaphore signal (~10 lines).
3. Compilation request ring buffer (GC root).
4. Patch routines: per-arch functions that rewrite `PATCH_CLASS` / `PATCH_TARGET` fields at Annex-frozen site shapes; ARM64 i-cache flush.
5. Send-machinery integration: activation checks `vmState` handle; miss/fill path patches compiled sites alongside heap entries; install-time invalidation walk extended to patch sites and unlink handles.
6. Transition trampolines and in-cache glue (hand-written asm per arch: entry trampoline, exit path, `CALL_INTERP`), glue copied into the code cache at startup (§7); interpreter return path gains the compiled-caller re-entry check (§4).
7. Runtime stubs per Annex J.3 (~16 `extern "C"` functions with the leaf/allocating/exiting classification and exit-disposition ABI, most delegating to existing interpreter internals — lookup, allocator, barrier, unwinder, primitives).
8. `primTransferTo:`/resume path: bytecode-pc → native re-entry via handle maps, interpreter fallback.
9. Sampler: ring buffer, tick hook, frame-walk capture.
10. Loader: reset `vmState` during the existing method walk.

**Primitives** (numbers in Annex J.6): `primJITNextRequest`, `primJITQueueSize`, `primJITInstall:code:maps:`, `primJITControl` (get/set: threshold, profiling level, sampling rate/depth, flush-all, enable/disable JIT), `primReadSamples`, `primJITCodeInfo:` (handle → sizes/counters, for the profiler and disassembler).

**Treaty amendments** (Annex J.1): `vmState` slot on CompiledMethod (slot 7) and CompiledBlock (slot 6); `returnInfo` gains site-index field; send-site entry gains counters word (6 words); special objects 13 `jitSemaphore`, 14 `profilerSemaphore`. **These are Treaty version bumps, not JIT-local tweaks**: they touch the heap writer (entry width, new slots), the loader, the debugger's frame decoding, and the Phase 0 goldens on both sides. They land at M0 as one atomic Treaty change — `treaty.json` bumped, `treaty.rs`/`Treaty.st` regenerated, goldens updated, interpreter and cross-compiler adjusted — *before* any JIT code exists, so the interpreter-only system is re-proven on the amended contract first.

Estimated VM-side total: **1.5–2.5 k lines of Rust + ~200 lines of assembly**, against an estimated 5–7 k lines of Smalltalk for assemblers, templates, compiler, and profiler. That ratio — and the fact that the Rust side contains no instruction selection — is the "minimal VM support" claim, made falsifiable.

---

## Part VI — Testing and Construction Plan

### 18. Test Strategy

The JIT inherits the project's two strongest assets: the **Treaty discipline** (all new binary contracts land in the Annex as machine-readable data first, golden-tested from both sides) and the **corpus** (SPEC.md Phase 3–4), which becomes the differential oracle.

1. **Assembler goldens** (SUnit, in-image): every instruction form → exact bytes; assembler↔disassembler round-trips. CI cross-check: a Rust-side test decodes the same golden blobs with a disassembler crate (capstone) and diffs mnemonics — catches "self-consistent but wrong" encodings, the classic assembler failure.
2. **Per-opcode differential units**: hand-assembled bytecode methods (the Phase 1 fixture assembler, now in-image) covering each opcode's fast path *and every slow-path edge*: ADD overflow → send; AT bounds/format misses; JUMPTRUE non-boolean; SEND through empty/hit/miss/megamorphic caches; DNU; prologue overflow → stack growth mid-compiled-call; NLR to live and dead homes; ensure-unwind through a compiled frame; PRIM failure fall-through. Each runs interpreted and compiled (via `JITCompiler compileNow:` + drain barrier); results and observable heap effects diffed.
3. **Corpus differential modes** (SPEC.md Phase 6): interpreter / JIT-after-N / JIT-always, outputs byte-diffed. Plus mode products: **JIT × GC-stress** (64 KB young space — collections constantly relocate stacks and literals under compiled code; this is J1/J2's trial by fire) and **JIT × snapshot** (snapshot mid-corpus under JIT-always, reload, complete — proves tier-free images and re-entry fallback).
4. **Invalidation battery**: install methods over hot compiled call chains (mono → new target; compiled → replaced-while-on-stack; class install) and assert both correctness and re-tiering.
5. **Scheduler battery**: timer preemption inside compiled hot loops (asserts back-edge re-entry — the no-OSR de-tiering trap of §9), terminate with compiled frames + pending ensures, semaphore ping-pong across tiers.
6. **Determinism test**: compile the entire kernel twice in one session; code + maps byte-identical per method.
7. **Profiler tests**: corpus programs with contrived hot spots; assert the sampler ranks them first and cache stats flag the planted megamorphic site.
8. **Benchmarks as regression tests**: a small fixed suite (send ping-pong, arithmetic loops, collection churn, exception unwind, compiler self-compile time) tracked per commit; targets in §19.

### 19. Milestones

Each milestone ends at a green, committed test suite; later milestones never weaken earlier ones.

- **M0 — Annex + VM groundwork.** Write Annex J.1–J.6 into `treaty.json` (jit section) as one atomic Treaty version bump (Part V); adjust heap writer, loader, debugger decoding, and goldens; re-run the full interpreter corpus on the amended contract before any JIT work. Rust side: code cache, handle table, ring buffer + trip + semaphore, install/control primitives, trampolines + in-cache glue + disposition ABI, stubs, patch routines, resume mapping, sampler buffer, loader reset. Tested **without any Smalltalk-generated code**: Rust tests install hand-written machine-code blobs (fixtures) that exercise entry, a leaf and an exiting stub call, a patched site, back-edge re-entry, cross-tier call and return through the flat loop, and exit. *Exit: a hand-written native method runs inside the full VM, calls an interpreted method and is returned into, survives a forced process switch at its back-edge; all Annex constants golden-tested from both sides.*
- **M1 — AMD64 assembler.** `AMD64Assembler` + disassembler + goldens + capstone cross-check. *Exit: every instruction form the templates need, verified.*
- **M2 — Straight-line templates.** MacroAssembler skeleton + templates for data movement, LOADs, GETIVAR/SETIVAR (with barrier), specialized arithmetic/comparison, JUMP/JUMPTRUE/JUMPFALSE, RET/RETSELF; MethodCompiler passes; `compileNow:` path. *Exit: per-opcode differential units green for this subset; a leaf arithmetic method runs compiled inside the corpus VM.*
- **M3 — Full send machinery.** SEND/SENDSUPER templates, inline caches + VM patching, cross-tier calls and returns, returnPoints, PRIM, MKCLOSURE/CAPTURE/MKBOX/GETBOX/SETBOX, block activation, NLR, safepoint polls + back-edge re-entry, stack growth. *Exit: entire per-opcode battery green; invalidation and scheduler batteries green; whole corpus green under `compileNow:`-everything.*
- **M4 — Background pipeline.** JIT process, tiering policy, drain barrier, flush-all, snapshot interaction, do-not-compile flags. *Exit: corpus green in all three Phase 6 modes and both mode products; kernel self-compiles under load; determinism test green.*
- **M5 — Profiling.** Sampler + primReadSamples + cache counters + profiling levels + `JITProfiler` reports. *Exit: profiler tests green; tier-residency report shows >90% compiled samples on the benchmark suite.*
- **M6 — Performance pass.** Benchmark suite baselined; template tuning within the rules (cold-section layout, poll placement, encoding choices). *Targets: ≥5× interpreter on send/arithmetic microbenchmarks, ≥3× on corpus macro programs, compile speed ≥50 methods/ms-of-compiled-code… concretely: median method compile < 1 ms once the compiler is itself hot.* Misses are analyzed with the M5 profiler — the system explains its own performance.
- **M7 — ARM64.** Per §15: assembler + goldens, macro backend, trampolines/patch/i-cache flush, same suites on ARM64 hardware/CI. *Exit: full suite green on both architectures.*

Sequencing: M0 is the Treaty-style unglamorous prerequisite; M1–M2 can begin against M0's fixtures immediately. The single highest-leverage artifact is M0's **hand-written-blob harness** — it decouples all VM-side risk from all compiler-side risk, exactly as the Phase 1 test assembler did for the interpreter.

### 20. Risks, Accepted Costs, v2 Hooks

| Risk / cost | Position |
|---|---|
| Macro-assembler abstraction leaks across ISAs | Coarse-op rule (§13); ARM64 constraints baked into the interface and Annex from M0; the reckoning is M7 and it is bounded to 45 methods. |
| Miscompiles crash the VM (no verifier) | Differential batteries + GC-stress product + guard pages; disassembler for post-mortems; the interpreter is always available as the oracle (J4). |
| Literal access is 3 dependent loads | Accepted for v1 (J1's price). Hook: reserved register as literals pointer, v2, measured. |
| Sends re-load site data from the heap | Accepted; the patched class compare and direct target keep the critical compare-and-branch tight. Hook: 2-entry PIC in the reserved site slot. |
| Cross-tier transition cost (trampoline round-trip per tier boundary) | The flat-loop model (J5) is the price of heap-only process state — accepted deliberately over recursive nesting, which is *unsound* here (hidden native continuations break suspension/snapshot). Lazy target upgrading and steady-state compilation make transitions rare; M6 benchmarks watch it. |
| Code cache never reclaims per-method | Flush-all only; 64 MB reserve; eviction is v2 with the handle table already shaped for it. |
| Background compile latency / test nondeterminism | High-priority JIT process; drain barrier + `compileNow:` for tests; differential modes never assert timing. |
| Sampler safepoint bias | Documented; send-dense workloads minimize it; exact counters at profiling level 2 are the v1.x escape hatch. |
| No OSR: cold-call hot-loop stays interpreted until next call | Spec-accepted (SPEC.md §19). Back-edge re-entry (§9) removes the *worse* trap (preemption de-tiering); true OSR remains out of scope. |

---

## Annex J — JIT Additions to the Treaty (index)

Normative content lands in `treaty.json` under `"jit"`; this index names the sections to be written at M0:

- **J.1** Object-model amendments: `vmState` packing; `returnInfo` repacking (pc:16 | dest:8 | site:8); 6-word send-site entry with counters word; special objects 13–14.
- **J.2** Per-arch register assignments (§6 table) and scratch/clobber discipline.
- **J.3** Linkage table indices, per-stub signatures, leaf/allocating/exiting classification, exit-disposition ABI.
- **J.4** Metadata ByteArray layouts: re-entry map, returnPoints, patch-site records; handle-table semantics.
- **J.5** Macro-op catalog and the two frozen patchable-site byte shapes per architecture.
- **J.6** New primitive numbers (420–429 block) and `primJITControl` operation codes.

---

## Part VII — Implementation Results (AMD64, M0–M6)

This part records what the implementation actually measures, as opposed to
what Parts I–VI plan. Milestones M0–M6 are done on AMD64 (M7/ARM64 not
started); every milestone's test battery is green (`cargo test` — including
the M0 blob harness, the in-image differential selftest, and the four
Phase 6 corpus modes — and 120 GST SUnit tests). Full detail and the
deviations-from-plan list live in `docs/jit-status.md`.

### 21. Microbenchmarks — where the template JIT wins

Release build, median of 5, warmed (`bench/run_jit.sh`), interpreter vs
JIT on the same workload with bit-identical output:

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

Compile speed: **~0.6 ms/method** with the compiler hot (M6 target < 1 ms).

The JIT wins decisively where code **stays native across a tight loop over
SmallInteger/array-typed data** (send, arithmetic, string building). The
gains shrink toward 1× as the workload becomes send-dense over
heterogeneous object types (dictionary), and go *below* 1× when it is
switch-dominated on a microsecond scale (process_pingpong pays a
trampoline round-trip per compiled `wait`/`signal` that the interpreter
does not).

### 22. Self-compile — the JIT is ~18× *slower* here

The headline macro workload — the compiler compiling itself — is the
template JIT's **worst case**, and this is the most important measured
result in the whole implementation:

| | time | notes |
|---|---|---|
| interpreter | **850 ms** | one self-compile pass, fresh |
| fully JIT-warmed | **~15,000 ms** | 4 warmup passes, tiering then disabled so the timed pass carries no compile cost; gen2 image bit-identical; 881 methods installed, zero request drops |

That is an ~18× **regression**. It is not thrash and not a measurement
artifact — it is the model meeting a workload it is structurally bad at.
The cause, established by per-activation and per-site counters (not
guessed):

1. **"Fully JIT-warmed" is a misnomer — only 881 methods ever compile.**
   Every activation counts toward tiering, but only 881 distinct methods
   reach the threshold; the remainder are blocks, primitive-bodied
   methods, and methods run too few times, none of which become native.
2. **83% of activations run interpreted anyway** (18.4M interpreted vs
   3.7M native in the timed pass). The compiled 881 are a *minority* of
   what executes: the system is mostly interpreting, with JIT machinery
   layered on top.
3. **Stale call-site routing is NOT the cause.** When a site was wired,
   the callee was already compiled 99.9% of the time (5.4M direct vs 3.5K
   through the CALL_INTERP glue), so the deferred lazy-upgrade (§8) is
   negligible on this workload. (An earlier hypothesis blamed this; the
   counters refute it. It is recorded here because the wrong answer is
   instructive: cross-tier *coverage*, not cross-tier *routing*, is what
   hurts.)
4. **The dominant per-crossing cost is specialized sends on
   non-fast-path types.** The compiler is comparison/accessor-dense over
   heterogeneous objects: ~2.9M `=` fall-throughs, plus `at:`/`size`/
   `at:put:` on non-array receivers. Each compiles to an EQNUM/AT/…
   template whose fast path is for SmallIntegers/arrays, misses, and
   takes SEND_SLOW → exit trampoline → the interpreter runs the real
   method → RESUME → re-enter — a full round-trip per operation, far
   costlier than the interpreter's single inline `spec_slow`. The
   template JIT's specialization is tuned for SmallInteger arithmetic; on
   symbol/string/node comparisons it is pure overhead.

So the flat-loop cross-tier cost (§20's explicitly-accepted risk) is real,
but the dominant contributor is specialized-send slow paths bouncing
through the interpreter, compounded by 83% of activations never being
compiled at all.

**The levers are v2 coverage work, not template tuning:** (1) synthesize
real inline-cache send sites for specialized-send slow paths, so a
repeated `=` on symbols caches its target and stays native (the PIC hook
in §20); (2) compile the transitive callee set so boundaries disappear;
(3) lazy CALL_INTERP upgrade. Until those land, the JIT should be applied
selectively to hot numeric/array loops, **not** blanket-enabled over a
broad, polymorphic workload — for which the interpreter is markedly
faster.

### 23. Tuning fixes made while measuring

Diagnosing §22 surfaced four correctness/tuning bugs in the tiering path,
all fixed (and correct independent of performance):

- The tiering trip tested `counter == threshold`, so a request dropped on
  queue overflow never re-tripped and its method stayed interpreted
  forever — now `>=`.
- A higher-priority wake (the background compiler) did not arm the
  safepoint, so a CPU-bound process could starve it indefinitely — the
  internal-signal path now arms the safepoint when it wakes a
  higher-priority waiter.
- `drain` chased its own tail forever: compiling a method activates
  compiler methods that themselves trip and re-enqueue — now it compiles
  a bounded snapshot (the queue size at entry) and lets the next signal
  pick up the rest.
- The compilation request queue was **256 entries** — so small that under
  aggressive tiering 99.98% of requests were dropped, leaving a
  near-random handful of methods compiled — now 64K (`JIT_QUEUE_CAPACITY`).

Latent VM bugs the JIT batteries flushed out earlier (GC compaction cursor
drift, large-object routing under tiny nurseries, send-arm safepoint
preemption) are listed in `docs/jit-status.md`.
