# Better generated code, same VM: the codegen plan

**Question**: can the compiler emit better bytecode — no VM changes — so the
self-compile ratio vs GST improves? Compile-time cost is acceptable if the
compiled code runs faster.

**Why this moves the ratio asymmetrically**: on GST the compiler runs as
GST-native bytecode produced by *GST's* compiler — our codegen's output never
executes there. Better generated code speeds up only our side of the ratio.
Extra analysis in the codegen costs both hosts roughly proportionally (the
workload *is* compilation), so quality-for-compile-time trades are nearly
free on the ratio.

**Status**: DONE, 2026-07-02. Levers 1 (`to:do:` inlining) and 3
(`isNil`/`notNil`) are **landed**; lever 2 (effect positions) was
prototyped, measured at ~0, and **removed**; lever 4 (branch fusion) is
parked. Baseline at the start: self-compile 654 ms, GST 413 ms, ratio
1.58×; after: **~605–625 ms, ratio ~1.40×**. Evidence:
`bench/results/self_compile.counters.txt` (gated build),
`self_compile.profile.txt`, and disassemblies of representative methods
via `StCodeSpec>>mnemonics`.

## The evidence

Counters (self-compile, current):

- 128.6 M instructions, 9.74 M sends, ≈ 8.5 M frame push/pops (RET 7.88 M +
  RETSELF 0.59 M). Every frame push nil-fills temps+scratch and every pop
  nil-fills the whole frame — activations are the dominant hidden cost.
- **MOVE = 34.8 M = 27% of all executed instructions.** LOADNIL 4.4 M,
  LOADSELF 4.8 M. Codegen shuffles far more than it computes.
- Block machinery: MKCLOSURE 157 k (≈ 40% of the 394 k young allocations),
  `value:`/`value` activations 1.07 M (prims 200/201).
- Profile top: `OrderedCollection>>do:` 10.1% self (block activation per
  element — VM territory, deferred), `Object>>isKindOf:` 9.3% (compiler
  *source* issue, separate track), **`SmallInteger>>to:do:` 6.5% self /
  28.7% total**, `OrderedCollection>>grow` 5.5%, `String>>=` 3.9%.
- `instVarAt:` (prim 9) 227 k calls: the codegen's ivar-access-inside-blocks
  path. `bitAnd:`/`bitShift:` (prims 111/114) 764 k primitive-send
  activations from the writer/encoder — source-level, not codegen.

Disassembly of `sum | s | s := 0. 1 to: 10 do: [:i | s := s + i]. ^s`:

    MKBOX 1, 1          ; s is boxed (captured + mutated)
    LOADINT 2, 0
    SETBOX 1, 2
    LOADINT 6, 1
    LOADINT 7, 10
    MKCLOSURE 8, 0      ; closure allocated per call
    CAPTURE 0, 1
    SEND 2, 6, 0        ; to:do: — then per iteration: value: send,
    GETBOX 2, 1         ;   frame push/pop, GETIVAR 4 + GETBOX/SETBOX
    RET 2               ;   for every access to s

and of `loop | i | i := 1. [i <= 10] whileTrue: [count := i. i := i + 1]`:

    LOADINT 1, 1
    MOVE 2, 1           ; DEAD: statement value of `i := 1`
    MOVE 2, 1           ; copy i for LE (needed)
    LOADINT 3, 10
    LE 2, 2, 3
    JUMPFALSE 2, 7
    MOVE 2, 1           ; count := i: value staged in scratch...
    SETIVAR 0, 2        ; ...then stored — could be SETIVAR 0, 1
    MOVE 2, 1
    LOADINT 3, 1
    ADD 1, 2, 3
    MOVE 2, 1           ; DEAD: statement value of `i := i + 1`
    JUMP -11
    LOADNIL 2           ; DEAD: statement value of the whileTrue:
    RETSELF

The dead MOVEs, the dead LOADNIL tails, and the boxed/closure-carried loop
above are all compiler-inflicted; the VM executes them faithfully.

## Ranked levers

### 1. Inline `to:do:` (then `to:by:do:` with literal step, `timesRepeat:`)

