# Profiling smallishtalk

Three instruments, one system (see `docs/profiling-plan.md` for the design):
the in-image sampling profiler, the exact VM counters, and the native
CPU-profiler workflow + benchmark harness described here.

## The Smalltalk-facing profiler

Inside any image built with the kernel:

```smalltalk
Profiler spy: [ ...workload... ].          "sample at 1 ms, print a tally"
Profiler spy: [ ... ] interval: 5.         "custom interval"
Profiler counters.                          "print the VM counter table"
Profiler gate: true.                        "enable per-opcode/send counters"
Profiler resetCounters.
```

The tally prints `self% total% name` rows sorted by self-samples; VM time
shows up as pseudo-leaves `<vm:scavenge>`, `<vm:compact>`, `<vm:prim:N>`
*on top of* the Smalltalk stacks that triggered it, so GC pressure is
attributed to the code that allocates.

Primitives (Treaty band 420–439): 420 start, 421 stop, 422 report,
424 counters, 425 reset, 426 gate.

## Counters without touching image code

Any run can double as a measurement:

```sh
SMALLISHTALK_STATS=1 target/release/smallishtalk image.im   # table on exit (stderr)
SMALLISHTALK_GATE=1  ...                                    # + per-opcode histogram,
                                                            #   send counts, poll-gap stats
```

GC pause time is `gc.scavenge.ns + gc.compact.ns` — exact, always on.
`poll.gap_max` / `poll.gap_le_*` bound the sampler's attribution error
(instructions executed between safepoint polls).

The gated tier (per-opcode histogram, send count, poll gaps) requires a
`--features vm-counters` build. This was a measured decision (plan §3
validation): even switched *off*, the runtime gate branch on the dispatch
loop cost ~10% on the self-compile, so the gate moved to compile time —
default builds carry no gate at all, and the always-on tier measures as
free (self-compile time is identical to the pre-instrumentation binary).
`make bench` builds both binaries: timed runs use the gate-free one,
counter captures the instrumented one.

## Benchmarks

```sh
make bench                       # all workloads + self-compile
make bench BENCH_ARGS="send_loop dictionary"
RUNS=9 bash bench/run.sh         # more samples
```

Per workload: release build, one discarded warmup, median of 5 with min and
IQR (flagged `noisy` above 5%), the same source timed under GST 3.2.5 for a
ratio column, and checksum cross-validation between the two hosts (for
self-compile the checksum is the output image's byte size — bit-identity
doing double duty). Rows append to `bench/history.csv`; each workload's full
counter table lands in `bench/results/<name>.counters.txt`.

Workloads live in `bench/workloads/*.st` in the portable dialect (they load
under both hosts unchanged); timing is in the per-host drivers
(`bench/vm_driver.st`, `bench/gst_driver.st`), not the workloads.

## Native profiling (the VM's own code)

Release builds carry line tables (`debug = "line-tables-only"`). For call
graphs, build with frame pointers:

```sh
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --release
perf record -g --call-graph fp target/release/smallishtalk image.im
perf report
# or
cargo install flamegraph
RUSTFLAGS="-C force-frame-pointers=yes" cargo flamegraph --release -- image.im
```

Use the bench images as standard workloads: run `make bench` once, or build
one directly:

```sh
gst -Q st/compiler/{Compat,Treaty,Platform,AST,Lexer,Parser,ChunkReader,CodeGen,Encoder,ImageWriter,Compiler}.st \
    st/tools/build_bench_image.st -a st/kernel/kernel.st \
    bench/workloads/send_loop.st bench/vm_driver.st /tmp/send_loop.im
perf record -g target/release/smallishtalk /tmp/send_loop.im
```

Reading the interpreter in `perf` output: almost everything inlines into
`Vm::run_rooted` (the dispatch loop). Look at *annotated assembly*
(`perf annotate`) rather than the function ranking — the interesting split
is dispatch vs. `do_send`/`lookup_cached` (send machinery), `alloc_gc`
(allocation), and `collect_young` (GC). The exact counters usually answer
"why is this hot" faster than staring at assembly: never report a time
without the counter table that explains it.

## Methodology rules

- Release build only; debug numbers are meaningless (≈40× slower).
- One warmup run, median of ≥5, report min and IQR; distrust anything
  flagged `noisy`.
- Fixed heap sizes (the defaults); note deviations with the numbers.
- For serious sessions pin the CPU: `taskset -c 2 bash bench/run.sh`.
- Measure profiler overhead when sampling matters: self-compile with
  `Profiler spy:` on vs. off — budget ≤ 10%, and profiler-off must be
  indistinguishable from a build that never had it.
