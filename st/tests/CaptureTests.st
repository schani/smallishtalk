"Phase 2 capture-analysis tests (SPEC §20): copy-vs-box classification per
 §10 on a battery of block shapes, plus block codegen and NLR flags."

TestCase subclass: #StCaptureTests
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Tests'!

!StCaptureTests methodsFor: 'helpers'!
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

blockSpecIn: methodSpec
	methodSpec literals do: [:l |
		(l isKindOf: StBlockSpec) ifTrue: [^l]].
	self error: 'no block literal'! !

!StCaptureTests methodsFor: 'tests-classification'!
testPlainBlockNoCaptures
	| spec blk |
	spec := self compile: 'foo ^[42]'.
	self assert: spec mnemonics = (self lines: #('MKCLOSURE 1, 0' 'RET 1')).
	blk := self blockSpecIn: spec.
	self assert: blk numCaptured = 0.
	self assert: blk hasNlr = false.
	self assert: blk mnemonics = (self lines: #('LOADINT 1, 42' 'RET 1'))!

testReadOnlyCaptureIsCopied
	"assigned only before capture → copied at MKCLOSURE time (§10)"
	| spec blk |
	spec := self compile: 'foo | t | t := 5. ^[t]'.
	self assert: spec mnemonics = (self lines: #(
		'LOADINT 1, 5' 'MOVE 2, 1'
		'MKCLOSURE 2, 0' 'CAPTURE 0, 1' 'RET 2')).
	blk := self blockSpecIn: spec.
	self assert: blk numCaptured = 1.
	self assert: blk mnemonics = (self lines: #('GETIVAR 1, 4' 'RET 1'))!

testMutatedInsideBlockIsBoxed
	| spec blk |
	spec := self compile: 'foo | t | t := 5. [t := 7] value. ^t'.
	self assert: spec mnemonics = (self lines: #(
		'MKBOX 1, 1'
		'LOADINT 2, 5' 'SETBOX 1, 2'
		'MKCLOSURE 6, 0' 'CAPTURE 0, 1' 'SEND 2, 6, 0'
		'GETBOX 2, 1' 'RET 2')).
	blk := self blockSpecIn: spec.
	self assert: blk mnemonics = (self lines: #(
		'LOADINT 1, 7' 'GETIVAR 5, 4' 'SETBOX 5, 1' 'RET 1'))!

testAssignedAfterCaptureIsBoxed
	"the mutation happens in the method, after the block was created"
	| spec |
	spec := self compile: 'foo | t b | t := 1. b := [t]. t := 2. ^b'.
	"t is boxed: reads inside the block must see the update"
	self assert: ((spec mnemonics indexOf: $B) > 0). "has MKBOX/SETBOX"
	self assert: (spec mnemonics occurrencesOf: $X) > 0!

testArgumentCaptureIsCopied
	"arguments are assigned at entry, before any capture"
	| spec blk |
	spec := self compile: 'foo: x ^[x]'.
	self assert: spec mnemonics = (self lines: #(
		'MKCLOSURE 2, 0' 'CAPTURE 0, 1' 'RET 2')).
	blk := self blockSpecIn: spec.
	self assert: blk mnemonics = (self lines: #('GETIVAR 1, 4' 'RET 1'))!

testBlockArgsAreLocalToBlock
	| spec blk |
	spec := self compile: 'foo ^[:a :b | a + b]'.
	blk := self blockSpecIn: spec.
	self assert: blk argc = 2.
	self assert: blk numCaptured = 0.
	self assert: blk mnemonics = (self lines: #(
		'MOVE 3, 1' 'MOVE 4, 2' 'ADD 3, 3, 4' 'RET 3'))! !

!StCaptureTests methodsFor: 'tests-self-and-nlr'!
testSelfCaptureAtIndexZero
	| spec blk |
	spec := self compile: 'foo ^[self]'.
	self assert: spec mnemonics = (self lines: #(
		'MKCLOSURE 1, 0' 'CAPTURE 0, 0' 'RET 1')).
	blk := self blockSpecIn: spec.
	self assert: blk numCaptured = 1.
	self assert: blk mnemonics = (self lines: #('GETIVAR 1, 4' 'RET 1'))!

testNlrBlock
	| spec blk |
	spec := self compile: 'foo [^42] value. ^0'.
	blk := self blockSpecIn: spec.
	self assert: blk hasNlr.
	self assert: blk mnemonics = (self lines: #('LOADINT 1, 42' 'NLR 1'))!

testIvarInBlockGoesThroughInstVarAt
	| scope spec blk |
	scope := OrderedCollection new.
	scope add: 'x'.
	spec := self compile: 'foo ^[x]' ivars: scope.
	blk := self blockSpecIn: spec.
	self assert: blk numCaptured = 1. "self"
	self assert: blk mnemonics = (self lines: #(
		'GETIVAR 5, 4' 'LOADINT 6, 1' 'SEND 1, 5, 0' 'RET 1')).
	self assert: (blk sites at: 1) selector = #instVarAt:!

testNestedBlockCaptureThreadsThroughOuter
	| spec outer inner |
	spec := self compile: 'foo: x ^[[x]]'.
	outer := self blockSpecIn: spec.
	self assert: outer numCaptured = 1.
	"outer re-captures x from its own captured slot for the inner block"
	self assert: outer mnemonics = (self lines: #(
		'GETIVAR 2, 4' 'MKCLOSURE 1, 0' 'CAPTURE 0, 2' 'RET 1')).
	inner := self blockSpecIn: outer.
	self assert: inner numCaptured = 1.
	self assert: inner mnemonics = (self lines: #('GETIVAR 1, 4' 'RET 1'))!

testBlockWithOwnTemps
	| spec blk |
	spec := self compile: 'foo ^[:v | | t | t := v + 1. t]'.
	blk := self blockSpecIn: spec.
	self assert: blk argc = 1.
	self assert: blk numCaptured = 0.
	self assert: blk mnemonics = (self lines: #(
		'MOVE 3, 1' 'LOADINT 4, 1' 'ADD 2, 3, 4' 'MOVE 3, 2'
		'MOVE 3, 2' 'RET 3'))! !
