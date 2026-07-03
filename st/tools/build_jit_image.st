"Build the JIT test image: kernel + Treaty + the in-image JIT compiler +
 the selftest classes, with the given program as startUp.
 Args: kernelPath programPath outPath"
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
b fileInFile: 'st/jit/JitSelftest.st'.
b programSource: (Platform readFile: (Smalltalk getArgv: 2)).
b writeTo: (Smalltalk getArgv: 3).
Transcript showCr: 'IMAGE-WRITTEN'.
