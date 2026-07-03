"AMD64 disassembler for exactly the StAMD64Assembler subset (JIT.md §13:
 each assembler ships with a disassembler for its subset, round-trip
 tested — the debugging tool for compiled methods). Strict by design: any
 byte sequence the assembler cannot produce is an error, so drift between
 the two is caught immediately.

 Output: one string per instruction, objdump-compatible mnemonics,
 decimal displacements/immediates, e.g. 'mov rbx, [r14+8]'."

Object subclass: #StAMD64Disassembler
	instanceVariableNames: 'bytes pos rexW rexR rexX rexB'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-JIT'!

!StAMD64Disassembler class methodsFor: 'disassembling'!
disassemble: aByteCollection
	"answer an OrderedCollection of instruction strings"
	^self new disassembleAll: aByteCollection!

text: aByteCollection
	"the whole listing joined with newlines"
	| out first |
	out := WriteStream on: String new.
	first := true.
	(self disassemble: aByteCollection) do: [:line |
		first ifFalse: [out nextPut: Character lf].
		out nextPutAll: line.
		first := false].
	^out contents! !

!StAMD64Disassembler methodsFor: 'access'!
disassembleAll: aByteCollection
	| out |
	bytes := aByteCollection.
	pos := 1.
	out := OrderedCollection new.
	[pos <= bytes size] whileTrue: [out add: self decodeOne].
	^out!

next
	| b |
	pos > bytes size ifTrue: [self error: 'truncated instruction'].
	b := bytes at: pos.
	pos := pos + 1.
	^b!

nextU32
	| v |
	v := self next.
	v := v + (self next bitShift: 8).
	v := v + (self next bitShift: 16).
	v := v + (self next bitShift: 24).
	^v!

nextS32
	| v |
	v := self nextU32.
	v >= 16r80000000 ifTrue: [v := v - 16r100000000].
	^v!

nextU64
	| lo |
	lo := self nextU32.
	^lo + (self nextU32 bitShift: 32)! !

!StAMD64Disassembler methodsFor: 'naming'!
reg64: n
	^#('rax' 'rcx' 'rdx' 'rbx' 'rsp' 'rbp' 'rsi' 'rdi'
	   'r8' 'r9' 'r10' 'r11' 'r12' 'r13' 'r14' 'r15') at: n + 1!

reg32: n
	^#('eax' 'ecx' 'edx' 'ebx' 'esp' 'ebp' 'esi' 'edi'
	   'r8d' 'r9d' 'r10d' 'r11d' 'r12d' 'r13d' 'r14d' 'r15d') at: n + 1!

regW: n
	^rexW = 1 ifTrue: [self reg64: n] ifFalse: [self reg32: n]!

condName: code
	^#('o' 'no' 'b' 'ae' 'e' 'ne' 'be' 'a' 's' 'ns' 'p' 'np'
	   'l' 'ge' 'le' 'g') at: code + 1!

dispString: d
	d = 0 ifTrue: [^'+0'].
	d < 0 ifTrue: [^'-' , d negated printString].
	^'+' , d printString! !

!StAMD64Disassembler methodsFor: 'decoding'!
decodeOne
	| b |
	rexW := 0. rexR := 0. rexX := 0. rexB := 0.
	b := self next.
	(b >= 16r40 and: [b <= 16r4F]) ifTrue: [
		rexW := (b bitShift: -3) bitAnd: 1.
		rexR := (b bitShift: -2) bitAnd: 1.
		rexX := (b bitShift: -1) bitAnd: 1.
		rexB := b bitAnd: 1.
		b := self next].
	b = 16r89 ifTrue: [^self decodeStore].
	b = 16r8B ifTrue: [^self decodeLoad].
	b = 16r3B ifTrue: [^self decodeCmpLoad].
	b = 16r8D ifTrue: [^self decodeLea].
	b = 16rC7 ifTrue: [^self decodeMovImm32].
	(b >= 16rB8 and: [b <= 16rBF]) ifTrue: [^self decodeMovRegImm: b - 16rB8].
	b = 16r01 ifTrue: [^self decodeAlu: 'add'].
	b = 16r29 ifTrue: [^self decodeAlu: 'sub'].
	b = 16r09 ifTrue: [^self decodeAlu: 'or'].
	b = 16r31 ifTrue: [^self decodeAlu: 'xor'].
	b = 16r21 ifTrue: [^self decodeAlu: 'and'].
	b = 16r39 ifTrue: [^self decodeAlu: 'cmp'].
	b = 16r85 ifTrue: [^self decodeAlu: 'test'].
	b = 16r81 ifTrue: [^self decodeGroup1Imm32].
	b = 16r83 ifTrue: [^self decodeGroup1Imm8].
	b = 16rC1 ifTrue: [^self decodeShift].
	b = 16rF7 ifTrue: [^self decodeGroup3].
	b = 16r99 ifTrue: [
		rexW = 1 ifTrue: [^'cqo'].
		^self error: 'cdq not in subset'].
	b = 16r88 ifTrue: [
		| m |
		m := self modrm.
		(m at: 1) = 2 ifFalse: [^self error: 'mov8: unsupported mod'].
		^'mov byte ' , (self memOperand: (m at: 3)) , ', '
			, (#('al' 'cl' 'dl' 'bl' 'spl' 'bpl' 'sil' 'dil'
				'r8b' 'r9b' 'r10b' 'r11b' 'r12b' 'r13b' 'r14b' 'r15b')
				at: (m at: 2) + 1)].
	b = 16rE9 ifTrue: [^'jmp ' , (self branchTarget: self nextS32)].
	b = 16rFF ifTrue: [^self decodeFF].
	b = 16r80 ifTrue: [^self decodeCmpByte].
	b = 16rCC ifTrue: [^'int3'].
	b = 16r0F ifTrue: [^self decodeTwoByte].
	^self error: 'unknown opcode ' , b printString!

