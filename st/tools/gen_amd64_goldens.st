"Dump the M1 golden catalog for the Rust/objdump cross-check
 (tests/amd64_cross_test.rs). Args: outPath.
 Line format: name|hex|disasm (newlines in the disasm joined with ';')."
| out |
out := WriteStream on: String new.
StAMD64Goldens do: [:name :bytes :text |
	| digits |
	digits := '0123456789ABCDEF'.
	out nextPutAll: name; nextPut: $|.
	bytes do: [:b |
		out nextPut: (digits charAt: (b bitShift: -4) + 1).
		out nextPut: (digits charAt: (b bitAnd: 15) + 1)].
	out nextPut: $|.
	text do: [:ch |
		ch = Character lf
			ifTrue: [out nextPut: $;]
			ifFalse: [out nextPut: ch]].
	out nextPut: Character lf].
Platform writeBytes: (Platform bytesOf: out contents)
	toFile: (Smalltalk getArgv: 1).
Transcript showCr: 'GOLDENS-WRITTEN'.
