"Build a corpus image: kernel chunk file + program source → STIM image.
 Args: kernelPath programPath outPath"
| b |
b := StImageBuilder new.
b fileInFile: (Smalltalk getArgv: 1).
b programSource: (Platform readFile: (Smalltalk getArgv: 2)).
b writeTo: (Smalltalk getArgv: 3).
Transcript showCr: 'IMAGE-WRITTEN'.