decodeTwoByte
	| b |
	b := self next.
	b = 16r0B ifTrue: [^'ud2'].
	b = 16rAF ifTrue: [^self decodeImul].
	b = 16rB6 ifTrue: [
		| m |
		m := self modrm.
		(m at: 1) = 2 ifFalse: [^self error: 'movzx: unsupported mod'].
		^'movzx ' , (self regW: (m at: 2)) , ', byte ' , (self memOperand: (m at: 3))].
	(b >= 16r80 and: [b <= 16r8F]) ifTrue: [
		^'j' , (self condName: b - 16r80) , ' ' , (self branchTarget: self nextS32)].
	^self error: 'unknown 0F opcode ' , b printString!

branchTarget: rel
	"absolute 0-based offset of the target (pos is already past the rel32)"
	^'0x' , (self hex: pos - 1 + rel)!

hex: n
	| digits v out |
	n = 0 ifTrue: [^'0'].
	n < 0 ifTrue: [^'-' , (self hex: n negated)].
	digits := '0123456789abcdef'.
	v := n.
	out := OrderedCollection new.
	[v > 0] whileTrue: [
		out addFirst: (digits charAt: (v bitAnd: 15) + 1).
		v := v bitShift: -4].
	^String withAll: out asArray!

modrm
	"answer {mode. reg. rm} with REX extensions applied to reg only;
	 rm extension is applied by the memory/register decoders"
	| m |
	m := self next.
	^Array with: (m bitShift: -6)
		with: ((m bitShift: -3) bitAnd: 7) + (rexR * 8)
		with: (m bitAnd: 7)!

memOperand: rm
	"mod=10 memory operand; answers the '[...]' string"
	| base idx scale disp sib |
	rm = 4
		ifTrue: [
			sib := self next.
			scale := sib bitShift: -6.
			idx := ((sib bitShift: -3) bitAnd: 7) + (rexX * 8).
			base := (sib bitAnd: 7) + (rexB * 8).
			disp := self nextS32.
			(idx bitAnd: 7) = 4
				ifTrue: [
					"no-index SIB (base rsp/r12)"
					^'[' , (self reg64: base) , (self dispString: disp) , ']']
				ifFalse: [
					^'[' , (self reg64: base) , '+' , (self reg64: idx)
						, '*' , (1 bitShift: scale) printString
						, (self dispString: disp) , ']']]
		ifFalse: [
			base := rm + (rexB * 8).
			disp := self nextS32.
			^'[' , (self reg64: base) , (self dispString: disp) , ']']!

decodeStore
	| m |
	m := self modrm.
	(m at: 1) = 3 ifTrue: [
		^'mov ' , (self regW: (m at: 3) + (rexB * 8)) , ', ' , (self regW: (m at: 2))].
	(m at: 1) = 2 ifTrue: [
		^'mov ' , (self memOperand: (m at: 3)) , ', ' , (self regW: (m at: 2))].
	^self error: 'mov store: unsupported mod'!

decodeLoad
	| m |
	m := self modrm.
	(m at: 1) = 2 ifTrue: [
		^'mov ' , (self regW: (m at: 2)) , ', ' , (self memOperand: (m at: 3))].
	^self error: 'mov load: unsupported mod'!

decodeCmpLoad
	| m |
	m := self modrm.
	(m at: 1) = 2 ifTrue: [
		^'cmp ' , (self regW: (m at: 2)) , ', ' , (self memOperand: (m at: 3))].
	^self error: 'cmp load: unsupported mod'!

decodeLea
	| m |
	m := self modrm.
	(m at: 1) = 2 ifTrue: [
		^'lea ' , (self regW: (m at: 2)) , ', ' , (self memOperand: (m at: 3))].
	^self error: 'lea: unsupported mod'!

