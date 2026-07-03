| t0 r |
"tiering on; two warmup runs trip and compile the hot code, the drain
 barrier finishes anything still queued, then one timed run (JIT.md §18.8)"
StJIT threshold: 2.
StJIT startBackgroundCompiler.
Bench run.
Bench run.
StJITCompiler drain.
t0 := Profiler primClockMs.
r := Bench run.
Transcript showCr: 'MS ' , (Profiler primClockMs - t0) printString.
Transcript showCr: 'CHECK ' , r printString
