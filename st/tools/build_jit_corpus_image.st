"Build a corpus image with the in-image JIT aboard (JIT.md §18.3, Phase 6
 modes): kernel + Treaty + JIT compiler sources + the program, prefixed
 with a tiering boot (threshold + background JIT process). The boot is
 inserted after the program's leading temp declaration when present.
 Args: kernelPath programPath outPath threshold"
| b src boot i j wrapped |
b := StImageBuilder new.
b fileInFile: (Smalltalk getArgv: 1).
b fileInFile: 'st/compiler/Treaty.st'.
b fileInFile: 'st/jit/AMD64Assembler.st'.
b fileInFile: 'st/jit/AMD64Disassembler.st'.
b fileInFile: 'st/jit/AMD64MacroAssembler.st'.
b fileInFile: 'st/jit/MethodCompiler.st'.
b fileInFile: 'st/jit/JIT.st'.
b fileInFile: 'st/jit/JITProfiler.st'.
src := Platform readFile: (Smalltalk getArgv: 2).
boot := 'StJIT threshold: ' , (Smalltalk getArgv: 4)
	, '. StJIT startBackgroundCompiler. '.
i := 1.
"skip leading whitespace and comments to find a temp declaration"
[i <= src size and: [(src at: i) isSeparator or: [(src at: i) = $"]]] whileTrue: [
	(src at: i) = $"
		ifTrue: [
			i := i + 1.
			[(src at: i) ~= $"] whileTrue: [i := i + 1].
			i := i + 1]
		ifFalse: [i := i + 1]].
(i <= src size and: [(src at: i) = $|])
	ifTrue: [
		j := i + 1.
		[(src at: j) ~= $|] whileTrue: [j := j + 1].
		wrapped := (src copyFrom: 1 to: j) , ' ' , boot
			, (src copyFrom: j + 1 to: src size)]
	ifFalse: [wrapped := boot , src].
b programSource: wrapped.
b writeTo: (Smalltalk getArgv: 3).
Transcript showCr: 'IMAGE-WRITTEN'.
