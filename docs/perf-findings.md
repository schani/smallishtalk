# Where we are slow: the self-compile, profiled

**Workload**: the compiler compiles itself in-image (kernel +
PlatformImage + 10 compiler files → gen2 image), release build,
2026-07-02. **Wall time ≈ 2.27 s** (gate-free build) vs **GST 3.2.5 ≈
0.53 s** — ratio ≈ 4.3×. All numbers below from `Profiler spy:` (2433
samples @ 1 ms, overhead 0.9%) and the exact counter table
(`bench/results/self_compile.counters.txt`); raw profile in
`bench/results/self_compile.profile.txt`.

Headline counters: **41.65 M sends** (≈ 54 ns/send), **452.7 M
instructions** (≈ 200 M insn/s), 1.07 M young allocations (34.6 MB),
4 scavenges totalling **4.1 ms**.

## Ranked findings

### 1. Symbol interning is the single biggest cost: ~39% of the run

`StImageWriter>>symbolFor:` carries **38.8% total**. Under it:
`Symbol class>>intern:` (11.2% self) linearly scans the global
SymbolTable, testing each candidate with `Symbol class>>is:sameAs:` /
`String>>=` — and `String>>=` alone is **15.0% self**, the #2 leaf in
the whole profile. Every send site the writer materializes interns its
selector against an ever-growing table: classic O(n²).

*Hypothesis "O(n) literal-pool dedup and symbol interning in the
writer": CONFIRMED, and it is #1.*

**Fix direction**: a hashed symbol table (bucket by length + first
bytes, or a real hashed Dictionary in the kernel). Plausible upside:
−25–35% of total runtime.

### 2. The generic iteration protocol: `do:` is 33.1% self, on-stack in 90% of samples

`SequenceableCollection>>do:` is the hottest leaf by far. Every `do:`
iteration pays: a `size` send, an `at:` send, and a full block
activation (`value:` = primitive 201, **3.07 M calls**). The
counters show why the sends are expensive for collections:

- `send.staged` = **9.15 M** — 22% of all sends take the
  specialized-send *slow path* (stage receiver+args above the frame,
  then full dispatch): `=` 3.63 M, `size` 2.78 M, `at:` 2.68 M
  fallthroughs. `OrderedCollection` (a two-ivar object, not an
  indexable format) never hits the `AT`/`SIZE` fast path.

*Hypothesis "every `OrderedCollection>>at:`/`size` pays the
specialized-send slow path": CONFIRMED.*

**Fix directions** (compounding): concrete `do:` overrides that hoist
`size` and walk the backing store directly; a VM fast path for
`OrderedCollection` `at:`/`size` reading the Treaty-known
`store`/`count` slots; cheaper staged sends (the staging copy is pure
overhead vs. a direct dispatch).

### 3. Character boxing and per-byte type tests: ~15% combined

`Object>>isKindOf:` is **8.6% self** (a send-loop up the superclass
chain per call), `Character>>=` **5.2%**, `Character class>>value:`
0.8%, `WriteStream>>nextPut:` 2.3% self / 3.9% total.
`WriteStream>>nextPut:` does `isKindOf: Character` **per byte
written**; the lexer compares boxed Characters (`Character>>=` calls
`isKindOf:` too) instead of raw byte values.

*Hypothesis "per-byte `WriteStream>>nextPut:` with an `isKindOf:` per
byte": CONFIRMED (as a mid-tier cost, not the top one).*

**Fix direction**: byte-specialized stream writing (the writer already
deals in SmallInteger bytes almost everywhere); lexer on raw bytes via
`String>>at:`; an `isKindOf:`-free `Character>>=`.

### 4. Killed hypotheses (the instruments earn their keep)

- **GC share**: 4 scavenges, **4.1 ms = 0.16%** of the run, 4.7 MB
  copied, no compactions, `<vm:scavenge>` never even sampled. The
  default 8 MB nursery absorbs the writer's churn. *KILLED* — do not
  spend effort here.
