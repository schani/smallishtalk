"Phase 2 chunk-reader tests (SPEC §20). The bang characters inside the
 test-source strings are doubled twice over: once for the enclosing GST
 chunk file, once for the string literal — written here via bang-free
 building blocks to keep this file readable."

TestCase subclass: #StChunkReaderTests
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Tests'!

!StChunkReaderTests methodsFor: 'helpers'!
bang
	^String with: (Character value: 33)!

events: aString
	^StChunkReader eventsFrom: aString! !

!StChunkReaderTests methodsFor: 'tests'!
testDoItChunks
	| evs |
	evs := self events: 'Transcript show: 1 printString', self bang, ' 3 + 4', self bang.
	self assert: evs size = 2.
	self assert: (evs at: 1) first = #doIt.
	self assert: ((evs at: 1) at: 2) = 'Transcript show: 1 printString'.
	"leading separators are skipped before a chunk starts"
	self assert: ((evs at: 2) at: 2) = '3 + 4'!

testEscapedBang
	| evs |
	"doubled bang inside a chunk is a literal bang"
	evs := self events: 'x := ', self bang, self bang, 'weird', self bang.
	self assert: evs size = 1.
	self assert: ((evs at: 1) at: 2) = ('x := ', self bang, 'weird')!

testMethodGroup
	| evs src grp |
	src := self bang, 'Point methodsFor: ''accessing''', self bang,
		' x ^x', self bang,
		' y ^y', self bang, ' ', self bang.
	evs := self events: src.
	self assert: evs size = 1.
	grp := evs at: 1.
	self assert: grp first = #methods.
	self assert: (grp at: 2) = 'Point'.
	self assert: (grp at: 3) = false.
	self assert: (grp at: 4) = 'accessing'.
	self assert: (grp at: 5) size = 2.
	self assert: ((grp at: 5) at: 1) = ' x ^x'.
	self assert: ((grp at: 5) at: 2) = ' y ^y'!

testClassSideGroup
	| evs src grp |
	src := self bang, 'Point class methodsFor: ''creation''', self bang,
		' new ^super new init', self bang, ' ', self bang.
	evs := self events: src.
	grp := evs at: 1.
	self assert: (grp at: 2) = 'Point'.
	self assert: (grp at: 3) = true.
	self assert: (grp at: 5) size = 1!

testMixedFile
	| evs src |
	src := 'Object subclass: 1', self bang, ' ',
		self bang, 'Foo methodsFor: ''a''', self bang,
		' bar ^1', self bang, ' ', self bang, '
		Foo new', self bang.
	evs := self events: src.
	self assert: evs size = 3.
	self assert: (evs at: 1) first = #doIt.
	self assert: (evs at: 2) first = #methods.
	self assert: (evs at: 3) first = #doIt!

testTrailingWhitespaceIgnored
	| evs |
	evs := self events: 'x', self bang, '   '.
	self assert: evs size = 1! !

!StChunkReaderTests methodsFor: 'tests'!
testDollarBangCharacterLiteral
	| evs src |
	src := 'x := $', self bang, ' value', self bang.
	evs := self events: src.
	self assert: evs size = 1.
	self assert: ((evs at: 1) at: 2) = ('x := $', self bang, ' value')! !
