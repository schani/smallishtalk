"Phase 2 heap-writer tests (SPEC §20): bytes at offsets per the Treaty,
 and the determinism rule (same input → identical bytes)."

TestCase subclass: #StImageWriterTests
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Tests'!

!StImageWriterTests methodsFor: 'helpers'!
miniBuilder
	| b |
	b := StImageBuilder new.
	b writer defineClass: 'Object' superclass: nil instVarNames: OrderedCollection new.
	b writer defineClass: 'UndefinedObject' superclass: 'Object' instVarNames: OrderedCollection new.
	b writer defineClass: 'True' superclass: 'Object' instVarNames: OrderedCollection new.
	b writer defineClass: 'False' superclass: 'Object' instVarNames: OrderedCollection new.
	b writer defineClass: 'SystemDictionary' superclass: 'Object' instVarNames: OrderedCollection new.
	^b! !

!StImageWriterTests methodsFor: 'tests'!
testHeaderLayout
	| b bytes |
	b := self miniBuilder.
	b programSource: '^1'.
	bytes := b imageBytes.
	"magic 'STIM' at offset 0 (1-based indices here)"
	self assert: (bytes at: 1) = 83.
	self assert: (bytes at: 2) = 84.
	self assert: (bytes at: 3) = 73.
	self assert: (bytes at: 4) = 77.
	"version 1"
	self assert: (bytes at: 5) = 1.
	"savedBaseAddress = 16r10000 little-endian at offset 16"
	self assert: (bytes at: 17) = 0.
	self assert: (bytes at: 18) = 0.
	self assert: (bytes at: 19) = 1.
	"old space begins at offset 64 with nil: header class 5, no slots.
	 5<<42 → high half 5<<10 = 16r1400"
	self assert: (bytes at: 65) = 0.
	self assert: (bytes at: 69) = 0.
	self assert: (bytes at: 70) = 16r14.
	"true (class 6) follows 8 bytes later: high half 6<<10 = 16r1800"
	self assert: (bytes at: 78) = 16r18!

testDeterminism
	"the §18 rule, enforced by construction: two fresh builds of the
	 same input are byte-identical"
	| b1 b2 bytes1 bytes2 |
	b1 := self miniBuilder.
	b1 programSource: '| a | a := 2 + 3. ^a'.
	bytes1 := b1 imageBytes.
	b2 := self miniBuilder.
	b2 programSource: '| a | a := 2 + 3. ^a'.
	bytes2 := b2 imageBytes.
	self assert: bytes1 size = bytes2 size.
	1 to: bytes1 size do: [:i |
		(bytes1 at: i) = (bytes2 at: i)
			ifFalse: [self assert: false]]!

testOldSpaceSizeMatchesLayout
	| b bytes size |
	b := self miniBuilder.
	b programSource: '^1'.
	bytes := b imageBytes.
	"oldSpaceByteSize at offset 24 (LE u64), file = 64 + that size"
	size := (bytes at: 25) + ((bytes at: 26) * 256) + ((bytes at: 27) * 65536)
		+ ((bytes at: 28) * 16777216).
	self assert: bytes size = (64 + size)!

testSymbolsAreInternedOnce
	| w s1 s2 |
	w := StImageWriter new.
	w init.
	s1 := w symbolFor: 'foo'.
	s2 := w symbolFor: 'foo'.
	self assert: s1 == s2.
	self assert: (w symbolFor: 'bar') ~~ s1! !
