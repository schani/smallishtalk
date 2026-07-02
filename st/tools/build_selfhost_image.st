"Build the self-host image: kernel + in-image Platform + the compiler's
 own source, with the driver program as startUp.
 Args: kernelPath driverProgramPath outPath"
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
b programSource: (Platform readFile: (Smalltalk getArgv: 2)).
b writeTo: (Smalltalk getArgv: 3).
Transcript showCr: 'SELFHOST-IMAGE-WRITTEN'.
