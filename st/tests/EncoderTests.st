"Phase 2 encoder tests: golden u32s from the Treaty (the same values the
 Rust asm tests pin), plus byte-order checks."

TestCase subclass: #StEncoderTests
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Tests'!

!StEncoderTests methodsFor: 'tests'!
testAbcGolden
	"the Phase-0 golden: ADD d=4,a=0,b=1"
	self assert: (StEncoder encode: #('ADD' 4 0 1)) = 16r01000430.
	self assert: (StEncoder encode: #('MOVE' 2 7)) = 16r00070210.
	self assert: (StEncoder encode: #('SEND' 1 2 3))
		= (Treaty opSend + (1 * 256) + (2 * 65536) + (3 * 16777216))!

testAdGolden
	self assert: (StEncoder encode: #('LOADK' 3 16r1234))
		= (Treaty opLoadk + (3 * 256) + (16r1234 * 65536)).
	self assert: (StEncoder encode: #('JUMP' -1))
		= (Treaty opJump + (16rFFFF * 65536)).
	self assert: (StEncoder encode: #('JUMPTRUE' 5 -2))
		= (Treaty opJumptrue + (5 * 256) + (16rFFFE * 65536)).
	self assert: (StEncoder encode: #('LOADINT' 0 -7))
		= (Treaty opLoadint + (((0 - 7) bitAnd: 16rFFFF) * 65536))!

testNoOperand
	self assert: (StEncoder encode: #('RETSELF')) = Treaty opRetself.
	self assert: (StEncoder encode: #('RET' 25))
		= (Treaty opRet + (25 * 256)).
	self assert: (StEncoder encode: #('PRIM' 4095))
		= (Treaty opPrim + (4095 * 65536))!

testLittleEndianBytes
	| bytes |
	bytes := StEncoder bytesForAll: (Array with: #('ADD' 4 0 1)).
	self assert: bytes asArray = #(16r30 16r04 16r00 16r01)!

testCompiledMethodEncodes
	"end to end: compile source, encode every instruction"
	| spec words |
	spec := StCodeGen compileMethodSource: 'foo ^3 + 4' inClass: StClassScope empty.
	words := StEncoder encodeAll: spec insns.
	self assert: words size = 4.
	self assert: (words at: 3) = 16r02010130	"ADD 1, 1, 2"! !
