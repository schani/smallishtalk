| t0 r |
t0 := Profiler primClockMs.
r := Bench run.
Transcript showCr: 'MS ' , (Profiler primClockMs - t0) printString.
Transcript showCr: 'CHECK ' , r printString