- **`Treaty` constant lookups on cold caches**: global lookup cache
  misses total **1,784** (of 41.65 M sends); dictionary walks are
  noise. Eager cache invalidation + the global cache work. *KILLED.*
- **`String>>,` building**: 0.1% self. The writer streams; it does not
  concatenate. *KILLED for this workload.*

### 5. Send machinery observations (VM-side, longer-term)

- **Inline-cache misses: 2.63 M (6.3% of sends)** — the hot `do:` /
  `value:` / `at:` sites are megamorphic. A 2-entry polymorphic cache
  would soak most of these (each miss pays the global-cache probe).
- **23% of all executed instructions are `MOVE`** (105.3 M of 452.7 M)
  — codegen shuffles registers around calls far more than it computes.
  Copy propagation / better slot assignment in `StCodeGen` is a real
  instruction-count lever.
- `RET`+`JUMPFALSE`+`GETIVAR`+`LOADSELF`+`LOADNIL` are the next tier —
  activation overhead, consistent with 41 M tiny methods activations.

### 6. Instrument validation results (plan §3/§5 gates)

- **Gated counter tier**: the runtime gate — one predictable branch in
  the dispatch loop, even when *off* — measured **~10%** on the
  self-compile (2.52 s vs 2.27 s). Per the plan's decision rule the
  gate moved to **compile time**: default builds carry no gate;
  `--features vm-counters` builds carry the histogram tier.
- **Always-on tier**: free at measurement precision — the instrumented
  (gate-free) binary matches the pre-instrumentation 2.28 s baseline.
- **Profiler overhead**: spy-on 2.574 s vs 2.552 s on the same build =
  **0.9%**, far inside the ≤10% budget.
- **Attribution error bound**: `poll.gap_max` = 188 instructions;
  p99 ≤ 64 — safepoints are dense, so sample smear is bounded and
  small, as designed.
- **Cross-validation**: sampler and counters agree — e.g. the sampler
  sees no GC while the exact pause counters read 4.1 ms (0.16%), and
  block-activation samples (`<vm:prim:201>` 1.2%) match 3.07 M calls
  at ~ns each.

## Benchmark baseline (bench/history.csv, gate-free timed build)

| workload | vm ms | gst ms | ratio | note |
|---|---|---|---|---|
| send_loop | 41 | 9 | 4.56 | raw send/activation cost |
| arith_loop | 53 | 11 | 4.82 | SmallInt fast paths + loop overhead |
| block_value | 44 | 9 | 4.89 | block activation |
| ordered_collection | 22 | 10 | 2.20 | add:/at:/size |
| dictionary | 374 | 5 | 74.8 | **kernel Dictionary is a linear scan** — O(n²) vs GST's hash |
| string_build | 112 | 15 | 7.47 | nextPut: per byte (finding 3) |
| exceptions | 11 | 73 | 0.15 | we are ~6× *faster* than GST |
| process_pingpong | 5 | 3 | 1.67 | |
| self_compile | 2260 | 517 | 4.37 | the real workload |

(Medians of 5 after warmup, 2026-07-02, also the latest rows in
`bench/history.csv`. Checksums cross-validated VM-vs-GST on every
workload; for self_compile the checksum is the output image's byte
size, so bit-identity held during timing too.)

The dictionary row is its own finding: the kernel `Dictionary` is
insertion-ordered parallel arrays with linear `at:` — fine for the
compiler's small tables, catastrophic at scale. A hashed Dictionary
fixes both this row and finding 1 (symbol interning wants exactly that
structure).

## Recommended attack order

1. Hash the symbol table / `Symbol class>>intern:` (finding 1) —
   biggest single win, pure image code, protected by phase-5
   bit-identity tests.
2. Concrete `do:` implementations + hashed Dictionary (findings 2, and
   the dictionary bench row).
3. Byte-level `WriteStream`/lexer paths, `isKindOf:`-free character
   handling (finding 3).
4. VM: OrderedCollection `at:`/`size` fast path; then polymorphic
   inline caches; then codegen MOVE reduction (finding 5).

Re-run `make bench` after each step; the self_compile row and its
counter table are the scoreboard.
