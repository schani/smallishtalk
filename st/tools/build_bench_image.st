"Build a benchmark image: kernel + workload chunk files + driver source →
 STIM image. Args: kernelPath workloadPath driverPath outPath"
| b |
b := StImageBuilder new.
b fileInFile: (Smalltalk getArgv: 1).
b fileInFile: (Smalltalk getArgv: 2).
b programSource: (Platform readFile: (Smalltalk getArgv: 3)).
b writeTo: (Smalltalk getArgv: 4).
Transcript showCr: 'IMAGE-WRITTEN'.
