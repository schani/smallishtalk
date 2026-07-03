"Build a JIT benchmark image: kernel + JIT compiler + workload chunk +
 driver source. Args: kernelPath workloadPath driverPath outPath"
| b |
b := StImageBuilder new.
b fileInFile: (Smalltalk getArgv: 1).
b fileInFile: 'st/compiler/Treaty.st'.
b fileInFile: 'st/jit/AMD64Assembler.st'.
b fileInFile: 'st/jit/AMD64Disassembler.st'.
b fileInFile: 'st/jit/AMD64MacroAssembler.st'.
b fileInFile: 'st/jit/MethodCompiler.st'.
b fileInFile: 'st/jit/JIT.st'.
b fileInFile: 'st/jit/JITProfiler.st'.
b fileInFile: (Smalltalk getArgv: 2).
b programSource: (Platform readFile: (Smalltalk getArgv: 3)).
b writeTo: (Smalltalk getArgv: 4).
Transcript showCr: 'IMAGE-WRITTEN'.
