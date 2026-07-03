"Build the self-host image with the JIT aboard: kernel + in-image
 Platform + the compiler's own source + the JIT compiler, with the driver
 program as startUp. Args: kernelPath driverProgramPath outPath"
| b |
b := StImageBuilder new.
b fileInFile: (Smalltalk getArgv: 1).
b fileInFile: 'st/selfhost/PlatformImage.st'.
b fileInFile: 'st/compiler/Treaty.st'.
b fileInFile: 'st/compiler/AST.st'.
b fileInFile: 'st/compiler/Lexer.st'.
b fileInFile: 'st/compiler/Parser.st'.
b fileInFile: 'st/compiler/ChunkReader.st'.
b fileInFile: 'st/compiler/CodeGen.st'.
b fileInFile: 'st/compiler/Encoder.st'.
b fileInFile: 'st/compiler/ImageWriter.st'.
b fileInFile: 'st/compiler/Compiler.st'.
b fileInFile: 'st/jit/AMD64Assembler.st'.
b fileInFile: 'st/jit/AMD64Disassembler.st'.
b fileInFile: 'st/jit/AMD64MacroAssembler.st'.
b fileInFile: 'st/jit/MethodCompiler.st'.
b fileInFile: 'st/jit/JIT.st'.
b fileInFile: 'st/jit/JITProfiler.st'.
b programSource: (Platform readFile: (Smalltalk getArgv: 2)).
b writeTo: (Smalltalk getArgv: 3).
Transcript showCr: 'SELFHOST-IMAGE-WRITTEN'.
