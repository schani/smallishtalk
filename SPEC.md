# The Smalltalk VM Specification

**Version 0.1 — Design Specification**

This document specifies a Smalltalk virtual machine designed around two goals: a fast, simple interpreter, and bytecode that compiles directly to efficient AMD64/ARM64 machine code via a template JIT. It is a classic *live system*: image-based, with green-thread processes, resumable exceptions, and an in-image debugger as first-class architectural concerns.

The design can be summarized as "Spur minus its three hardest features": no `become:`, no reified contexts, no hybrid fixed+indexable objects. In exchange it adds a register/frame-slot bytecode (Lua-style) instead of a stack bytecode, and a single hard invariant governing everything that touches execution state:

> **The Stack Invariant.** No raw stack address is ever stored in a heap object. All references into a stack are `(process, offset, serial)` triples. A process's stack may be moved or grown only while that process is not running, with one sanctioned exception: a running process may grow its own stack at its own safepoint. The interpreter/JIT may cache the running process's stack base in a machine register between safepoints of that same process.

Everything else in this document is either a consequence of this invariant or independent of it.

This specification is the source document for two codebases that must agree on bit layouts:

1. **The VM**, written in Rust: interpreter, GC, primitives, snapshot loader, later a template JIT.
2. **The compiler**, written in portable Smalltalk: parser, register-bytecode generator, and heap/image writer. It runs first under GNU Smalltalk (cross-compiling) and later self-hosts in the image.

The binary contract between them — tag values, header layouts, opcodes, image format — is collected in Appendix A ("the Treaty") and mirrored as machine-readable constant files consumed by both test suites.

---

## Part I — The Object Model

### 1. Values and Tagging

All Smalltalk values are 64-bit words, in one of two (eventually three) forms, distinguished by low-order tag bits:

| Low bits | Meaning |
|---|---|
| `...1` | **SmallInteger** immediate. Value is `word >> 1` (arithmetic shift). 63-bit signed range: −2⁶² … 2⁶²−1. |
| `...000` | **Object pointer**, 8-byte aligned address of an object header. |
| `...010` | *Reserved* for immediate Float (v2+; not used in v1). |
| `...100`, `...110` | *Reserved*, never valid. |

Design notes:

- A single tag bit for SmallInteger makes the integer fast path minimal: addition is `a + b − 1` with overflow check; equality, ordering, and hashing operate on raw words without untagging.
- **`nil`, `true`, and `false` are ordinary heap objects** at known fixed addresses (they live at fixed offsets at the start of old space and are listed in the special objects array, §17). They are *not* immediates. Consequence: every non-SmallInteger value is a pointer from which a header can be loaded, so dispatch and inline-cache checks never need extra cases. `x == nil` remains a single compare against a known constant address.
- No immediate Characters. Characters are ordinary objects in v1 (and rarely on hot paths given UTF-8 strings, §15).

SmallInteger tagging/untagging:

```
tag(n)   = (n << 1) | 1        ; valid iff n fits in 63 bits
untag(w) = w >> 1              ; arithmetic shift
isInt(w) = w & 1
isPtr(w) = (w & 7) == 0
```

### 2. Object Header

Every heap object begins with one 64-bit header word:

```
 bits 63..42   classIndex     (22 bits)
 bits 41..20   identityHash   (22 bits)
 bits 19..12   numSlots       (8 bits)
 bits 11..8    format         (4 bits)
 bits  7..0    gcBits         (8 bits)
```

**classIndex** (22 bits, up to ~4.2M classes). An index into the global class table (§4), *not* a pointer. This is the load-bearing decision of the object model:

- Inline caches and dispatch compare a 22-bit immediate against a field of the header — one load, one masked compare, no pointer chase to a class object.
- Headers contain no pointers, so GC never rewrites headers when classes move, and image loading needs no header fixups (§17).
- The class *object* is reached via `classTable[classIndex]` only on the slow path (lookup misses, reflection).

**identityHash** (22 bits). Lazily assigned: objects are born with hash 0 (meaning "unassigned"); the first `identityHash` send assigns the next value from a per-VM counter (wrapping, skipping 0) via a primitive that mutates the header. Decoupling hash from address is what makes moving GC safe. SmallIntegers hash as their value; the primitive special-cases them.

**numSlots** (8 bits). The object's body size in 8-byte slots, 0–254. The value 255 is an escape: the true size is a raw 64-bit slot count stored in one **overflow word immediately preceding the header**. Objects with ≥255 slots therefore cost 16 bytes of overhead instead of 8. An object's total footprint is `8*(numSlots) + 8` bytes (+8 for the overflow word when present), always 8-byte aligned.

**format** (4 bits) — see §3.

**gcBits** (8 bits). Owned by the GC: mark bit, remembered bit (object is in the sequential-store buffer), pinned bit (reserved), generation/age bits. The mutator never reads or writes these except through the write barrier.

### 3. Object Formats

Exactly three formats. There are deliberately **no hybrid fixed+indexable objects** — the classic Smalltalk feature exists mainly for `CompiledMethod`, which is here an ordinary fixed object (§9) holding pointers to a literals `Array` and a bytecodes `ByteArray`.

