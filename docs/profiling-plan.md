# Profiling plan

> **Status (2026-07-02): implemented, phases A–E.** Usage lives in
> `docs/profiling.md`; the phase-E results in `docs/perf-findings.md`.
> One design decision was overturned by its own validation gate: the
> runtime counter gate measured ~10% on the dispatch loop, so per §3's
> decision rule it moved to compile time (`--features vm-counters`).

Goal: a principled answer to "where are we slow" — a profiler **built into
the VM and driven from Smalltalk**, useful both for profiling Smalltalk
programs (the self-hosted compiler is the motivating workload: 2.28 s vs
GST's 0.52 s) and for understanding the VM itself. Everything below follows
the project's rules: treaty-first for anything both codebases see,
tests-first for every layer, and zero cost when the profiler is off (the
phase-5 bit-identity suite is the regression guard).

## 1. Three instruments, one system

Different questions need different instruments; the plan builds all three
behind one Smalltalk-facing API:

1. **Sampling profiler** — *"which Smalltalk code is hot?"* Statistical
   stack sampling at safepoints, MessageTally-style. Primary tool for
   profiling the compiler and other image code.
2. **Exact VM counters** — *"what is the VM doing, and how often?"* Send
   fast/slow-path ratios, cache misses, allocation volume, GC pauses,
   primitive calls, per-opcode mix. Diagnostic, not time-attributed; they
   explain *why* a sampled hotspot is hot.
3. **Native profiling workflow + benchmark harness** — *"where does the
   Rust code spend cycles, and are we getting faster over time?"* External
   `perf`/flamegraph on the release binary plus a fixed workload suite with
   history, including GST ratio tables.

## 2. The sampling profiler

### Trigger

A host timer thread (the first real use of the spec's §13 "host-side timer
tick") sets a shared `AtomicBool` every T ms (default 1 ms). The
interpreter's **existing** safepoint poll — one load-test-branch at every
send and backward jump — is widened to that atomic. Sampling therefore adds
**zero new hot-path cost**: the same branch that already serves preemption
and timers now also serves the profiler. (Side benefit: this thread is the
missing tick source for real preemptive scheduling; the plan wires the flag,
scheduling semantics stay as they are.)

### Taking a sample

At a flagged safepoint the VM walks the frame chain via `FRAME_CALLER`
links (bounded depth, default 64), recording per frame:

- **method identity as an interned id**, not a heap Value: methods move
  under GC, so keying aggregation by `Value` would make the profile store a
  GC root that every scavenge and compaction must forward. Instead the
  sampler resolves each frame's method to a small integer id through a
  Rust-side cache keyed by `(method address, gc_epoch)` — a hash hit after
  the first sample of a method, with the name string (methodClass name +
  selector; CompiledBlocks as `[] in Class>>selector`) built **once per
  method per GC epoch**. The epoch key makes stale addresses impossible
  after a moving collection (the cache entry simply misses and
  re-symbolizes), so the profile data stays plain Rust state with no GC
  interaction, and steady-state sampling does no string work at all.
- the **bytecode pc bucket** of the leaf frame (coarse, for later
  line-level attribution; v1 reports method-level only).

Aggregation is a leaf-tally plus a full-path tally (call-tree), stored VM-
side. A sample is O(depth) with no allocation in the common case.

### Attributing VM time

Samples land only at safepoints, so time inside GC or a long primitive
would smear onto the next executed method. Fix: the GC entry points and the
primitive dispatcher check the sample flag **at their own boundaries** and,
if due, take the sample immediately with a *pseudo-leaf* appended:
`<vm:scavenge>`, `<vm:compact>`, `<vm:prim:NNN>`. The Smalltalk stack
underneath is still recorded, so the report can say "12 % of time in
scavenge, mostly under `WriteStream>>nextPut:`".

Documented — and measured — bias: straight-line bytecode between
safepoints attributes to the following poll site. Safepoints are dense
(every send and backward jump) and v1 has no FFI, so the only long
poll-free native regions are GC and numbered primitives, both of which
self-report (above). To *quantify* rather than assume the residual bias, a
gated counter records the distribution of instructions executed between
polls; the bench report includes its maximum and p99 so the attribution
error bound is a number, not a hope.

### The walkability contract

Sampling relies on an invariant this VM already maintains for the GC:
**at every safepoint the frame chain is consistent** — caller links are
SmallIntegers forming a chain to the base frame, every stack word is a
valid tagged value, and `save_regs` runs before any walk. Send setup
cannot break this: the safepoint poll fires *before* the callee's control
words are written, and unwinding polls only inside ensure-block
activations, where the chain below is intact. The plan makes this contract
explicit and enforced: a stress mode forces a sample at **every** poll
across the entire corpus — every walk must complete without faulting and
every frame must symbolize.

### Determinism guard

Profiler **off** must change nothing: the timer thread isn't started, the
poll sees the same flag word, no allocation paths change. The phase-5
bit-identity tests and the full corpus run with the profiler off are the
enforcement.

## 3. Exact VM counters

Two tiers, chosen by what the increment costs relative to the path it sits
on:

**Always on** (a `u64` add on an already-slow path — unmeasurable):
- send slow paths: inline-cache misses, global-cache misses, dictionary
  walk count *and* total classes walked, DNU count, mustBeBoolean count,
  specialized-send fallthroughs (per opcode kind)
- primitive calls and failures (per primitive number)
- allocation: count + bytes, young vs old, large-object count
- GC: scavenge/compact counts, **pause times**, bytes copied, bytes
  tenured, SSB size at drain, remembered-set survivors
- stack growths, process switches, semaphore blocks, unwind runs, ensure
  interceptions, NLRs, cache flushes, method installs

**Runtime-gated** (a branch on a mostly-false bool, perfectly predicted):
- per-opcode dispatch histogram + total instruction count (gives
  instructions/second and instruction mix)
- per-send counter (gives sends/second; with wall time → ns/send)

Every counter has a name in one Rust table; `SMALLISHTALK_STATS=1` prints
the full table at exit, so **any** run — corpus, phase-5, the binary — can
double as a measurement without touching image code.

The enablement model is validated, not assumed: the gated tier also sits
behind a cargo feature (`vm-counters`), and the bench harness compares the
default build (gate present, off) against a feature-less build on the
self-compile, so the cost of the *gate itself* — branch-predictor and
icache effects included — is a measured number. If it is not ≈ 0, the gate
moves to compile time.

## 4. Smalltalk-facing API

New treaty primitives in a reserved band (Appendix A.3 gains
"420–439 profiling"; treaty.json → treaty.rs → Treaty.st regenerated):

| # | primitive | behavior |
|---|---|---|
| 420 | `primProfilerStart:` | start sampling at the given interval (ms); resets sample store |
| 421 | `primProfilerStop` | stop the timer; sampling ceases |
| 422 | `primProfilerReport` | materialize the tally **on demand** as heap Arrays: rows of `{name. selfSamples. totalSamples}` plus total count and interval |
| 423 | `primProfilerTree` | v1.5: the path tally as nested Arrays (flat report ships first) |
| 424 | `primVmCounters` | Array of `{name. value}` rows for every counter |
| 425 | `primVmCountersReset` | zero the resettable counters |
| 426 | `primProfilerGate:` | enable/disable the gated hot-path counters |

Kernel gains a `Profiler` class (pure image code over these primitives):

```smalltalk
Profiler spy: [ ...workload... ]
```

runs the block sampled, then prints a classic tally — total samples,
interval, then `self% total% name` rows sorted by self-samples, followed by
a VM section: GC share, top primitives, cache hit rates, allocation volume.
`Profiler counters` prints the counter table alone. Reports print through
`Transcript`, so profiling the compiler is just a corpus-style program:

```smalltalk
Profiler spy: [ | b | b := StImageBuilder new.
    b fileInFile: '...kernel.st'. b programSource: '...'. b imageBytes ]
```

## 5. Native profiling + benchmark harness

For the VM's own code the right tool is the CPU profiler, not something
in-VM — the deliverable is a paved road:

- release profile gains `force-frame-pointers`; `docs/profiling.md`
  documents `perf record -g` / `cargo flamegraph` invocations against the
  standard workloads, and how to read interpreter loops in the output.
- `bench/` gets fixed workloads: **self-compile** (the real one), the
  corpus suite, and Smalltalk microbenchmarks isolating one mechanism each
  (send loop, arithmetic loop, block `value:` loop, OrderedCollection and
  Dictionary churn, String building, exception raise/handle loop, process
  ping-pong). Each prints its own timing via the clock primitive.
- a runner (`make bench`) executes everything median-of-5 on the release
  build with fixed heap sizes, prints wall times **and** the VM counter
  table, runs the same microbenchmarks under GST for a ratio column, and
  appends a row to `bench/history.csv` so regressions and wins are visible
  over time.

Methodology rules, stated once and enforced by the runner: release build
only; one discarded warmup run, then median of ≥5 with min and IQR
reported, and runs flagged when spread exceeds 5 %; fixed heaps; optional
CPU pinning via `taskset` documented for serious sessions; never report a
time without the counters that explain it; measure profiler overhead
itself (self-compile with sampling on vs off — budget ≤ 10 %, and the
*off* configuration must be indistinguishable from today's binary).

**Truth-source cross-validation**: the instruments are themselves tested
against workloads with computable ground truth — a loop of exactly N sends
must show ~N in the send counter; a synthetic two-phase program that spends
90 %/10 % of its bytecode in two methods must sample within a few points of
90/10. An instrument that can't reproduce a known answer doesn't get to
produce unknown ones.

## 6. Tests (first, as always)

- **Sampler unit tests, deterministic**: a `force_sample_at_next_poll()`
  test hook replaces the timer; run a two-method recursive workload and
  assert both selectors appear with sane self/total relations; assert block
  frames render as `[] in ...`; assert GC pseudo-frames appear under an
  allocation-churn workload.
- **Counter exactness**: fixed hand-assembled programs with known send/
  allocation counts assert exact counter values (e.g. N sends → N inline-
  cache probes, first-call miss then hits).
- **Primitive/API tests**: report primitives return well-formed rows;
  start/stop idempotence; gate on/off changes only gated counters.
- **Timer integration**: sampling a spinning Smalltalk loop for 100 ms
  yields > 0 samples and stops cleanly.
- **Acceptance (fuzzy)**: profile the in-image compiler; assert the report
  mentions selectors we *know* must be hot (e.g. `nextPut:`, `at:`) without
  pinning percentages — statistical output stays out of the deterministic
  corpus.
- **No-regression**: full suite + phase-5 bit-identity with profiler off;
  overhead measurement asserted under a generous ceiling.

## 7. Phasing

- **A — counters**: tiered counters + `SMALLISHTALK_STATS` + exactness
  tests. Immediately answers cache-hit-rate/GC/allocation questions.
- **B — sampler**: timer thread, safepoint hook, string-keyed aggregation,
  GC/prim pseudo-frames, deterministic tests.
- **C — Smalltalk API**: treaty band 420–439, kernel `Profiler` +
  report printing, acceptance test.
- **D — bench harness**: workloads, runner, GST ratio table, history file,
  native-profiling docs.
- **E — the findings**: profile the self-compile end-to-end and write up
  the ranked hotspot list with counter evidence. Standing hypotheses to
  confirm or kill: per-byte `WriteStream>>nextPut:` sends (with an
  `isKindOf:` per byte), `String ,` building through per-byte sends,
  O(n) literal-pool dedup and symbol interning in the writer, `Treaty`
  constant lookups on cold caches, every `OrderedCollection>>at:`/`size`
  paying the specialized-send slow path, and GC share under the writer's
  byte-list churn.

Phase E is the actual answer to "where are we slow"; A–D are what make that
answer trustworthy and repeatable.