The classic Smalltalk optimization — GST, Squeak and Pharo all do it in
their compilers, so the portable dialect's semantics stay host-identical
(GST already runs the compiler's own `to:do:` loops inlined).

When the third part is a literal block with exactly one argument and no
temps, compile:

    <recv>  := eval receiver        ; slot pinned for the loop's extent
    <i>     := MOVE from <recv>
    <limit> := eval limit
    loop:  LE t, <i>, <limit> ; JUMPFALSE t, exit
           <body inlined, block arg bound to slot i>
           LOADINT one, 1 ; ADD <i>, <i>, one ; JUMP loop
    exit:  MOVE dest, <recv>        ; to:do: answers the receiver

This eliminates, per loop: the `to:do:` send + activation, the MKCLOSURE +
CAPTUREs, and the boxing of mutated captured locals; per **iteration**: the
`value:` send, a full block-frame push/pop (nil-fills included), and the
GETIVAR/GETBOX/SETBOX indirection on every captured variable — replaced by
direct slot access. Non-SmallInteger receivers still work: LE/ADD fall
through to staged `#<=`/`#+` sends, preserving numeric semantics.

**Guards** (Codex-reviewed; the first design's scope-relative collision
check was unsound because the walkers and nested codegens would compute
different answers). The inlinability predicate must be a *pure function of
the method AST + class scope*, so every StCodeGen instance — the method's,
a nested block's, and every analysis walker — agrees. Inline iff:

1. the `do:` argument is a literal block with exactly one argument `v` and
   no temps;
2. `v` is not an instance-variable name of the class;
3. `v` occurs **nowhere else in the entire method** (as reference,
   assignment target, or declaration) outside `to:do:`-shaped bodies that
   themselves bind `v` — sibling loops reusing `:i` still inline, and
   shadowing anything (locals, captures, globals) is impossible by
   construction;
4. inside the body, `v` is never assigned and never occurs inside a nested
   block that compiles as a *real closure*. Nested blocks flattened by the
   syntactic macros (`ifTrue:` etc.) are transparent; a nested `to:do:` is
   transparent iff it passes this same predicate (recursion on a strictly
   smaller subtree — this matters: an inner `to:do:` that *falls back*
   turns its body into a real closure, so an optimistic check would lie).

Everything else falls back to the real send. With these guards the
existing analysis walkers are automatically correct: `walkMessage:with:`
already flattens macro block arguments, and loop-variable references bind
nothing in any scope, so `classifyLocals`/`freeLocalNamesOf:`/
`blockUsesSelfOrIvars:` all ignore them.

**Mechanics**: `macroKindOf:` gets a `#toDo` kind gated on the predicate;
the codegen keeps a stack of loop-variable bindings consulted by `resolve:`
(innermost first, always unboxed slots); the receiver/loop/limit slots
allocate at `sp` and `sp` is bumped for the loop's extent so body sends
stage above them (same discipline as `compileWhile:`'s condition slot).
Guard 3's over-conservatism (a real nested block wanting to close over the
loop variable could actually be supported with copy-capture, which matches
the kernel's fresh-argument-per-iteration semantics) is a later extension —
measure the fallback rate first.

Expected: `to:do:` carries 28.7% total time today; killing the control
overhead should recover on the order of **−10–15% wall time**.

### 2. Effect-position codegen (statement context)

Thread a "value needed?" bit through statement compilation:

- Assignment statements: skip the trailing MOVE-to-dest (one dead MOVE per
  assignment statement — see disassembly).
- Assignment *sources* that are plain unboxed slots: SETIVAR/SETBOX/MOVE
  directly from the source slot instead of staging through dest.
- `ifTrue:`/`ifFalse:`/`whileTrue:` in statement position: restructure to
  drop the dead LOADNIL arm *and* its jump (for a value-less single-arm if:
  condition, JUMPFALSE past the body, nothing else — this is a different
  shape, not a deletion from the two-arm form).

Review caveats: the "value unused" bit is a property of statement position
in a sequence, never inherited top-down — the last statement of an inlined
macro block in value position still produces the value (`x := cond ifTrue:
[y := 1] ifFalse: [2]` must keep the inner assignment's value).

MOVE is 27% of executed instructions and LOADNIL 3.4%; the dead share is a
few tens of millions of cheap instructions. Expected **−3–6%**.

### 3. Inline `isNil` / `notNil`

`x isNil` today: MOVE + SEND + full activation of a method that loads a
boolean. Inline: `LOADNIL t ; IDEQ dest, x, t` (+ NOT for `notNil`) — no
send at all. 49 static sites in kernel + compiler, several in the hottest
walkers. This is a dialect decision, not a pure optimization (Codex): a
class overriding `isNil`/`notNil` would silently stop being consulted. So
document both as **sealed selectors** in SPEC terms, exactly like the
specialized selectors (`#+`, `#==`, `#size`…) that already compile
unconditionally to opcodes — this adds two names to an existing category
of the dialect. Optionally extend to `ifNil:`/`ifNotNil:` macros later.
Expected **−1–2%**.

### 4. Branch fusion for conditions (later)

`(a notNil and: [a > 3]) ifTrue: [...]` materializes booleans, jumps over a
LOADFALSE, then re-tests with a second JUMPFALSE. Compiling conditions in a
branch context (true/false jump targets threaded through `and:`/`or:`/`not`)
removes 2–4 instructions per compound condition. Also loop rotation
(condition at the bottom) saves one JUMP per iteration. Real but smaller and
the most invasive rewrite of the control macros — do it after 1–3, if the
profile still says so.

### Considered and parked

- **Pre-built closures for clean blocks** (no captures, no NLR) as literals:
  saves ~157 k allocations but GC is 0.16% of the run; adds closure-identity
  semantics questions and image-writer work. Not worth it now.
- **`=` as plain SEND instead of EQNUM**: EQNUM's int fast path hits 83%
  (2.84 M executed, 487 k fallthroughs); trading it for an inline-cached
  send would tax the majority to help the minority. No.
- **Ivar access inside blocks** (`instVarAt:` sends, 227 k): no opcode reads
  an ivar of an arbitrary slot; genuinely needs the VM. Parked with the
  other VM items (block-activation fast path, staged-send Vec allocation,
  polymorphic caches).

## Safety net

Golden mnemonic tests in `st/tests/CodeGenTests.st` update alongside each
lever (they pin exact output). Phase-5 bit-identity is preserved by
construction (GST cross-compile and in-image compile share the one codegen
source — both change together). Corpus ×4 modes + full SUnit on both hosts
gate every step; `make bench` (self_compile row + counter table) is the
scoreboard. New tests per lever: inline-`to:do:` semantics (value, NLR out
of the body, float receiver via fallthrough sends, every guard's fallback
path), effect-position equivalence, `isNil` on non-nil/nil.

## Order of attack

1. Lever 1 (`to:do:`) — prototype first as the decisive experiment.
2. Lever 2 (effect positions) — mechanical, measurable via MOVE/LOADNIL
   opcode counts.
3. Lever 3 (`isNil`/`notNil`).
4. Re-profile; decide on lever 4 vs. the parked VM items.

## Experimental results (2026-07-02)

All three levers were prototyped and measured (full ladder green each
time: GST SUnit, corpus ×4, 171/171 release incl. phase-5 bit-identity).
Outcome: levers 1 and 3 kept (with dedicated SUnit tests in
`tests-inline-loops` categories and runtime-semantics corpus program
`021_inline_loops.st`); lever 2 reverted on the evidence below.

**Lever 1, `to:do:` inlining**: 17 of the 20 static `to:do:` sites in
kernel+compiler inline (the 3 fallbacks are one cold method whose `i` is
used across loops *and* outside them). `SmallInteger>>to:do:` left the
profile entirely (was 6.5% self / 28.7% total); block activations
1.03 M → 689 k, MKCLOSURE 157 k → 130 k. But self-compile only
**654 → 645 ms (−1.4%)** — the 28.7% "total" was mostly body work that
remains, and the inline loop re-executes placement instructions the
shared kernel loop amortized. Where it shines is loop-and-arithmetic
code: **send_loop ratio 4.56→2.33 → 2.10, arith_loop 4.82→2.45,
block_value 4.89→2.2–2.6** (the drivers' own loops are `to:do:`). The
guard analysis costs ~1% of compile time (`macroKindOf:` now visible at
1.2% self), paid on both hosts.

**Lever 2, effect positions (scoped to assignments)**: mechanism
confirmed — MOVE 34.8 M → 32.7 M — but **no measurable wall change**:
the removed MOVEs were ~1.6% of instructions at ~1 ns each. The bulk of
MOVE traffic is send-argument staging, which the overlapping-frame
calling convention requires. The full lever (dead LOADNIL arms of
statement ifs/whiles) is bounded by LOADNIL's 3.4% share — not worth the
`compileIf:` restructuring on this evidence. **Reverted**: complexity in
the hottest compile path for nothing measurable.

**Lever 3, `isNil`/`notNil` → LOADNIL+IDEQ(+NOT)**: the surprise
winner — **self-compile 647 → ~603–620 ms (−5–7%), ratio → 1.39–1.41**.
Total sends fell 9.65 M → 8.48 M and, notably, **inline-cache misses
halved (1.37 M → 0.69 M)**: the nil-test sites were the megamorphic
ones. The 49 static sites sit in the hottest walkers (`classifyLocals`,
`resolve:`, parser token tests), so their dynamic share was far above
the 1–2% estimate.

**Net (levers 1+3 landed, final counters)**: self-compile **654 →
~605–625 ms, ratio 1.58× → ~1.40×**. Sends 9.74 M → 8.45 M,
inline-cache misses 1.37 M → 0.69 M, block activations 1.03 M → 689 k,
MKCLOSURE 157 k → 130 k, young allocations 394 k → 361 k. GST's own
wall went ~415 → ~437 ms: the guard analysis costs ~5% of compile time
on both hosts — GST pays it without the codegen benefit; we pay it and
win more back.

**Post-change profile** (what's left, in order): `OrderedCollection
>>do:` 11.3% self / 85.8% total — pure block-activation cost, VM
territory; `OrderedCollection>>grow` 5.8% + `StByteBuf>>grow` 3.4% —
source-level pre-sizing, not codegen; `Object>>isKindOf:` 5.0% — AST
node type tests, compiler-source fix (node-kind flag methods);
`String>>=` down to 2.9%. **Conclusion: codegen leverage on the
self-compile ratio is now largely spent** — the remaining gap vs GST is
send/activation machinery (VM) and library-level costs, not code
quality. Lever 4 (branch fusion) would buy ~1–2% at high complexity;
park it. The next real ratio movers are the deferred VM items
(block-activation fast path, staged-send allocation, OrderedCollection
`at:`/`size` fast path) and the two source-level fixes above.