| format | Name | Body |
|---|---|---|
| `0` | **Fixed** | `numSlots` named instance variables, all tagged words. The *semantic* slot count is defined by the class; the header's numSlots exists so the GC can scan without consulting the class. |
| `1` | **Indexable pointers** | `numSlots` tagged words, 1-indexed at the language level (`Array`, `OrderedCollection`'s store, method dictionaries' arrays, …). |
| `8`–`15` | **Indexable bytes** | Raw bytes, padded to a slot boundary. `pad = format − 8` (0–7); `byteSize = numSlots*8 − pad`. (`ByteArray`, `String`, `LargeInteger` magnitudes, `Float`'s 8 bytes.) |

Formats 2–7 are reserved and invalid in v1.

Consequences:

- The GC scans an object by reading only its header: format 0/1 → scan `numSlots` words; formats 8–15 → scan nothing.
- `at:`/`at:put:` have exactly one shape per format: load header, extract numSlots (and pad for bytes), bounds-check, indexed load/store. Four to six machine instructions, and the header load is shared with the inline-cache check that preceded it (§8).
- `Float` is a byte-format object (format 8, numSlots 1) with a known class index; no fourth format is needed.
- Variable-pointer classes with named ivars (e.g. classic `MethodContext`) do not exist; any class that wants both holds an Array in an ivar.

### 4. The Class Table and Classes

A VM-global table maps `classIndex → class object pointer`. Entry 0 is invalid. A contiguous low range of indices is **fixed by the Treaty** (Appendix A) and known to compiled code: SmallInteger, Float, ByteString, Symbol, Array, ByteArray, BlockClosure, CompiledMethod, CompiledBlock, Box, Process, Semaphore, MethodDictionary, the boolean/nil classes, and the metaclass kernel. SmallInteger's class index is what arithmetic fast paths and inline caches use for integer receivers (checked via the tag bit, not a header load).

Classes are ordinary fixed-format objects. The VM knows (by Treaty-fixed slot indices) these instance variables of `Behavior`:

```
slot 0: superclass          (class or nil)
slot 1: methodDictionary    (MethodDictionary)
slot 2: formatAndSlots      (SmallInteger encoding: instance format in high bits,
                             named-slot count in low bits)
slot 3: classIndex          (SmallInteger; the class's own index, for fast instantiation)
```

Subclasses of `Behavior` (`Class`, `Metaclass`) add name, class variables, etc. — invisible to the VM. The metaclass tower is fully classic: `x class` reads the receiver's header classIndex through the class table; `Metaclass`'s instances are per-class as usual. Instantiation (`new`, `new:`) is a primitive that reads `formatAndSlots` and allocates.

Class table management: the image allocates class indices via a primitive (`registerClass:`), which installs into the table and stamps slot 3. The table itself is VM-private memory, rebuilt from the image's class-list on load (§17). Freed indices (class GC) are out of scope for v1 — indices are never recycled.

### 5. Special Objects

A Treaty-fixed **special objects array** gives the VM and the image shared access to: `nil`, `true`, `false`, `Processor` (the ProcessorScheduler), the class table's image-side list, the selector table for specialized sends (§8), the signal-routing objects (`doesNotUnderstand:` selector, `mustBeBoolean` handler, low-space semaphore, the unwind-protect marker), and `Smalltalk` (the system dictionary). Its layout is Appendix A.4.

---

## Part II — Execution

### 6. Bytecode: Register / Frame-Slot Design

Methods compile to fixed-width **32-bit instructions** operating on **slots** of the current frame — no operand stack. A frame holds a fixed number of slots determined per-method by the compiler; arguments and temporaries occupy the low slots, scratch slots above them. Slot lifetimes are decided at compile time (for tree-shaped Smalltalk expressions, slot allocation is essentially a depth counter).

Why this design: the bytecode's data flow is explicit, so a template JIT translates each instruction independently — `slot[i]` is `[FP + 8*i]` — with no symbolic stack tracking. The bytecode is the IR.

**Instruction encodings** (two shapes):

```
ABC:  [ opcode:8 | A:8 | B:8 | C:8 ]
AD:   [ opcode:8 | A:8 |   D:16    ]     ; D unsigned, or signed for jumps
```

A/B/C are slot numbers (so a frame has ≤ 255 slots — ample; the compiler rejects pathological methods), literal indices for small indices, or counts. D is a 16-bit literal index, jump offset (signed, in instructions, relative to the *next* instruction), or immediate.

**Core instruction set** (full opcode table with numbers: Appendix A.2):

*Data movement*

| Mnemonic | Encoding | Semantics |
|---|---|---|
| `MOVE d, a` | ABC | `slot[d] := slot[a]` |
| `LOADK d, k` | AD | `slot[d] := literals[k]` |
| `LOADINT d, imm` | AD | `slot[d] := tagged(signed imm)` (small constants without literal-pool traffic) |
| `LOADNIL d` / `LOADTRUE d` / `LOADFALSE d` | AD | known constants |
| `LOADSELF d` | AD | `slot[d] := receiver` (the receiver is bytecode slot 0, see §7; this is a MOVE alias kept for compiler clarity) |
| `GETIVAR d, i` | ABC | `slot[d] := receiver.ivar[i]` |
| `SETIVAR i, a` | ABC | `receiver.ivar[i] := slot[a]` (+ write barrier) |
| `GETBOX d, a` | ABC | `slot[d] := slot[a].value` — read through a Box (§10) |
| `SETBOX a, b` | ABC | `slot[a].value := slot[b]` (+ write barrier) |
| `MKBOX d, a` | ABC | allocate Box containing `slot[a]`, store pointer in `slot[d]` |

*Control*

| Mnemonic | Encoding | Semantics |
|---|---|---|
| `JUMP off` | AD | unconditional, signed offset |
| `JUMPTRUE a, off` / `JUMPFALSE a, off` | AD(A=a) | branch if `slot[a]` is `true`/`false`; **any other value** (including non-booleans) triggers the `mustBeBoolean` send (slow path). Backward jumps poll the safepoint flag (§13). |
| `RET a` | AD | return `slot[a]` to caller's destination slot; pop frame |
| `RETSELF` | AD | return receiver (so common it pays for itself) |
| `NLR a` | AD | non-local return of `slot[a]` from a block to its home's caller (§10) |

*Sends*

| Mnemonic | Encoding | Semantics |
|---|---|---|
| `SEND d, r, s` | ABC | send: receiver in `slot[r]`, args in `slot[r+1] … slot[r+argc]`; result to `slot[d]`. `s` is an index into the method's **send-site table** (literal-adjacent array holding selector, argc, and the inline cache pair). |
| `SENDSUPER d, r, s` | ABC | as SEND, but lookup starts above the *static* defining class, recorded in the send-site entry. |
| `MKCLOSURE d, b` | AD | allocate BlockClosure for CompiledBlock `literals[b]`; subsequent `CAPTURE` instructions fill it (§10). |
| `CAPTURE c, a` | ABC | `closure-under-construction.captured[c] := slot[a]` (compiler emits these immediately after MKCLOSURE; `d` of MKCLOSURE holds the closure). |
| `PRIM n` | AD | only as first instruction of a method body: invoke numbered primitive; on success, returns; on failure, falls through to the Smalltalk body (§16). |

*Specialized sends* (§8): `ADD, SUB, MUL, DIV, MOD, LT, GT, LE, GE, EQNUM, IDEQ, AT, ATPUT, SIZE, CLASSOF, NOT` — all ABC-encoded `op d, a, b` (unused field zero), each semantically a SEND with an inlined fast path.

Method size is bounded: ≤ 2¹⁶ instructions, ≤ 2¹⁶ literals, ≤ 255 send sites and ≤ 255 slots per frame. The compiler enforces these; real methods don't approach them.

### 7. Frames, Stacks, and Processes

A **stack** is an ordinary pointer-format heap object owned by exactly one Process. All its words are tagged values; it is scanned exactly by the GC and may move *only* per the Stack Invariant.

A **frame** within a stack:

```
slot 0: callerFrameOffset   (SmallInteger; 0 = base frame)
slot 1: returnInfo          (SmallInteger packing caller's resume PC (bytecode
                             offset, not pointer) and caller dest slot)
slot 2: method              (CompiledMethod or CompiledBlock being executed)
slot 3: flagsAndSerial      (SmallInteger: bit 0 handler-installed, bit 1 has-ensure,
                             bit 2 closure-context (frame is a block activation),
                             bits 32.. frame serial)
slot 4: receiver
slot 5…: arguments, then temporaries, then scratch slots
```

Five words of fixed overhead per activation. Bytecode slot numbers are relative to slot 4 (so bytecode `slot[0]` = receiver, `slot[1]` = first argument…). Frames marked as handlers reserve two additional fixed slots after the temps for the handler's exception-class and handler-block (§11), at offsets recorded in the method.

**PCs are stored as bytecode offsets**, never machine or interior pointers — frames are position-independent, which is what makes stacks movable, snapshotable, and debugger-readable as plain data.

**Frame serials.** A per-process 32-bit counter, incremented on every send, stamped into `flagsAndSerial`. A `(process, offset, serial)` triple identifies a *specific activation*: if the frame at `offset` no longer carries `serial` (or the offset is beyond the live top, or the process's stack is nil), the activation is dead. This is the liveness test for non-local return and debugger references. Serial wraparound after 2³² sends within one process is accepted (an NLR surviving 4 billion sends in its home process, landing on a frame with the colliding serial at the same offset, is beyond-astronomically unlikely; the spec notes it as a known theoretical hole).

**Calling convention — overlapping frames.** The compiler places receiver and arguments contiguously in the caller's scratch area, reserving four slots immediately before the receiver position. A send writes the four control words (slots 0–3) into that reserved gap, and the callee's frame begins there — its slot 4 (receiver) *is* the caller's receiver slot. No argument is ever copied. The callee's prologue checks `frameOffset + method.frameSlots ≤ stackLimit` — this single compare is both the stack-growth trigger and the overflow check.

**Stack growth.** On prologue overflow: allocate a stack object twice the size (via a leaf path that must not itself fail — growth allocation goes directly to old space if young space can't satisfy it), memcpy, update the Process's stack slot, re-derive the cached base register, continue. Sanctioned by the Invariant since the running process moves its own stack at its own safepoint. Initial stack: 4KB object; maximum: image-settable limit, default 16MB, beyond which `signal: StackOverflow` (delivered on the same stack's remaining headroom — the limit check keeps one emergency frame's worth in reserve).

**The Process object** (fixed format, Treaty slots):

```
slot 0: stack            (pointer-format stack object, or nil if terminated)
slot 1: frameOffset      (SmallInteger; current/resume frame)
slot 2: pc               (SmallInteger; resume bytecode offset, valid when suspended)
slot 3: priority         (SmallInteger 1..numPriorities)
slot 4: nextLink         (Process or nil — scheduler/semaphore queue link)
slot 5: myList           (the queue currently holding this process, or nil)
slot 6: serialCounter    (SmallInteger)
... image-defined slots follow (name, etc.) — invisible to the VM.
```

A process is *running* (on the CPU; its pc/frameOffset slots are stale, live state is in VM registers), *runnable* (on a run queue), *blocked* (on a semaphore's queue), *suspended* (on no queue), or *terminated* (stack is nil). Only the running process's stack is pinned; all others are ordinary movable objects.

### 8. Sends, Lookup, and Inline Caches

Every send site in a method has an entry in the method's **send-site table** (an Array referenced by the CompiledMethod, parallel to but distinct from the literal Array so that caches are mutable without touching literals):

```
entry: [ selector (Symbol) | argc (SmallInteger) | cacheClass (SmallInteger classIndex, 0 = empty)
       | cacheMethod (CompiledMethod or nil) | staticClass (for SENDSUPER, else nil) ]
```

**Send fast path** (interpreter and JIT share it structurally):

1. Load receiver; if tagged int → classIndex := SmallInteger's (constant); else load header, extract classIndex.
2. Compare against `cacheClass`. Hit → activate `cacheMethod`.
3. Miss → global lookup cache: a VM-private open-addressed table of 4096 entries keyed by `(classIndex, selector)`. Hit → refill inline cache, activate.
4. Miss → walk `classTable[classIndex]`'s method dictionaries up the superclass chain (§14). Found → fill both caches, activate.
5. Not found → re-dispatch as `doesNotUnderstand:` with a Message object (allocated on this slow path only).

Activation: check primitive number (run primitive, §16), else push frame and enter the method.

**Invalidation.** Installing a method (the `MethodDictionary at:put:` primitive) and `registerClass:` flush the global lookup cache entirely and **eagerly clear all inline caches** by walking a VM-maintained registry of send-site tables (every CompiledMethod registers its table at installation; the registry is VM-private, rebuilt at image load). The alternative — epoch counters checked on the cache-hit path — is rejected: it adds a compare to every send's fast path to optimize the rare case. With eager clearing, method installs pay a linear walk (rare, fast) and the hit path stays one load + one compare. JIT-compiled call sites use the same registry: compiled methods register their patchable sites, and install-time clearing patches them back to the miss stub.

**Megamorphic sites** are simply sites that miss often; they fall to the global cache every time, which is the designed behavior — no polymorphic inline caches in v1 (the JIT may add 2-entry PICs later without spec changes; the send-site entry reserves one spare slot for it).

### 9. CompiledMethod and CompiledBlock

Both are **fixed-format** objects (no hybrid format exists):

```
CompiledMethod:
slot 0: header        (SmallInteger packing: frameSlots:8, argc:4, primitive:12,
                       hasPrimitive:1, handlerSlotBase:8, flags)
slot 1: bytecodes     (ByteArray; u32 little-endian instructions)
slot 2: literals      (Array)
slot 3: sendSites     (Array; §8 entries, 5 words each, flattened)
slot 4: selector      (Symbol; for the debugger)
slot 5: methodClass   (class in which installed; needed for SENDSUPER's static class
                       and the debugger)
slot 6: sourceInfo    (image-defined: source pointer/map; nil ok; invisible to VM)

CompiledBlock: identical layout (slots 0–3 as above), plus
slot 4: outerMethod   (the enclosing CompiledMethod/CompiledBlock)
slot 5: blockInfo     (SmallInteger: numCaptured:8, hasNLR:1, ...)
```

The bytecodes ByteArray is immutable after installation (the image-side compiler builds, then installs; installation sets a header flag the `at:put:` primitive respects). Literals are likewise frozen; send-site arrays are VM-mutable (caches).

### 10. Closures, Capture, and Non-Local Return

**Capture-by-value with explicit boxes, decided at compile time.** For each variable referenced by an enclosing scope *and* a block, the compiler classifies it:

- Read-only after capture (assigned nowhere after block creation, including in the block) → **copied** into the closure at MKCLOSURE time.
- Mutated by anyone after capture → **boxed**: the variable's home slot holds a pointer to a 1-slot `Box` object (allocated by `MKBOX` at variable initialization); both the method and the closure reference the Box; reads/writes go through `GETBOX`/`SETBOX`.

The receiver (`self`) and any used enclosing args/temps are captured by these same rules (self is always copy — it's immutable). There is no frame pointer in a closure for variable access, ever.

**BlockClosure** (fixed format):

```
slot 0: compiledBlock   (CompiledBlock)
slot 1: homeProcess     (Process or nil)        \
slot 2: homeOffset      (SmallInteger)           > only if blockInfo.hasNLR,
slot 3: homeSerial      (SmallInteger)          /  else nil/0
slot 4…: captured values and boxes (count = blockInfo.numCaptured)
```

Blocks without `^` carry nil home fields and never touch the process — they are pure values. **Clean blocks** (no captures, no NLR, no self-reference) *may* be allocated once and reused as a per-CompiledBlock singleton; the spec permits but does not require this.

**Activation**: `value`, `value:`, … are primitives on BlockClosure that push a frame whose `method` slot holds the CompiledBlock and whose receiver slot holds the *closure itself*; bytecode in blocks accesses captured values via `GETIVAR`-style indexing into the closure (the compiler emits `GETIVAR d, 4+c` against the closure-as-receiver — same opcode, no special block instructions needed) and the original receiver via captured slot 0 by convention when self is used. Argument-count mismatch fails the primitive → in-image `wrongNumberOfArguments` signal.

**Non-local return** (`NLR a`): using the closure's `(homeProcess, homeOffset, homeSerial)`:

1. If `homeProcess` ≠ the current process, or its stack is nil, or the frame at `homeOffset` doesn't carry `homeSerial` → signal `BlockCannotReturn` (in-image exception; resumable so a debugger can intervene).
2. Otherwise **unwind** (§11's shared unwinder) from the current frame down to `homeOffset`, running pending `ensure:` blocks, then return `slot[a]` *from the home frame to its caller* exactly as if the home method executed `RET`.

NLR from a closure whose home lives in *another* process is always an error (matches classic semantics — a block evaluated in a forked process cannot return through it).

### 11. Exceptions

Full classic resumable semantics. Almost everything is in-image code; the VM provides exactly three things: the handler-frame flag, the frame walk, and the unwinder.

**`on:do:`** is an ordinary method whose frame is special only in that: (a) its handler bit is set in flagsAndSerial, (b) two reserved frame slots (at `handlerSlotBase` from the method header) hold the exception class (or ExceptionSet) and the handler block, (c) a third reserved slot holds handler state (armed / in-progress) to give correct semantics for exceptions raised *within* a handler (the search must skip in-progress handlers).

**Signaling** (`Exception>>signal`, in-image, on top of two primitives):

- `primFindHandler: exceptionClass from: frameOffset` — walk caller links from `frameOffset`, return the offset of the nearest *armed* handler frame whose stored class `handles:` the exception (the class test itself is a send back into the image — the primitive yields candidate frames; the in-image loop applies `handles:`. Division of labor: VM walks, image decides).
- The handler block then runs **in a new frame on top of the signaling frame** — the signaling frames stay live underneath. The exception object carries `(process, signalOffset, signalSerial)` and `(handlerOffset, handlerSerial)`.

**Outcomes** (in-image methods over the unwinder primitive):

- `resume: v` — unwind only the frames *above* the signal frame (the handler activation itself), then return `v` as the result of the `signal` send. Legal only for resumable exceptions; the signal-frame triple validates liveness.
- `return: v` — unwind to the handler frame; `on:do:` returns `v`.
- `retry` / `pass` / `outer` — in-image combinations of the above plus re-signaling; no further VM support.
- Falling off the handler block = `return:` its value.

**The unwinder** — one primitive, two clients (exceptions and NLR):

`primUnwindTo: targetOffset` pops frames from the top down to (not including) `targetOffset`. For each popped frame with the has-ensure bit: instead of popping past it, the VM *returns control to the image* by activating that frame's ensure-block with a continuation marker, and the in-image unwind loop re-invokes the primitive afterward. (Mechanically: `ensure:`/`ifCurtailed:` are methods whose frames carry the bit and whose receiver-block and argument-block live in reserved slots, mirroring the handler scheme.) The unwinder is therefore re-entrant and interruptible — an ensure block can itself signal, NLR, or never return, and semantics remain well-defined because unwinding state lives in frames, not VM globals.

**Termination interacts here** (the classic bug, designed out): `Process>>terminate` does **not** free the stack. It makes the target process runnable with a pending *unwind-everything order* (pc redirected to a Treaty-known `terminate` trampoline in the image): the process resumes, runs `primUnwindTo: base` executing all its ensure blocks *in its own context*, then sets its stack slot to nil and yields forever. A process only ever unwinds itself.

### 12. Specialized Sends (Inlined Primitives)

Each of these opcodes is *semantically a SEND* of the corresponding selector; the fast path is an inlined check-and-execute, and any check failure falls into the ordinary send machinery (same send-site table entry, so the slow path is indistinguishable from a real send — Floats, Fractions, user classes all work, they just pay send cost).

| Opcode | Selector | Fast path condition | Fast path action |
|---|---|---|---|
| `ADD/SUB d,a,b` | `+` `-` | both SmallInt, no overflow | tagged add/sub (`a+b−1`, `a−b+1`), `jo → slow` |
| `MUL d,a,b` | `*` | both SmallInt, no overflow | untag one, `imul`, overflow check |
| `DIV/MOD d,a,b` | `//` `\\` | both SmallInt, b≠0 | **floored** division (sign fixup after machine div) |
| `LT/GT/LE/GE d,a,b` | `<` etc. | both SmallInt | raw tagged-word compare → LOADTRUE/FALSE |
| `EQNUM d,a,b` | `=` | both SmallInt | raw compare |
| `IDEQ d,a,b` | `==` | always (no slow path) | raw word compare — identity is universal |
| `AT d,a,b` | `at:` | a is ptr, format 1 or 8–15, b SmallInt in 1..size | indexed load (bytes load as SmallInt) |
| `ATPUT d,a,b` | `at:put:` | as AT, plus format 1 → barrier; immutable flag clear | indexed store; d := stored value |
| `SIZE d,a` | `size` | a is ptr, format 1 or 8–15 | numSlots/byteSize as SmallInt |
| `CLASSOF d,a` | `class` | always | classTable[classIndex(a)] |
| `NOT d,a` | `not` | a is true/false | constant-compare flip |

(ATPUT's three-operand limit: the value comes in `slot[b+1]` by convention — receiver `a`, index `b`, value `b+1`, contiguous like a send's args, which is exactly what the slow path needs anyway.)

The compiler emits these opcodes for the corresponding selectors **unconditionally** — which is sound because the Treaty declares the SmallInteger/Float arithmetic methods and the format-generic `at:`/`at:put:`/`size` on the built-in receiver classes **sealed**: the image refuses recompilation of those specific methods (enforced in-image; the VM additionally ignores overriding installs on SmallInteger for these selectors). User classes overriding `+`, `at:` etc. for *their own* instances work normally — they're just always on the slow path.

`ifTrue:`/`ifFalse:`/`and:`/`or:`/`whileTrue:` are **compiled to jumps** (JUMPTRUE/JUMPFALSE) when their arguments are literal blocks — the classic macro-expansion, with the `mustBeBoolean` slow path preserving message semantics for non-boolean receivers. With non-literal-block arguments they compile as real sends.

### 13. Processes, Scheduler, Safepoints

**Scheduler structure is image-visible** (live-system ethos: the scheduler is debuggable from inside). `ProcessorScheduler` holds an Array of run queues (linked via Process `nextLink`/`myList`), one per priority (default 8 levels), plus `activeProcess`. The VM reads/writes these objects directly via Treaty slot indices.

**VM primitives** (the *only* VM-side scheduling logic):

- `primTransferTo: aProcess` — the context switch: store pc/frameOffset into the current Process, clear its running pin; load target's, re-derive stack-base and frame registers, mark running. (Both stacks may be moved by GC before/after but not during — the switch is a safepoint.)
- `Semaphore>>wait` / `signal` — Semaphore is fixed-format: `excessSignals`, queue head/tail. `wait` with no excess signals enqueues the active process and transfers to the highest-priority runnable; `signal` with a waiting process makes it runnable (preempting if higher priority). Implemented as primitives because they must be atomic w.r.t. preemption.
- `primYield`, `Process>>suspend`/`resume` — queue manipulation + possible transfer.

**Safepoints.** One poll site, three consumers. The interpreter checks a single VM flag word at: every send (in the activation path) and every backward jump. The flag is set by: the timer tick (host-side, for preemption — checks for higher-priority runnable processes and for expired Delays via the image's timer semaphore), the GC (allocation failure requests a collection at the next safepoint — though in practice the allocator *is* at a safepoint, see §14), and external events (host event loop signaling input semaphores). At a safepoint with the flag set, the VM stores pc/frameOffset (making the process suspendable/scannable), services the request, and continues or switches. The JIT compiles the identical check (one load-test-branch on a dedicated flag address).

**v1 ships with the full mechanism but may run a single process**; `wait` on a semaphore nobody will signal raises a deadlock error in that configuration. The scheduler run-queue code (~200 lines) activates when the timer and event sources come online. Nothing in the object layout changes.

**Delays and I/O** follow classic Smalltalk: `Delay` is image code over a VM-maintained "signal this semaphore at time T" list (one primitive: `primSignal:atMilliseconds:`); blocking I/O is image code that issues a non-blocking host primitive and waits on a semaphore the host event loop signals. The VM never blocks the OS thread while Smalltalk processes are runnable.

---

## Part III — Memory and the System

### 14. Garbage Collection

Generational, exact, moving. All roots are exact: stacks contain only tagged words; the VM's own references during execution are confined to a small, enumerable root set (current process, special objects array, class table, the send-site registry, in-flight primitive temporaries via a handle scope).

**Young generation**: copying semispace, default 8MB per space, bump allocation (allocation is a pointer increment + limit check; the limit check failing *is* the GC trigger, and since allocation only happens in primitives/instruction implementations that are at well-defined points, every allocation site is a safepoint — pc/frameOffset are storable on demand). Objects surviving N scavenges (age bits in gcBits, N=4 default) are tenured to old space. Large objects (> 64KB body) allocate directly in old space.

**Old generation**: mark-compact, sliding (Lisp-2 style: mark; compute forwarding addresses into a side table or via the classic threading scheme — v1 uses a forwarding side table, simpler than threading and headers stay untouched until the fixup pass); preserves allocation order. Headers contain no pointers (class *indices*), so compaction rewrites only object slots, stack-object contents, and the root set. Triggered by tenure pressure or old-space exhaustion; runs at a safepoint, stop-the-world.

**Write barrier** (mutator side, emitted by SETIVAR/ATPUT/SETBOX and the corresponding primitives): on storing a *pointer to a young object* into an *old object*, if the old object's remembered bit is clear: set it, append the old object's address to the **sequential-store buffer** (SSB). Scavenge drains the SSB as roots and clears bits for objects that no longer point young. The filter (young-pointer-into-old) is two compares against the young-space bounds; the common store (into young, or storing an int) takes the first branch out.

**Stacks and GC**: suspended processes' stacks are ordinary objects — scanned, moved, compacted like anything else (their frames are position-independent: offsets and bytecode PCs only). The running process's stack is GC-scanned via its known location and *pinned for the duration of that collection* (the one pinned object; alternatively the post-safepoint path re-derives the base register, permitted by the Invariant — v1 pins, the simpler rule). The interpreter's cached registers are re-derived after any safepoint that ran a GC.

**Finalization & weakness**: out of scope for v1 except the minimum the system needs: none. (Symbols intern in a strong set, §15; weak arrays, ephemerons, and the finalization process are a v2 design item — the gcBits reserve a bit for it.)

### 15. Numbers and Strings

**SmallInteger**: §1. Overflow in v1 signals `ArithmeticOverflow` (in-image, from the primitive fallback code of `SmallInteger>>+` etc.).
**Float**: boxed; byte-format object, 8 bytes, IEEE 754 binary64, Treaty class index. Full arithmetic via primitives on the slow path of the specialized sends. Immediate floats: tag `010` reserved, not implemented.
**LargeInteger** (v1.x, after first boot): `LargePositiveInteger`/`LargeNegativeInteger`, byte-format little-endian magnitudes; comparison/add/subtract/multiply/divmod primitives over digit arrays; everything else in-image. SmallInteger overflow paths then construct them instead of signaling.
**Fraction, ScaledDecimal**: pure image code. The VM has no opinion.

**Strings**: `ByteString` is **UTF-8 bytes**, format 8–15. `at:` and `size` are *byte*-indexed at the VM level (`'héllo' size` = 6). The character/grapheme layer is image code (`CodePointStream`, `do:` via decoding, comparison as byte comparison = code-point order for valid UTF-8). There is no WideString. Consequence accepted openly: O(1) `at:` yields bytes, not Characters; idiomatic code uses streams. `Character` is an ordinary object wrapping a code point (no immediates in v1). **Symbol** is a subclass of ByteString, interned via a strong, image-visible `SymbolTable` (open-addressed Array, image-managed; the VM consults it only in the bootstrap loader). Symbol identity = pointer identity after interning; selector comparison everywhere is `==`. (Weak interning is a v2 refinement alongside weak references generally, §14.)

Mutability: all byte-format objects (and Arrays used as literals) carry an **immutable flag** — Treaty: bit 7 of gcBits — set on literals and Symbols at compile/intern time; `ATPUT` and the `at:put:` primitive fail on it, and the fallback code signals `ModificationForbidden`.

### 16. Primitives

A flat numbered table: `CompiledMethod` header carries a 12-bit primitive index; the VM holds `primitives: [fn; 4096]` in Rust. Convention: **a primitive either fully succeeds (returns a value, no observable side effects on failure paths) or fails cleanly**, in which case execution falls through to the method's Smalltalk body — the fallback code. This convention is load-bearing: bounds errors, type errors, overflow, wrong-argument-counts all become ordinary in-image signals raised by fallback code, with full debugger access.

v1 primitive set (numbers in Appendix A.3): object essentials (`class`, `identityHash`, `==`, `new`, `new:`, `at:`, `at:put:`, `size`, `instVarAt:(put:)`, `perform:withArguments:`); arithmetic and bit operations (SmallInteger and Float full sets — these back the specialized sends' slow paths); `BlockClosure>>value…` (0–4 args + `valueWithArguments:`); Process/Semaphore/scheduler (§13) and the unwinder/handler-walk (§11); stream-of-bytes file I/O (open/close/read/write/position/size/delete on paths), stdio, monotonic and wall clocks, `primSignal:atMilliseconds:`; snapshot (§17); `registerClass:`, method installation, global-cache flush; one event-input primitive (`primNextEvent` → SmallInteger-encoded event or nil, the UI builds on it) and one bulk pixel-blit primitive reserved-but-unimplemented (display comes later; the Treaty reserves its number).

**No FFI in v1.** Every host capability is an enumerated primitive. (FFI is a v2+ design with its own document; nothing here precludes it.)

Primitive failure encoding: the Rust function returns `Result<Value, PrimFailCode>`; the fail code is stashed where fallback code can read it via `primFailureCode` (a reserved frame slot in primitive-bearing methods).

### 17. Image Format and Snapshots

An image file is:

```
[ header: magic 'STIM', version, flags, savedBaseAddress,
  oldSpaceByteSize, specialObjectsOffset, classListOffset, activeProcessOffset ]
[ old space: one contiguous heap dump ]
```

Saving (`primSnapshot:`): reach a safepoint; scavenge young space (twice, to empty it — the image is old-space-only); for the running process, store pc/frameOffset so it serializes as suspended; write header + old space; on resume-after-save the primitive returns `false`, on load-and-continue it returns `true` (classic idiom for "am I the resumed image?").

Loading: read/mmap old space; one linear pass over **object slots only** (headers need nothing — class indices, not pointers; this is the §2 design paying off) adding `delta = actualBase − savedBaseAddress` to every word with pointer tag; rebuild the VM-private class table from the image's class list; rebuild the send-site registry by walking compiled methods (or: registry is rebuilt lazily on first install — v1 walks, it's one pass); locate the active process from the header and `primTransferTo:` it. Symbols, caches: the global lookup cache starts empty; inline caches were saved as-is and remain valid (class indices are stable across save/load).

**Image zero** is produced by the cross-compiler (§18): the same format, byte-identical semantics, containing the kernel classes, compiled kernel methods, the special objects, and **one Process whose stack holds a single frame poised at the first instruction of `Smalltalk startUp`**.

Endianness/word size: little-endian, 64-bit only. Not negotiable in v1.

---

## Part IV — Compiler and Bootstrap

### 18. The Compiler and the GNU Smalltalk Bootstrap

**One compiler, written in Smalltalk, two hosts.** The Smalltalk→bytecode compiler is written from day one in a **portable dialect subset**:

- ANSI-classic classes only: collections, streams, basic magnitudes. No host-specific namespaces, extensions, or pragmas.
- All host divergence behind a `Platform` facade (file read/write, byte output, command-line args): one GST implementation, later one native implementation.
- Source lives in **chunk format** (`!ClassName methodsFor: 'x'! ... ! !`): GST files it in natively; our image's file-in loader (needed for a live environment anyway) reads the same files. The compiler's own source is thereby the first cross-system compatibility test.
- **Determinism rule** (load-bearing for §20's bit-equality test): no iteration over unordered collections anywhere in code generation or image writing. Literal pools, selector tables, class lists, symbol interning during bootstrap — all use insertion-ordered structures (`OrderedCollection` + lookup Dictionary as an index, never iterating the Dictionary). This is a coding standard of the compiler, stated here so the test suite may enforce it by construction (bit-identical output) rather than by review.

The compiler has three parts:

1. **Parser**: source → AST. Classic Smalltalk-80 grammar; chunk-format reader for file-in.
2. **Code generator**: AST → register bytecode. Single pass over the tree with a slot allocator (expression depth counter + temp map), block-capture analysis (the copy-vs-box classification of §10, a small pre-pass over each method's AST), macro-expansion of `ifTrue:`/`whileTrue:`/`and:`/`or:` with literal blocks, specialized-send selection (§12), send-site table construction.
3. **Heap writer** (cross-compile mode only): builds the initial object graph — classes, metaclasses, method dictionaries, symbols, literals, the special objects array, the poised initial Process and its stack — as bytes in our object format, and emits an image file per §17. This is the largest of the three parts; it is, in effect, a Smalltalk implementation of §§1–5 and 17 as a serializer.

**Bootstrap pipeline** (`make image0`): GST — pinned to an exact version; the development environment provides GNU Smalltalk 3.2.5 (Homebrew) directly, verified against everything this pipeline needs (chunk-format file-in, byte-exact binary output via `FileStream nextPutByte:`, SUnit). GST is unmaintained and packaging is fragile, so the version pin is the reproducibility story; a container wrapping the same binary is an option for CI hardening, not a prerequisite. GST files in the compiler source, which files in the kernel source tree, compiles every method, constructs the heap, writes `image0.im`. The Rust VM then runs it. After self-hosting (§20 phase 5), this pipeline is demoted to the from-nothing path and kept alive by a CI job that rebuilds and boots image zero on every commit — bootstrap paths rot in weeks if not exercised.

**GST 3.2.5 portability notes** (verified; part of the portable-dialect discipline): class definitions in chunk-format source use the classic five-keyword message (`subclass:instanceVariableNames:classVariableNames:poolDictionaries:category:`) — the newer `package:` variant is not understood by 3.2.5, and the five-keyword form is also the simpler contract for our own image's chunk loader. `Transcript` output flushes reliably under `gst -Q`, so the phase-2/3 test runners can diff plain stdout.

**Hedge**: if GST bit-rot ever bites, the portable-subset discipline is the mitigation — Pharo can substitute with a thin `Platform` adapter and a chunk-format loader (Pharo retains one). The discipline is cheap; keep it strict.

### 19. Interpreter and JIT Mechanics

**Interpreter** (Rust): a hot loop over `u32` instructions decoded with shifts/masks (no structs). Dispatch: `match` on opcode in a tight loop, written so rustc produces a jump table; revisit with explicit tail-call dispatch when the `become` guaranteed-tail-call feature lands in stable Rust — measure first, contort later. Cached in locals/registers across instructions: stack base, frame offset (or derived frame pointer), pc, the bytecodes pointer, literals pointer. All spilled to the Process/frame at safepoints, re-derived after.

**JIT** (later tier, spec'd now only as constraints the v1 design must honor — all already stated, collected here):

- Interpreter and compiled code share **frame layout byte-for-byte**; tier transitions happen **only at method entry** (invocation-counter in the send-site/activation path; compile after N calls). No on-stack replacement — a long-running loop in a cold method stays interpreted until next call; accepted.
- Template translation: each opcode → fixed instruction sequence; `slot[i]` = `[FP + 8·(4+i)]`; specialized sends inline their fast paths with slow-path stubs into the shared send machinery; safepoint poll = load-test-branch on the flag address; inline-cache compare against a patchable immediate, registered for install-time clearing (§8).
- PCs in frames remain bytecode offsets even for compiled frames (the return sequence maps back through a side table per method) — this keeps snapshots and the debugger oblivious to tiers. Equivalently v1-simple option: compiled methods spill the bytecode pc at sends only, which is the only place it's observable. Decide during JIT construction; both are Invariant-compatible.

**The debugger** needs no VM hooks beyond what exists: a suspended Process is plain data (§7); `primFrameInfo: (process, offset)` decodes one frame into an Array (method, pc, receiver, slot values) — one convenience primitive; modify-and-resume is `instVarAt:put:` on the stack object plus method installation plus `primTransferTo:`. Fix-and-continue at the granularity of "restart this frame with the recompiled method" is image-side logic: rewrite the frame's method slot and pc to 0. (Restarting frames with changed frame-slot counts requires re-pushing the frame — image-side, using the same primitives.)

---

## Part V — Test-First Construction Plan

### 20. The Plan

Two codebases, two languages, one binary contract. The TDD style that polices such a contract is **golden files against the Treaty**: the formats are specified as data before either implementation exists, and every layer is tested against hand-constructed expected bytes before the layer that produces those bytes automatically is built.

**Phase 0 — the Treaty as executable data.** Write Appendix A; encode it as `treaty.json` (opcodes, tag values, header field shifts/masks, frame slot indices, Treaty class indices, primitive numbers, special-object indices, image header layout). Generate (or hand-mirror with a checksum test) `treaty.rs` and `Treaty.st`. First tests, both sides: "ADD with d=4,a=0,b=1 encodes to this exact u32"; "header for a 3-slot fixed instance of class 17 with hash 0 is this exact u64"; "frame slot RECEIVER is 4". A CI check asserts the three files agree (the JSON is canonical; the others are generated or checksummed against it).

**Phase 1 — VM core in Rust, no Smalltalk anywhere.** Unit tests: tag round-trips, header pack/unpack, allocation into a test heap, per-format `at:` semantics, SSB barrier filtering. Then the interpreter against **hand-assembled bytecode**: a ~50-line test assembler (mnemonic tuples → `Vec<u32>`) and a heap-builder fixture (`heap.fixed(class, &slots)`, `heap.array(&[..])`, `heap.method(insns, literals, sites)`). The assembler is written together with its inverse, the disassembler, and they are round-trip-tested against each other; the disassembler is a keeper (debugging tool), the assembler remains test-only. Every opcode gets tests this way: `ADD` fast path, `ADD` overflow → slow path → send, `SEND` through empty caches → dictionary lookup → DNU, `JUMPTRUE` on non-boolean → mustBeBoolean, frame overflow → stack growth (assert the stack object was replaced and execution continued), `NLR` to dead frame → BlockCannotReturn path reached. GC unit tests: hand-built heaps, scavenge, assert forwarding and SSB drain; compaction with a suspended-process stack in the heap, assert frame offsets still resolve.

**Phase 2 — compiler in GST, no VM anywhere.** SUnit suites (GST ships SUnit): **parser** (source → expected AST, structural equality), **codegen** (source → expected mnemonic sequences — *the same mnemonics as the Rust assembler*, so test cases are eyeball-comparable across suites; port the Phase-1 test programs), **slot allocator** focused tests (nested expressions, temp reuse, block arg numbering — single-pass compilers hide their bugs here), **capture analysis** (copy vs. box classification per §10 on a battery of block shapes), **heap writer** (build a one-class mini-image; assert bytes at offsets per the Treaty; assert the image header). Plus the determinism rule's enforcement test: compile the kernel twice in fresh GST sessions; outputs byte-identical.

**Phase 3 — the handshake.** A `corpus/` directory of `.st` programs with `.expected` output files, consumed by both sides: GST-side runner compiles each program to an image; Rust-side runner executes and captures output via the stdio primitive; diff. First corpus entry: `Transcript show: (3 + 4) printString` → `7`. **The day this passes, the architecture is proven**; everything afterward is accretion. The corpus is append-only and becomes the permanent regression suite.

**Phase 4 — kernel growth, corpus-driven.** Extend the corpus through: collections and streams; blocks, closures, NLR (escaping blocks, NLR-after-home-dead); exceptions adversarially (`resume:`, `return:`, `retry`, `pass`, signal-during-handler, `ensure:` during NLR, signal during unwind, nested ensures, `ifCurtailed:`); processes and semaphores (producer/consumer, priorities, `terminate` with pending ensures — the §11 termination design gets its own battery); Delays and the timer. **Snapshot round-trips** as a corpus *mode*: run any corpus program to a marker, snapshot, reload in a fresh VM process, run to completion, diff against the uninterrupted run — one mechanism covering relocation, frame position-independence, and Process serialization for every test in the corpus. GC stress as another mode: the whole corpus under a 64KB young space and aggressive tenuring, so collection happens constantly under real programs.

**Phase 5 — self-hosting and the payoff test.** The compiler source files into the image via the chunk loader; the in-image compiler compiles the entire corpus and the kernel; outputs must be **bit-identical** to GST-cross-compiled outputs. This test is brutal by design — it flushes every accidental dependence on GST collection ordering, hash iteration, or float printing. (The §18 determinism rule exists so this test can pass at all.) After it passes: GST is bootstrap-only, exercised by the CI image-zero job; the in-image compiler is the system compiler; development moves into the live image.

**Phase 6 — JIT, differentially tested.** The corpus runs in three modes — interpreter, JIT-after-N-calls, JIT-always (N=0) — outputs diffed. Plus targeted tests for tier transitions at the snapshot boundary (compiled code is never saved; images are tier-free by construction).

Sequencing note: Phases 1 and 2 are independent and parallelizable; Phase 0 is a hard prerequisite of both and is deliberately unglamorous — roughly a week of treaty-and-harness work before any fun, and the corpus harness from Phase 3 is the single highest-leverage artifact in the project.

---

## Appendix A — The Treaty (Binary Contract)

*Canonical machine-readable form: `treaty.json`. This appendix is its prose rendering; on conflict, the JSON wins after v0.1.*

### A.1 Tags, Header, Frame

```
TAG_INT_BIT        = 0x1
TAG_PTR_MASK       = 0x7, TAG_PTR = 0x0
TAG_FLOAT_IMM      = 0x2   (reserved)

HDR_CLASS_SHIFT    = 42, HDR_CLASS_BITS = 22
HDR_HASH_SHIFT     = 20, HDR_HASH_BITS  = 22
HDR_NSLOTS_SHIFT   = 12, HDR_NSLOTS_BITS = 8   (255 = overflow word precedes header)
HDR_FORMAT_SHIFT   = 8,  HDR_FORMAT_BITS = 4
HDR_GC_SHIFT       = 0,  HDR_GC_BITS    = 8    (bit 7 = immutable)

FMT_FIXED = 0, FMT_PTRS = 1, FMT_BYTES_BASE = 8  (pad = format - 8)

FRAME_CALLER = 0, FRAME_RETINFO = 1, FRAME_METHOD = 2,
FRAME_FLAGS  = 3, FRAME_RECEIVER = 4, FRAME_FIXED = 5
FLAG_HANDLER = 1, FLAG_ENSURE = 2, FLAG_BLOCKCTX = 4; SERIAL_SHIFT = 32
```

### A.2 Opcodes

```
00 NOP            10 MOVE d,a       20 SEND d,r,s     30 ADD d,a,b    40 AT d,a,b
01 BREAK          11 LOADK d,k      21 SENDSUPER      31 SUB d,a,b    41 ATPUT d,a,b
                  12 LOADINT d,imm  22 RET a          32 MUL d,a,b    42 SIZE d,a
                  13 LOADNIL d      23 RETSELF        33 DIV d,a,b    43 CLASSOF d,a
                  14 LOADTRUE d     24 NLR a          34 MOD d,a,b    44 NOT d,a
                  15 LOADFALSE d    25 PRIM n         35 LT d,a,b     45 IDEQ d,a,b
                  16 LOADSELF d     26 MKCLOSURE d,b  36 GT d,a,b
                  17 GETIVAR d,i    27 CAPTURE c,a    37 LE d,a,b
                  18 SETIVAR i,a    28 JUMP off       38 GE d,a,b
                  19 GETBOX d,a     29 JUMPTRUE a,off 39 EQNUM d,a,b
                  1A SETBOX a,b     2A JUMPFALSE a,off
                  1B MKBOX d,a
```

(Gaps reserved. Encodings: §6. Final numeric assignments live in treaty.json; this table is normative for v0.1.)

### A.3 Treaty Class Indices and Primitive Numbers

Class indices 1–63 fixed: `1 Object… ` — assigned in treaty.json; notable: SmallInteger=8, Float=9, ByteString=12, Symbol=13, Array=16, ByteArray=17, Box=20, BlockClosure=21, CompiledMethod=22, CompiledBlock=23, Process=24, Semaphore=25, MethodDictionary=26, UndefinedObject=5, True=6, False=7. Primitive numbers: blocks 1–99 object essentials, 100–199 numeric, 200–299 blocks/processes/unwinder, 300–399 I/O & time, 400+ system (snapshot=400, registerClass=401, methodInstall=402, frameInfo=410). Final assignments in treaty.json.

### A.4 Special Objects Array

```
0 nil   1 true   2 false   3 Smalltalk   4 Processor   5 classList
6 symbolTable   7 specializedSelectors (Array: #+ #- #* #// #\\ #< #> #<= #>= #= #== #at: #at:put: #size #class #not)
8 selDoesNotUnderstand:   9 selMustBeBoolean   10 terminateTrampoline
11 lowSpaceSemaphore   12 timerSemaphore
```

### A.5 Image Header

```
offset 0:  magic 'STIM' (4 bytes)     offset 4:  version u32
offset 8:  flags u64                  offset 16: savedBaseAddress u64
offset 24: oldSpaceByteSize u64       offset 32: specialObjectsOffset u64
offset 40: classListOffset u64        offset 48: activeProcessOffset u64
offset 56: reserved (8 bytes); old space begins at offset 64
```

---

## Appendix B — Deliberate Omissions

For the record, features classic Smalltalk VMs have that this design **excludes by decision**, with the invariant each exclusion protects: `become:` (no forwarding pointers, no all-heap scans); reified `thisContext`/MethodContext (contiguous frames; the debugger uses suspended-process introspection instead); hybrid object formats (one shape per accessor); immediate floats and characters (one-bit integer tag stays trivial; tag 010 reserved); on-stack replacement (tier transitions at method entry only); native threads (one OS thread executes Smalltalk; green processes only); FFI (enumerated primitives only, v1); weak references and finalization (v2, gcBit reserved); class-index recycling (indices are forever); WideString (UTF-8 monoculture).

Each is re-admittable later without violating the Stack Invariant or the Treaty's frozen constants — that re-admissibility was checked when each decision was made, and the relevant section notes the hook where one exists.
