"Phase 2 codegen tests (SPEC §20): source → expected mnemonic sequences,
 the same mnemonics as the Rust assembler. Slot-allocator behavior is
 pinned exactly (no peephole optimizer in v1: redundant MOVEs are part of
 the expected output)."

TestCase subclass: #StCodeGenTests
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Tests'!

!StCodeGenTests methodsFor: 'helpers'!
lines: anArray
	| ws |
	ws := WriteStream on: String new.
	anArray do: [:l | ws nextPutAll: l] separatedBy: [ws nextPut: (Character value: 10)].
	^ws contents!

compile: source
	^StCodeGen compileMethodSource: source inClass: StClassScope empty!

compile: source ivars: names
	^StCodeGen compileMethodSource: source
		inClass: (StClassScope name: 'T' instVarNames: names)!

assertMethod: source gives: expectedLines
	self assert: (self compile: source) mnemonics = (self lines: expectedLines)! !

!StCodeGenTests methodsFor: 'tests-basics'!
testReturnLiteral
	self assertMethod: 'foo ^42'
		gives: #('LOADINT 1, 42' 'RET 1')!

testReturnSelf
	self assertMethod: 'foo ^self' gives: #('RETSELF')!

testImplicitReturnSelf
	self assertMethod: 'foo 1' gives: #('LOADINT 1, 1' 'RETSELF')!

testReturnArgument
	self assertMethod: '+ other ^other'
		gives: #('MOVE 2, 1' 'RET 2')!

testBigIntegerGoesToLiteralPool
	| spec |
	spec := self compile: 'foo ^100000'.
	self assert: spec mnemonics = (self lines: #('LOADK 1, 0' 'RET 1')).
	self assert: (spec literals at: 1) = 100000!

testPseudoVariables
	self assertMethod: 'foo ^true' gives: #('LOADTRUE 1' 'RET 1').
	self assertMethod: 'foo ^nil' gives: #('LOADNIL 1' 'RET 1')!

testTempAssignment
	self assertMethod: 'foo | a | a := 1. ^a'
		gives: #('LOADINT 1, 1' 'MOVE 2, 1' 'MOVE 2, 1' 'RET 2')!

testFrameSlots
	self assert: (self compile: 'foo ^42') frameSlots = 2.
	self assert: (self compile: 'foo ^self') frameSlots = 1.
	self assert: (self compile: 'foo ^self bar: 1') frameSlots = 7! !

!StCodeGenTests methodsFor: 'tests-sends'!
testSpecializedAdd
	self assertMethod: 'foo ^3 + 4'
		gives: #('LOADINT 1, 3' 'LOADINT 2, 4' 'ADD 1, 1, 2' 'RET 1')!

testSpecializedUnary
	self assertMethod: 'foo: x ^x size'
		gives: #('MOVE 2, 1' 'SIZE 2, 2' 'RET 2')!

testSpecializedAtPut
	self assertMethod: 'foo: a a at: 1 put: 2'
		gives: #('MOVE 2, 1' 'LOADINT 3, 1' 'LOADINT 4, 2' 'ATPUT 2, 2, 3' 'RETSELF')!

testGeneralSendStagesWithGap
	"receiver at sp+4: the four control-word slots stay free below it"
	self assertMethod: 'foo ^self bar: 1'
		gives: #('LOADSELF 5' 'LOADINT 6, 1' 'SEND 1, 5, 0' 'RET 1')!

testEachCallSiteGetsItsOwnEntry
	| spec |
	spec := self compile: 'foo self bar. self bar. ^nil'.
	self assert: spec sites size = 2.
	self assert: (spec sites at: 1) selector = #bar.
	self assert: (spec sites at: 2) selector = #bar!

testSuperSend
	| spec |
	spec := self compile: 'foo ^super foo'.
	self assert: spec mnemonics
		= (self lines: #('LOADSELF 5' 'SENDSUPER 1, 5, 0' 'RET 1')).
	self assert: (spec sites at: 1) isSuper!

testNestedSendArguments
	"inner send staged above the outer's pending receiver+args"
	self assertMethod: 'foo ^self bar: (self baz)'
		gives: #('LOADSELF 5' 'LOADSELF 10' 'SEND 6, 10, 0' 'SEND 1, 5, 1' 'RET 1')!

testCascade
	| spec |
	spec := self compile: 'foo | c | c := 1. c add: 2; add: 3. ^c'.
	self assert: spec mnemonics = (self lines: #(
		'LOADINT 1, 1' 'MOVE 2, 1'
		'MOVE 2, 1'
		'MOVE 7, 2' 'LOADINT 8, 2' 'SEND 3, 7, 0'
		'MOVE 7, 2' 'LOADINT 8, 3' 'SEND 2, 7, 1'
		'MOVE 2, 1' 'RET 2')).
	self assert: spec sites size = 2!

testGlobalRead
	| spec |
	spec := self compile: 'foo ^Transcript'.
	self assert: spec mnemonics
		= (self lines: #('LOADK 5, 0' 'SEND 1, 5, 0' 'RET 1')).
	self assert: (spec literals at: 1) name = 'Transcript'.
	self assert: (spec sites at: 1) selector = #value!

testLiteralDedupKeepsClassesApart
	| spec |
	spec := self compile: 'foo self a: ''x''. self b: #x. ^nil'.
	"the string 'x' and the symbol #x must be distinct literals"
	self assert: spec literals size = 2! !

!StCodeGenTests methodsFor: 'tests-control'!
testIfTrueIfFalse
	self assertMethod: 'foo ^1 < 2 ifTrue: [''y''] ifFalse: [''n'']'
		gives: #('LOADINT 1, 1' 'LOADINT 2, 2' 'LT 1, 1, 2'
			'JUMPFALSE 1, 2' 'LOADK 1, 0' 'JUMP 1' 'LOADK 1, 1' 'RET 1')!

testIfTrueAlone
	self assertMethod: 'foo ^true ifTrue: [1]'
		gives: #('LOADTRUE 1' 'JUMPFALSE 1, 2' 'LOADINT 1, 1'
			'JUMP 1' 'LOADNIL 1' 'RET 1')!

