"M2 structural tests, GST-side: compile synthetic methods (mock objects
 with Treaty-shaped slots) and validate the output with the *strict*
 disassembler — every emitted byte pattern must be a form the assembler
 owns — plus the Annex J.4 maps invariants. The semantic truth is the
 in-image differential selftest (tests/jit_image_test.rs)."

ByteArray extend [
	byteAt: i [ ^self at: i ]
]

Object subclass: #StJitMockMethod
	instanceVariableNames: 'slots'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Tests'!

!StJitMockMethod class methodsFor: 'instance creation'!
frameSlots: fs argc: argc insns: insnArrays sites: nsites
	| m slots header |
	m := self new.
	header := fs + (argc bitShift: Treaty mhArgcShift).
	slots := Array new: Treaty methodNumSlots.
	slots at: Treaty methodHeader + 1 put: header.
	slots at: Treaty methodBytecodes + 1
		put: (StEncoder bytesForAll: insnArrays) asByteArray.
	slots at: Treaty methodLiterals + 1 put: (Array new: 4).
	slots at: Treaty methodSendSites + 1 put: (Array new: nsites * Treaty siteStride).
	slots at: Treaty methodVmstate + 1 put: 0.
	m setSlots: slots.
	^m! !

!StJitMockMethod methodsFor: 'accessing'!
setSlots: anArray
	slots := anArray!

instVarAt: i
	^slots at: i! !

TestCase subclass: #StMethodCompilerTests
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Tests'!

!StMethodCompilerTests methodsFor: 'helpers'!
mapsHeaderOf: maps
	"{entry. nReentry. nRetpts. nPatch}"
	| u32 |
	u32 := [:i |
		(maps at: 4 * i - 3) + ((maps at: 4 * i - 2) bitShift: 8)
			+ ((maps at: 4 * i - 1) bitShift: 16)
			+ ((maps at: 4 * i) bitShift: 24)].
	^Array with: (u32 value: 1) with: (u32 value: 2)
		with: (u32 value: 3) with: (u32 value: 4)!

compileInsns: insnArrays frameSlots: fs sites: n
	| m |
	m := StJitMockMethod frameSlots: fs argc: 0 insns: insnArrays sites: n.
	^StJITMethodCompiler compile: m! !

!StMethodCompilerTests methodsFor: 'tests'!
testStraightLine
	| pair hdr listing |
	pair := self compileInsns: (OrderedCollection new
			add: #('LOADINT' 1 5);
			add: #('LOADINT' 2 7);
			add: #('ADD' 3 1 2);
			add: #('RET' 3);
			yourself)
		frameSlots: 6 sites: 0.
	self deny: pair isNil.
	"strict decode: every byte pattern is an owned form"
	listing := StAMD64Disassembler disassemble: (pair at: 1) asOrderedCollection.
	self assert: listing size > 20.
	hdr := self mapsHeaderOf: (pair at: 2).
	"the ADD's continuation is a mapped re-entry point (staged
	 specialized-send fallbacks return through it)"
	self assert: hdr = (Array with: 0 with: 1 with: 0 with: 0)!

testLoopHasReentry
	"1 to: n style loop: backward jump target becomes a re-entry point"
	| pair hdr |
	pair := self compileInsns: (OrderedCollection new
			add: #('LOADINT' 1 0);
			add: #('LOADINT' 2 100);
			add: #('LOADINT' 3 0);
			add: #('GT' 4 2 3);
			add: #('JUMPFALSE' 4 3);
			add: #('ADD' 1 1 2);
			add: #('LOADINT' 5 1);
			add: #('SUB' 2 2 5);
			add: #('JUMP' -6);
			add: #('RET' 1);
			yourself)
		frameSlots: 8 sites: 0.
	self deny: pair isNil.
	StAMD64Disassembler disassemble: (pair at: 1) asOrderedCollection.
	hdr := self mapsHeaderOf: (pair at: 2).
	"back edge (pc 3) plus the GT/ADD/SUB continuations (pcs 4, 6, 8)"
	self assert: (hdr at: 2) = 4.
	self assert: (hdr at: 4) = 0!

testSendGetsReturnPoint
	| pair hdr maps rp rpWord |
	pair := self compileInsns: (OrderedCollection new
			add: #('LOADINT' 5 7);
			add: #('SEND' 1 5 0);
			add: #('RET' 1);
			yourself)
		frameSlots: 10 sites: 1.
	self deny: pair isNil.
	StAMD64Disassembler disassemble: (pair at: 1) asOrderedCollection.
	hdr := self mapsHeaderOf: (pair at: 2).
	self assert: (hdr at: 2) = 1. "the send continuation is a re-entry point"
	self assert: (hdr at: 3) = 1. "one returnPoints entry"
	self assert: (hdr at: 4) = 2. "class + target patch records (M3)"
	maps := pair at: 2.
	"retpts[0] follows the header (4 words) and the re-entry pairs"
	rpWord := 4 + (2 * (hdr at: 2)) + 1.
	rp := (maps at: 4 * rpWord - 3) + ((maps at: 4 * rpWord - 2) bitShift: 8)
		+ ((maps at: 4 * rpWord - 1) bitShift: 16)
		+ ((maps at: 4 * rpWord) bitShift: 24).
	self deny: rp = Treaty jitRetpointNone.
	self assert: rp < (pair at: 1) size!

testDeterminism
	"§14: compiling the same method twice yields identical bytes"
	| insns p1 p2 |
	insns := OrderedCollection new
		add: #('LOADINT' 1 5);
		add: #('MUL' 2 1 1);
		add: #('RET' 2);
		yourself.
	p1 := self compileInsns: insns frameSlots: 6 sites: 0.
	p2 := self compileInsns: insns frameSlots: 6 sites: 0.
	self assert: (p1 at: 1) = (p2 at: 1).
	self assert: (p1 at: 2) = (p2 at: 2)!

testAllTemplatesDecode
	"one of each templated opcode through the strict decoder"
	| pair |
	pair := self compileInsns: (OrderedCollection new
			add: #('MOVE' 1 0);
			add: #('LOADK' 1 0);
			add: #('LOADNIL' 1);
			add: #('LOADTRUE' 1);
			add: #('LOADFALSE' 1);
			add: #('LOADSELF' 1);
			add: #('GETIVAR' 1 0);
			add: #('SETIVAR' 0 1);
			add: #('MKBOX' 2 1);
			add: #('GETBOX' 1 2);
			add: #('SETBOX' 2 1);
			add: #('ADD' 1 1 1);
			add: #('SUB' 1 1 1);
			add: #('MUL' 1 1 1);
			add: #('LT' 3 1 1);
			add: #('GT' 3 1 1);
			add: #('LE' 3 1 1);
			add: #('GE' 3 1 1);
			add: #('EQNUM' 3 1 1);
			add: #('IDEQ' 3 1 1);
			add: #('NOT' 3 3);
			add: #('CLASSOF' 3 1);
			add: #('SIZE' 3 1);
			add: #('AT' 3 1 1);
			add: #('ATPUT' 3 1 1);
			add: #('DIV' 3 1 1);
			add: #('MOD' 3 1 1);
			add: #('JUMPTRUE' 3 1);
			add: #('JUMP' 0);
			add: #('RETSELF');
			yourself)
		frameSlots: 8 sites: 0.
	self deny: pair isNil.
	self assert: (StAMD64Disassembler
		disassemble: (pair at: 1) asOrderedCollection) size > 100! !
