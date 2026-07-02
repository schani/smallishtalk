"GST-side timing driver: loaded after a workload file; prints the same
 MS / CHECK lines as bench/vm_driver.st does inside the VM image."
| t0 r |
t0 := Time millisecondClockValue.
r := Bench run.
Transcript showCr: 'MS ', (Time millisecondClockValue - t0) printString.
Transcript showCr: 'CHECK ', r printString.