decodeMovImm32
	| m |
	m := self modrm.
	((m at: 2) bitAnd: 7) = 0 ifFalse: [^self error: 'C7 /n: not /0'].
	(m at: 1) = 2 ifTrue: [
		| mem |
		mem := self memOperand: (m at: 3).
		^'mov qword ' , mem , ', ' , self nextS32 printString].
	(m at: 1) = 3 ifTrue: [
		^'mov ' , (self regW: (m at: 3) + (rexB * 8)) , ', ' , self nextS32 printString].
	^self error: 'C7: unsupported mod'!

decodeMovRegImm: low
	| r |
	r := low + (rexB * 8).
	rexW = 1 ifTrue: [^'movabs ' , (self reg64: r) , ', ' , self nextU64 printString].
	^'mov ' , (self reg32: r) , ', ' , self nextU32 printString!

decodeAlu: name
	| m |
	m := self modrm.
	(m at: 1) = 3 ifTrue: [
		^name , ' ' , (self regW: (m at: 3) + (rexB * 8)) , ', ' , (self regW: (m at: 2))].
	^self error: name , ': unsupported mod'!

decodeImul
	| m |
	m := self modrm.
	(m at: 1) = 3 ifTrue: [
		^'imul ' , (self regW: (m at: 2)) , ', ' , (self regW: (m at: 3) + (rexB * 8))].
	^self error: 'imul: unsupported mod'!

group1Name: ext
	ext = 0 ifTrue: [^'add'].
	ext = 1 ifTrue: [^'or'].
	ext = 4 ifTrue: [^'and'].
	ext = 5 ifTrue: [^'sub'].
	ext = 7 ifTrue: [^'cmp'].
	^self error: 'group1 ext ' , ext printString!

decodeGroup1Imm32
	| m name |
	m := self modrm.
	name := self group1Name: ((m at: 2) bitAnd: 7).
	(m at: 1) = 3 ifTrue: [
		^name , ' ' , (self regW: (m at: 3) + (rexB * 8)) , ', ' , self nextS32 printString].
	^self error: '81: unsupported mod'!

decodeGroup1Imm8
	| m name v |
	m := self modrm.
	name := self group1Name: ((m at: 2) bitAnd: 7).
	(m at: 1) = 3 ifFalse: [^self error: '83: unsupported mod'].
	v := self next.
	v >= 128 ifTrue: [v := v - 256].
	^name , ' ' , (self regW: (m at: 3) + (rexB * 8)) , ', ' , v printString!

decodeShift
	| m ext name |
	m := self modrm.
	(m at: 1) = 3 ifFalse: [^self error: 'C1: unsupported mod'].
	ext := (m at: 2) bitAnd: 7.
	ext = 4 ifTrue: [name := 'shl'].
	ext = 5 ifTrue: [name := 'shr'].
	ext = 7 ifTrue: [name := 'sar'].
	name isNil ifTrue: [^self error: 'C1 ext ' , ext printString].
	^name , ' ' , (self regW: (m at: 3) + (rexB * 8)) , ', ' , self next printString!

decodeGroup3
	| m ext r |
	m := self modrm.
	ext := (m at: 2) bitAnd: 7.
	(m at: 1) = 3 ifFalse: [^self error: 'F7: unsupported mod'].
	r := self regW: (m at: 3) + (rexB * 8).
	ext = 0 ifTrue: [^'test ' , r , ', ' , self nextS32 printString].
	ext = 3 ifTrue: [^'neg ' , r].
	ext = 7 ifTrue: [^'idiv ' , r].
	^self error: 'F7 ext ' , ext printString!

decodeFF
	| m |
	m := self modrm.
	((m at: 2) bitAnd: 7) = 4 ifTrue: [
		(m at: 1) = 3 ifTrue: [^'jmp ' , (self reg64: (m at: 3) + (rexB * 8))].
		(m at: 1) = 2 ifTrue: [^'jmp qword ' , (self memOperand: (m at: 3))].
		^self error: 'FF /4: unsupported mod'].
	((m at: 2) bitAnd: 7) = 2 ifTrue: [
		(m at: 1) = 2 ifTrue: [^'call qword ' , (self memOperand: (m at: 3))].
		^self error: 'FF /2: unsupported mod'].
	^self error: 'FF ext ' , (m at: 2) printString!

decodeCmpByte
	| m mem |
	m := self modrm.
	((m at: 2) bitAnd: 7) = 7 ifFalse: [^self error: '80: not /7'].
	(m at: 1) = 2 ifFalse: [^self error: '80: unsupported mod'].
	mem := self memOperand: (m at: 3).
	^'cmp byte ' , mem , ', ' , self next printString! !