testAnd
	self assertMethod: 'foo ^true and: [false]'
		gives: #('LOADTRUE 1' 'JUMPFALSE 1, 2' 'LOADFALSE 1'
			'JUMP 1' 'LOADFALSE 1' 'RET 1')!

testOr
	self assertMethod: 'foo ^false or: [true]'
		gives: #('LOADFALSE 1' 'JUMPTRUE 1, 2' 'LOADTRUE 1'
			'JUMP 1' 'LOADTRUE 1' 'RET 1')!

testWhileLoop
	self assertMethod: 'foo | s i | s := 0. i := 3. [i > 0] whileTrue: [s := s + i. i := i - 1]. ^s'
		gives: #(
			'LOADINT 1, 0' 'MOVE 3, 1'
			'LOADINT 2, 3' 'MOVE 3, 2'
			'MOVE 3, 2' 'LOADINT 4, 0' 'GT 3, 3, 4'
			'JUMPFALSE 3, 9'
			'MOVE 3, 1' 'MOVE 4, 2' 'ADD 1, 3, 4' 'MOVE 3, 1'
			'MOVE 3, 2' 'LOADINT 4, 1' 'SUB 2, 3, 4' 'MOVE 3, 2'
			'JUMP -13'
			'LOADNIL 3'
			'MOVE 3, 1' 'RET 3')!

testNonLiteralBlockArgIsRealSend
	"ifTrue: with a non-block argument compiles as an ordinary send"
	| spec |
	spec := self compile: 'foo: b ^true ifTrue: b'.
	self assert: (spec sites at: 1) selector = #ifTrue:! !

!StCodeGenTests methodsFor: 'tests-pragmas'!
testPrimitivePragma
	| spec |
	spec := self compile: 'size <primitive: 8> ^0'.
	self assert: spec primitive = 8.
	self assert: spec mnemonics
		= (self lines: #('PRIM 8' 'LOADINT 2, 0' 'RET 2')).
	"failure-code slot 1 is reserved: fallback scratch starts at 2"
	self assert: spec frameSlots = 3!

testPrimitiveWithDeclaredTempAliasesFailureCode
	| spec |
	spec := self compile: 'bar: x <primitive: 100> | code | ^code'.
	self assert: spec mnemonics
		= (self lines: #('PRIM 100' 'MOVE 3, 2' 'RET 3'))!

testHandlerFramePragma
	| spec |
	spec := self compile: 'on: c do: b <handlerFrame> | hc hb hs | ^nil'.
	self assert: spec handlerSlotBase = 3.
	self assert: spec mhFlags = Treaty mhFlagIsHandler!

testEnsureFramePragma
	| spec |
	spec := self compile: 'ensure: b <ensureFrame> | eb et es ev | ^nil'.
	self assert: spec handlerSlotBase = 2.
	self assert: spec mhFlags = Treaty mhFlagIsEnsure! !

!StCodeGenTests methodsFor: 'tests-ivars'!
testIvarReadWrite
	| scope |
	scope := OrderedCollection new.
	scope add: 'x'; add: 'y'.
	self assert: (self compile: 'getY ^y' ivars: scope) mnemonics
		= (self lines: #('GETIVAR 1, 1' 'RET 1')).
	self assert: (self compile: 'setX: v x := v. ^self' ivars: scope) mnemonics
		= (self lines: #('MOVE 2, 1' 'SETIVAR 0, 2' 'RETSELF'))! !
