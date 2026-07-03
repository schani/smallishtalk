"M1 assembler tests (JIT.md §18.1): exact-byte goldens for representative
 forms, full assembler->disassembler round trips over the golden catalog,
 and the label/fixup and patch-offset contracts. The objdump cross-check
 (tests/amd64_cross_test.rs) covers 'self-consistent but wrong'."

TestCase subclass: #StAMD64AssemblerTests
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Tests'!

!StAMD64AssemblerTests methodsFor: 'helpers'!
hexOf: byteCollection
	| digits out |
	digits := '0123456789ABCDEF'.
	out := WriteStream on: String new.
	byteCollection do: [:b |
		out nextPut: (digits charAt: (b bitShift: -4) + 1).
		out nextPut: (digits charAt: (b bitAnd: 15) + 1)].
	^out contents!

asmDo: aBlock
	| a |
	a := StAMD64Assembler new.
	aBlock value: a.
	^self hexOf: a finish! !

!StAMD64AssemblerTests methodsFor: 'tests'!
testExactGoldens
	self assert: (self asmDo: [:a | a movR: 0 fromR: 3]) = '4889D8'.
	self assert: (self asmDo: [:a | a movR: 3 fromM: 14 disp: 8])
		= '498B9E08000000'.
	self assert: (self asmDo: [:a | a movM: 3 disp: 40 fromR: 2])
		= '48899328000000'.
	self assert: (self asmDo: [:a | a leaR: 3 base: 3 index: 1 scale: 3 disp: 8])
		= '488D9CCB08000000'.
	self assert: (self asmDo: [:a | a cmp32R: 11 imm32: 42])
		= '4181FB2A000000'.
	self assert: (self asmDo: [:a | a jmpM: 13 disp: 80])
		= '41FFA550000000'.
	self assert: (self asmDo: [:a | a movR: 0 imm32: 42]) = 'B82A000000'.
	self assert: (self asmDo: [:a | a movR: 2 imm64: 16r0123456789ABCDEF])
		= '48BAEFCDAB8967452301'.
	self assert: (self asmDo: [:a | a addR: 10 imm8: -1]) = '4983C2FF'.
	self assert: (self asmDo: [:a | a cqo]) = '4899'.
	self assert: (self asmDo: [:a | a int3]) = 'CC'!

testStackAndR12Sib
	"rsp/r12 bases force the no-index SIB byte"
	self assert: (self asmDo: [:a | a movR: 1 fromM: 4 disp: 16])
		= '488B8C2410000000'.
	self assert: (self asmDo: [:a | a movR: 1 fromM: 12 disp: 24])
		= '498B8C2418000000'!

testLabelFixups
	| a l bytes |
	a := StAMD64Assembler new.
	l := a newLabel.
	a jmpTo: l.
	a int3.
	a bind: l.
	bytes := a finish.
	"E9 rel32: rel = 6 - 5 = 1"
	self assert: (bytes at: 1) = 16rE9.
	self assert: (bytes at: 2) = 1.
	self assert: (bytes at: 6) = 16rCC.

	a := StAMD64Assembler new.
	l := a newLabel.
	a bind: l.
	a int3.
	a j: #ne to: l.
	bytes := a finish.
	"CC, then 0F 85 rel32 with rel = 0 - 7 = -7"
	self assert: (bytes at: 2) = 16r0F.
	self assert: (bytes at: 3) = 16r85.
	self assert: (bytes at: 4) = 16rF9.
	self assert: (bytes at: 5) = 16rFF.
	self assert: (bytes at: 6) = 16rFF.
	self assert: (bytes at: 7) = 16rFF!

testPatchSiteOffsets
	"the two Annex J.5 patchable shapes answer their immediate offsets"
	| a off bytes l |
	a := StAMD64Assembler new.
	a movR: 0 fromR: 3.
	off := a cmp32R: 11 imm32: 16r11223344.
	bytes := a finish.
	self assert: off = 6. "3 (mov) + 3 (41 81 FB)"
	self assert: (bytes at: off + 1) = 16r44.
	self assert: (bytes at: off + 4) = 16r11.

	a := StAMD64Assembler new.
	l := a newLabel.
	off := a jmpPatchableTo: l.
	a bind: l.
	bytes := a finish.
	self assert: off = 1.
	self assert: (bytes at: 1) = 16rE9!

testRoundTripCatalog
	"assemble -> disassemble every catalog entry; the text must match"
	| failures |
	failures := OrderedCollection new.
	StAMD64Goldens do: [:name :bytes :text |
		| got |
		got := StAMD64Disassembler text: bytes.
		got = text ifFalse: [
			failures add: name , ': expected <' , text , '> got <' , got , '>']].
	self assert: failures isEmpty
		description: (failures inject: '' into: [:acc :f | acc , f , ' | '])!

testStrictDecoder
	"bytes the assembler cannot produce are an error, not a guess"
	self should: [StAMD64Disassembler text: (OrderedCollection with: 16r90)]
		raise: Error!

testTaggedIntHelpersShape
	"the shapes M2 templates lean on: tagged add via lea, untag via sar"
	| a bytes |
	a := StAMD64Assembler new.
	a leaR: 9 base: 1 index: 1 scale: 0 disp: 1.
	a sarR: 9 by: 1.
	bytes := a finish.
	self assert: (StAMD64Disassembler disassemble: bytes) asArray
		= (Array with: 'lea r9, [rcx+rcx*1+1]' with: 'sar r9, 1')! !
