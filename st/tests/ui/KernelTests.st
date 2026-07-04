"Kernel runtime behavior tested ON the VM: the primitive-failure fallback
 methods for the inlined at: / at:put: (primitives 6/7). The inline
 primitives handle every proper access, so reaching Smalltalk code means the
 access is improper — and it must surface as a DESCRIPTIVE Error, not as the
 MessageNotUnderstood a missing fallback produced (the option-left Workspace
 crash: storing char 63234 into a byte String)."

TestCase subclass: #KernelTests
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Testing'!

!KernelTests methodsFor: 'helpers'!
errorFrom: aBlock
	"run aBlock, answering the Error it signals (nil if none)."
	aBlock on: Error do: [:e | ^e].
	^nil! !

!KernelTests methodsFor: 'tests'!
testImproperStoreIsADescriptiveErrorNotADnu
	"a byte String cannot hold a code point > 255: the store must raise a
	 real diagnosis, not 'String does not understand at:put:'."
	| e |
	e := self errorFrom: [(String new: 1) at: 1 put: 63234].
	self deny: e isNil.
	self deny: (e isKindOf: MessageNotUnderstood).
	self assert: e messageText
		equals: 'at:put: improper store (value unstorable here, or the receiver is immutable)'!
testAtPutIndexErrorsAreDescriptive
	| e |
	e := self errorFrom: [(Array new: 2) at: 5 put: 1].
	self deny: (e isKindOf: MessageNotUnderstood).
	self assert: e messageText equals: 'at:put: index out of bounds'.
	e := self errorFrom: [(Array new: 2) at: 'x' put: 1].
	self assert: e messageText equals: 'at:put: index must be an Integer'!
testAtIndexErrorsAreDescriptive
	| e |
	e := self errorFrom: [(Array new: 2) at: 5].
	self deny: (e isKindOf: MessageNotUnderstood).
	self assert: e messageText equals: 'at: index out of bounds'.
	e := self errorFrom: [(String new: 1) at: nil].
	self assert: e messageText equals: 'at: index must be an Integer'! !
