"The kernel (SPEC §20, phases 3-4): grown corpus-first. Written in the
 portable dialect: chunk format, literal-block control macros, no class
 variables, insertion-ordered collections only.

 Conventions: Transcript is the TranscriptStream CLASS (class-side
 protocol, no runtime setup needed). String at: answers BYTES at the VM
 level (§15); the Character layer is charAt:. The exception system (§11)
 is image code over the VM primitives 220-224."

nil subclass: #Object
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #Behavior
	instanceVariableNames: 'superclass methodDictionary formatAndSlots classIndex name instVarNames'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Behavior subclass: #Class
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Behavior subclass: #Metaclass
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #UndefinedObject
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #Boolean
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Boolean subclass: #True
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Boolean subclass: #False
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #Magnitude
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Magnitude subclass: #Number
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Number subclass: #Integer
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Integer subclass: #SmallInteger
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #Collection
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Collection subclass: #SequenceableCollection
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

SequenceableCollection subclass: #String
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

String subclass: #ByteString
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

ByteString subclass: #Symbol
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

SequenceableCollection subclass: #Array
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

SequenceableCollection subclass: #ByteArray
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

SequenceableCollection subclass: #OrderedCollection
	instanceVariableNames: 'store count'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Collection subclass: #Dictionary
	instanceVariableNames: 'dKeys dVals'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #WriteStream
	instanceVariableNames: 'target pos'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #Character
	instanceVariableNames: 'codePoint'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #Association
	instanceVariableNames: 'key value'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #Box
	instanceVariableNames: 'boxValue'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #BlockClosure
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #CompiledMethod
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #CompiledBlock
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #Message
	instanceVariableNames: 'selector arguments'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #MethodDictionary
	instanceVariableNames: 'keys values'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #Process
	instanceVariableNames: 'stack frameOffset pc priority nextLink myList serialCounter'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #Semaphore
	instanceVariableNames: 'excessSignals queueHead queueTail'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #LinkedList
	instanceVariableNames: 'head tail'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #ProcessorScheduler
	instanceVariableNames: 'queues activeProcess'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #Stack
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #SystemDictionary
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #TranscriptStream
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #Delay
	instanceVariableNames: 'ms'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #Profiler
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #ExRetryToken
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Object subclass: #Exception
	instanceVariableNames: 'messageText signalOff signalSerial handlerOff handlerSerial'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Exception subclass: #Error
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Error subclass: #ZeroDivide
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Error subclass: #ArithmeticOverflow
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Error subclass: #MessageNotUnderstood
	instanceVariableNames: 'message receiver'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Error subclass: #BlockCannotReturn
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

Error subclass: #ModificationForbidden
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Kernel'!

!Object methodsFor: 'comparing'!
== other
	<primitive: 3>
	^self primitiveFailed: #==!
= other
	^self == other!
~= other
	^(self = other) not!
~~ other
	^(self == other) not!
isNil
	^self == nil!
notNil
	^(self == nil) not!
-> aValue
	| a |
	a := Association new.
	a key: self.
	a value: aValue.
	^a! !

!Object methodsFor: 'accessing'!
class
	<primitive: 1>
	^self primitiveFailed: #class!
identityHash
	<primitive: 2>
	^self primitiveFailed: #identityHash!
hash
	^self identityHash!
instVarAt: i
	<primitive: 9>
	^self primitiveFailed: #instVarAt:!
instVarAt: i put: v
	<primitive: 10>
	^self primitiveFailed: #instVarAt:put:!
perform: aSelector withArguments: anArray
	<primitive: 11>
	^self primitiveFailed: #perform:withArguments:!
yourself
	^self!
isKindOf: aClass
	| c |
	c := self class.
	[c isNil] whileFalse: [
		c == aClass ifTrue: [^true].
		c := c superclass].
	^false! !

!Object methodsFor: 'printing'!
printString
	^'a ' , self class name!
printNl
	Transcript show: self printString.
	Transcript cr.
	^self! !

!Object methodsFor: 'error handling'!
halt: aString
	"last resort: print and stop this process, bypassing exceptions"
	Transcript show: 'FATAL: '.
	Transcript show: aString.
	Transcript cr.
	Processor activeProcess terminate!
error: aString
	^Error new signal: aString!
primitiveFailed: aSelector
	^Error new signal: 'primitive failed: ' , aSelector!
doesNotUnderstand: aMessage
	| e |
	e := MessageNotUnderstood new.
	e setMessage: aMessage receiver: self.
	^e signal!
mustBeBoolean
	^Error new signal: 'mustBeBoolean'!
cannotReturn: value
	^BlockCannotReturn new signal: 'block cannot return'!
subclassResponsibility
	^Error new signal: 'subclass responsibility'! !

!Behavior methodsFor: 'creation'!
new
	<primitive: 4>
	"indexable classes: an empty instance"
	^self new: 0!
new: size
	<primitive: 5>
	^self primitiveFailed: #new:!
basicNew
	<primitive: 4>
	^self primitiveFailed: #basicNew!
name
	^name!
superclass
	^superclass!
instVarNames
	^instVarNames! !

!UndefinedObject methodsFor: 'printing'!
printString
	^'nil'!
isNil
	^true!
notNil
	^false! !

!Boolean methodsFor: 'controlling'!
ifTrue: trueBlock ifFalse: falseBlock
	^self ifTrue: [trueBlock value] ifFalse: [falseBlock value]!
ifFalse: falseBlock ifTrue: trueBlock
	^self ifTrue: [trueBlock value] ifFalse: [falseBlock value]!
ifTrue: trueBlock
	^self ifTrue: [trueBlock value] ifFalse: [nil]!
ifFalse: falseBlock
	^self ifTrue: [nil] ifFalse: [falseBlock value]!
and: aBlock
	^self and: [aBlock value]!
or: aBlock
	^self or: [aBlock value]! !

!True methodsFor: 'logic'!
not
	^false!
& other
	^other!
| other
	^true!
printString
	^'true'! !

!False methodsFor: 'logic'!
not
	^true!
& other
	^false!
| other
	^other!
printString
	^'false'! !

!SmallInteger methodsFor: 'arithmetic'!
+ other
	<primitive: 100>
	(other isKindOf: SmallInteger)
		ifTrue: [^ArithmeticOverflow new signal: 'integer overflow in +'].
	^self primitiveFailed: #+!
- other
	<primitive: 101>
	(other isKindOf: SmallInteger)
		ifTrue: [^ArithmeticOverflow new signal: 'integer overflow in -'].
	^self primitiveFailed: #-!
* other
	<primitive: 102>
	(other isKindOf: SmallInteger)
		ifTrue: [^ArithmeticOverflow new signal: 'integer overflow in *'].
	^self primitiveFailed: #*!
// other
	<primitive: 103>
	other = 0 ifTrue: [^ZeroDivide new signal: 'division by zero'].
	^self primitiveFailed: #//!
\\ other
	<primitive: 104>
	other = 0 ifTrue: [^ZeroDivide new signal: 'division by zero'].
	^self primitiveFailed: #\\!
quo: other
	<primitive: 105>
	other = 0 ifTrue: [^ZeroDivide new signal: 'division by zero'].
	^self primitiveFailed: #quo:!
negated
	^0 - self!
abs
	^self < 0 ifTrue: [self negated] ifFalse: [self]!
min: other
	^self < other ifTrue: [self] ifFalse: [other]!
max: other
	^self < other ifTrue: [other] ifFalse: [self]!
even
	^(self \\ 2) = 0!
odd
	^(self \\ 2) = 1!
bitAnd: other
	<primitive: 111>
	^self primitiveFailed: #bitAnd:!
bitOr: other
	<primitive: 112>
	^self primitiveFailed: #bitOr:!
bitXor: other
	<primitive: 113>
	^self primitiveFailed: #bitXor:!
bitShift: other
	<primitive: 114>
	^self primitiveFailed: #bitShift:! !

!SmallInteger methodsFor: 'comparing'!
< other
	<primitive: 106>
	^self primitiveFailed: #<!
> other
	<primitive: 107>
	^self primitiveFailed: #>!
<= other
	<primitive: 108>
	^self primitiveFailed: #<=!
>= other
	<primitive: 109>
	^self primitiveFailed: #>=!
= other
	<primitive: 110>
	^false!
hash
	^self! !

!SmallInteger methodsFor: 'iterating'!
timesRepeat: aBlock
	| i |
	i := 1.
	[i <= self] whileTrue: [
		aBlock value.
		i := i + 1]!
to: stop do: aBlock
	| i |
	i := self.
	[i <= stop] whileTrue: [
		aBlock value: i.
		i := i + 1]!
to: stop by: step do: aBlock
	| i |
	i := self.
	step > 0
		ifTrue: [
			[i <= stop] whileTrue: [
				aBlock value: i.
				i := i + step]]
		ifFalse: [
			[i >= stop] whileTrue: [
				aBlock value: i.
				i := i + step]]! !

!SmallInteger methodsFor: 'printing'!
printString
	| n digits s i |
	self = 0 ifTrue: [^'0'].
	self < 0 ifTrue: [^'-' , self negated printString].
	n := self.
	digits := 0.
	[n > 0] whileTrue: [
		digits := digits + 1.
		n := n // 10].
	s := String new: digits.
	n := self.
	i := digits.
	[n > 0] whileTrue: [
		s at: i put: 48 + (n \\ 10).
		i := i - 1.
		n := n // 10].
	^s! !

!Collection methodsFor: 'testing'!
isEmpty
	^self size = 0!
notEmpty
	^self size > 0!
asOrderedCollection
	| c |
	c := OrderedCollection new.
	self do: [:e | c add: e].
	^c! !

!SequenceableCollection methodsFor: 'comparing'!
= other
	"classic elementwise equality (String overrides with its own)"
	other isNil ifTrue: [^false].
	other class == self class ifFalse: [^false].
	self size = other size ifFalse: [^false].
	1 to: self size do: [:i |
		(self at: i) = (other at: i) ifFalse: [^false]].
	^true!
hash
	| h |
	h := self size.
	self do: [:e | h := (h * 31 + e hash) bitAnd: 16rFFFFFF].
	^h! !

!SequenceableCollection methodsFor: 'iterating'!
do: aBlock
	| i |
	i := 1.
	[i <= self size] whileTrue: [
		aBlock value: (self at: i).
		i := i + 1]!
do: aBlock separatedBy: sepBlock
	| i |
	i := 1.
	[i <= self size] whileTrue: [
		i > 1 ifTrue: [sepBlock value].
		aBlock value: (self at: i).
		i := i + 1]!
reverseDo: aBlock
	| i |
	i := self size.
	[i >= 1] whileTrue: [
		aBlock value: (self at: i).
		i := i - 1]!
inject: start into: aBlock
	| acc |
	acc := start.
	self do: [:each | acc := aBlock value: acc value: each].
	^acc!
includes: anObject
	self do: [:each | each = anObject ifTrue: [^true]].
	^false!
detect: aBlock ifNone: noneBlock
	self do: [:each | (aBlock value: each) ifTrue: [^each]].
	^noneBlock value!
indexOf: anObject
	| i |
	i := 1.
	[i <= self size] whileTrue: [
		(self at: i) = anObject ifTrue: [^i].
		i := i + 1].
	^0!
first
	^self at: 1!
last
	^self at: self size!
copyFrom: start to: stop
	| r i n |
	n := stop - start + 1.
	n < 0 ifTrue: [n := 0].
	r := self species new: n.
	i := start.
	[i <= stop] whileTrue: [
		r at: i - start + 1 put: (self at: i).
		i := i + 1].
	^r!
species
	^self class! !

!String methodsFor: 'accessing'!
asString
	^self!
charAt: i
	"the Character layer over byte-indexed at: (§15)"
	^Character value: (self at: i)!
species
	^String! !

!String methodsFor: 'building'!
, other
	| r i j |
	r := String new: self size + other size.
	i := 1.
	[i <= self size] whileTrue: [
		r at: i put: (self at: i).
		i := i + 1].
	j := 1.
	[j <= other size] whileTrue: [
		r at: i put: (other at: j).
		i := i + 1.
		j := j + 1].
	^r!
= other
	| i |
	other isNil ifTrue: [^false].
	(other isKindOf: String) ifFalse: [^false].
	self size = other size ifFalse: [^false].
	i := 1.
	[i <= self size] whileTrue: [
		(self at: i) = (other at: i) ifFalse: [^false].
		i := i + 1].
	^true!
hash
	| h i |
	h := self size.
	i := 1.
	[i <= self size] whileTrue: [
		h := (h * 31 + (self at: i)) bitAnd: 16rFFFFFF.
		i := i + 1].
	^h!
< other
	"byte comparison = code-point order for valid UTF-8 (§15)"
	| i limit |
	limit := self size min: other size.
	i := 1.
	[i <= limit] whileTrue: [
		(self at: i) < (other at: i) ifTrue: [^true].
		(self at: i) > (other at: i) ifTrue: [^false].
		i := i + 1].
	^self size < other size!
printString
	^'''' , self , ''''!
asSymbol
	^Symbol intern: self! !

!String class methodsFor: 'creation'!
with: aCharacter
	| s |
	s := self new: 1.
	s at: 1 put: aCharacter value.
	^s! !

!Symbol class methodsFor: 'interning'!
intern: aString
	"strong interning over the SymbolTable global (an Array, grown by
	 copying; §15). Symbol identity = pointer identity afterwards."
	| table i cand sym |
	table := SymbolTable.
	i := 1.
	[i <= table size] whileTrue: [
		cand := table at: i.
		(cand notNil and: [self is: cand sameAs: aString])
			ifTrue: [^cand].
		i := i + 1].
	sym := self newFrom: aString.
	SymbolTable := self append: sym to: table.
	^sym!
is: aSymbol sameAs: aString
	aSymbol size = aString size ifFalse: [^false].
	1 to: aSymbol size do: [:k |
		(aSymbol at: k) = (aString at: k) ifFalse: [^false]].
	^true!
newFrom: aString
	| s |
	s := Symbol new: aString size.
	1 to: aString size do: [:k | s at: k put: (aString at: k)].
	^s!
append: anObject to: anArray
	| r |
	r := Array new: anArray size + 1.
	1 to: anArray size do: [:k | r at: k put: (anArray at: k)].
	r at: r size put: anObject.
	^r! !

!Symbol methodsFor: 'printing'!
asString
	"a plain (non-interned, content-equal) String copy — Symbol equality
	 is identity (§15), so mixed comparisons need the String view"
	| r i |
	r := String new: self size.
	i := 1.
	[i <= self size] whileTrue: [
		r at: i put: (self at: i).
		i := i + 1].
	^r!
printString
	^'#' , self!
= other
	^self == other!
hash
	^self identityHash!
asSymbol
	^self! !

!Character class methodsFor: 'creation'!
value: anInteger
	| c |
	c := self new.
	c setCodePoint: anInteger.
	^c! !

!Character methodsFor: 'accessing'!
setCodePoint: anInteger
	codePoint := anInteger!
value
	^codePoint!
= other
	^(other isKindOf: Character) and: [codePoint = other value]!
hash
	^codePoint! !

!Array class methodsFor: 'creation'!
with: a
	| r |
	r := self new: 1.
	r at: 1 put: a.
	^r!
with: a with: b
	| r |
	r := self new: 2.
	r at: 1 put: a.
	r at: 2 put: b.
	^r!
with: a with: b with: c
	| r |
	r := self new: 3.
	r at: 1 put: a.
	r at: 2 put: b.
	r at: 3 put: c.
	^r!
with: a with: b with: c with: d
	| r |
	r := self new: 4.
	r at: 1 put: a.
	r at: 2 put: b.
	r at: 3 put: c.
	r at: 4 put: d.
	^r!
with: a with: b with: c with: d with: e
	| r |
	r := self new: 5.
	r at: 1 put: a.
	r at: 2 put: b.
	r at: 3 put: c.
	r at: 4 put: d.
	r at: 5 put: e.
	^r! !

!Array methodsFor: 'printing'!
printString
	| ws |
	ws := WriteStream on: (String new: 8).
	ws nextPutAll: '#('.
	self do: [:e | ws nextPutAll: e printString] separatedBy: [ws nextPut: 32].
	ws nextPutAll: ')'.
	^ws contents! !

!OrderedCollection class methodsFor: 'creation'!
new
	| c |
	c := super new.
	c initStore.
	^c!
with: a
	| c |
	c := self new.
	c add: a.
	^c! !

!OrderedCollection methodsFor: 'accessing'!
initStore
	store := Array new: 8.
	count := 0!
size
	^count!
at: i
	(i >= 1 and: [i <= count]) ifFalse: [^self error: 'index out of bounds'].
	^store at: i!
at: i put: v
	(i >= 1 and: [i <= count]) ifFalse: [^self error: 'index out of bounds'].
	^store at: i put: v!
add: v
	count = store size ifTrue: [self grow].
	count := count + 1.
	store at: count put: v.
	^v!
addLast: v
	^self add: v!
addAll: aCollection
	aCollection do: [:e | self add: e].
	^aCollection!
removeLast
	| v |
	count = 0 ifTrue: [^self error: 'empty collection'].
	v := store at: count.
	store at: count put: nil.
	count := count - 1.
	^v!
grow
	| n i |
	n := Array new: store size * 2.
	i := 1.
	[i <= count] whileTrue: [
		n at: i put: (store at: i).
		i := i + 1].
	store := n!
asArray
	| r i |
	r := Array new: count.
	i := 1.
	[i <= count] whileTrue: [
		r at: i put: (store at: i).
		i := i + 1].
	^r!
species
	^OrderedCollection!
copyFrom: start to: stop
	| r i |
	r := OrderedCollection new.
	i := start.
	[i <= stop] whileTrue: [
		r add: (self at: i).
		i := i + 1].
	^r! !

!Dictionary class methodsFor: 'creation'!
new
	| d |
	d := super new.
	d initStore.
	^d! !

!Dictionary methodsFor: 'accessing'!
initStore
	dKeys := OrderedCollection new.
	dVals := OrderedCollection new!
size
	^dKeys size!
indexOfKey: k
	| i |
	i := 1.
	[i <= dKeys size] whileTrue: [
		(dKeys at: i) = k ifTrue: [^i].
		i := i + 1].
	^0!
at: k put: v
	| i |
	i := self indexOfKey: k.
	i = 0
		ifTrue: [
			dKeys add: k.
			dVals add: v]
		ifFalse: [dVals at: i put: v].
	^v!
at: k
	^self at: k ifAbsent: [self error: 'key not found']!
at: k ifAbsent: aBlock
	| i |
	i := self indexOfKey: k.
	i = 0 ifTrue: [^aBlock value].
	^dVals at: i!
at: k ifAbsentPut: aBlock
	| i |
	i := self indexOfKey: k.
	i = 0 ifTrue: [^self at: k put: aBlock value].
	^dVals at: i!
includesKey: k
	^(self indexOfKey: k) > 0!
keysDo: aBlock
	dKeys do: aBlock!
valuesDo: aBlock
	dVals do: aBlock!
keysAndValuesDo: aBlock
	| i |
	i := 1.
	[i <= dKeys size] whileTrue: [
		aBlock value: (dKeys at: i) value: (dVals at: i).
		i := i + 1]!
do: aBlock
	dVals do: aBlock! !

!WriteStream class methodsFor: 'creation'!
on: aCollection
	| s |
	s := self new.
	s setTarget: aCollection.
	^s! !

!WriteStream methodsFor: 'writing'!
setTarget: aCollection
	target := aCollection.
	pos := 0!
nextPut: x
	| v |
	pos = target size ifTrue: [self growTarget].
	pos := pos + 1.
	v := x.
	(x isKindOf: Character) ifTrue: [v := x value].
	target at: pos put: v.
	^x!
nextPutAll: aCollection
	| i |
	i := 1.
	[i <= aCollection size] whileTrue: [
		self nextPut: (aCollection at: i).
		i := i + 1].
	^aCollection!
growTarget
	| n i newSize |
	newSize := target size * 2.
	newSize < 8 ifTrue: [newSize := 8].
	n := target species new: newSize.
	i := 1.
	[i <= pos] whileTrue: [
		n at: i put: (target at: i).
		i := i + 1].
	target := n!
contents
	| r i |
	r := target species new: pos.
	i := 1.
	[i <= pos] whileTrue: [
		r at: i put: (target at: i).
		i := i + 1].
	^r! !

!Association methodsFor: 'accessing'!
key
	^key!
key: k
	key := k!
value
	^value!
value: v
	value := v! !

!Message methodsFor: 'accessing'!
selector
	^selector!
arguments
	^arguments! !

!BlockClosure methodsFor: 'evaluating'!
value
	<primitive: 200>
	^self primitiveFailed: #value!
value: a
	<primitive: 201>
	^self primitiveFailed: #value:!
value: a value: b
	<primitive: 202>
	^self primitiveFailed: #value:value:!
value: a value: b value: c
	<primitive: 203>
	^self primitiveFailed: #value:value:value:!
value: a value: b value: c value: d
	<primitive: 204>
	^self primitiveFailed: #value:value:value:value:!
valueWithArguments: anArray
	<primitive: 205>
	^self primitiveFailed: #valueWithArguments:! !

!BlockClosure methodsFor: 'exceptions'!
on: excClass do: handlerBlock
	"§11 — the retry loop; the handler frame itself is protectOn:do:"
	| result |
	[true] whileTrue: [
		result := self protectOn: excClass do: handlerBlock.
		(result == ExRetryToken) ifFalse: [^result]]!
protectOn: excClass do: handlerBlock
	<handlerFrame>
	| hClass hBlock hState |
	hClass := excClass.
	hBlock := handlerBlock.
	hState := 1.
	^self value!
ensure: aBlock
	<ensureFrame>
	| eBlock ePendingTarget ePendingSerial ePendingValue result runNow |
	eBlock := aBlock.
	result := self value.
	"normal completion: consume the slot, then run the block once"
	runNow := eBlock.
	eBlock := nil.
	runNow notNil ifTrue: [runNow value].
	^result!
ifCurtailed: aBlock
	<ensureFrame>
	| eBlock ePendingTarget ePendingSerial ePendingValue result |
	eBlock := aBlock.
	result := self value.
	"normal completion: cancel — the block runs only when unwound through"
	eBlock := nil.
	^result! !

!BlockClosure methodsFor: 'processes'!
fork
	^self forkAt: 4!
forkAt: aPriority
	"§13: process creation is pure image code over the object model"
	| p s |
	p := Process new.
	s := Stack new: 512.
	s at: 1 put: p.
	s at: 2 put: 0.
	s at: 3 put: 0.
	s at: 4 put: (self instVarAt: 1).
	s at: 5 put: (1 bitShift: 32).
	s at: 6 put: self.
	p instVarAt: 1 put: s.
	p instVarAt: 2 put: 1.
	p instVarAt: 3 put: 0.
	p instVarAt: 4 put: aPriority.
	p instVarAt: 7 put: 1.
	p resume.
	^p! !

!Exception methodsFor: 'primitives'!
findHandlerFor: excClass from: anOffset
	<primitive: 220>
	^nil!
unwindTo: off serial: ser return: v
	<primitive: 221>
	^BlockCannotReturn new signal: 'unwind target is dead'!
handlerInfoAt: off
	<primitive: 222>
	^self primitiveFailed: #handlerInfoAt:!
setHandlerState: off to: state
	<primitive: 223>
	^self primitiveFailed: #setHandlerState:to:!
signalContext
	<primitive: 224>
	^self primitiveFailed: #signalContext! !

!Exception methodsFor: 'signaling'!
signal
	| ctx hoff info hBlock result |
	ctx := self signalContext.
	signalOff := ctx at: 1.
	signalSerial := ctx at: 2.
	hoff := self findHandlerFor: self class from: nil.
	hoff isNil ifTrue: [^self noHandler].
	info := self handlerInfoAt: hoff.
	handlerOff := hoff.
	handlerSerial := info at: 3.
	self setHandlerState: hoff to: 2.
	hBlock := info at: 2.
	result := hBlock value: self.
	"falling off the handler = return: its value (§11)"
	^self unwindTo: handlerOff serial: handlerSerial return: result!
signal: aString
	messageText := aString.
	^self signal!
messageText
	^messageText!
description
	^messageText isNil
		ifTrue: [self class name]
		ifFalse: [messageText]!
noHandler
	Transcript show: 'Unhandled exception: ' , self class name.
	messageText notNil ifTrue: [
		Transcript show: ': ' , messageText].
	Transcript cr.
	Processor activeProcess terminate! !

!Exception methodsFor: 'handling'!
return: v
	^self unwindTo: handlerOff serial: handlerSerial return: v!
return
	^self return: nil!
resume: v
	^self unwindTo: signalOff serial: signalSerial return: v!
resume
	^self resume: nil!
retry
	^self unwindTo: handlerOff serial: handlerSerial return: ExRetryToken!
pass
	"re-signal to the next enclosing handler (ours is in-progress and
	 skipped by the walk); if the outer handler resumes, the resumption
	 value becomes our handler's result"
	^self signal! !

!Exception class methodsFor: 'creation'!
signal: aString
	^self new signal: aString!
signal
	^self new signal! !

!MessageNotUnderstood methodsFor: 'accessing'!
setMessage: aMessage receiver: anObject
	message := aMessage.
	receiver := anObject.
	messageText := anObject class name , ' does not understand ' , aMessage selector!
message
	^message!
receiver
	^receiver! !

!Process methodsFor: 'scheduling'!
terminate
	<primitive: 216>
	^self primitiveFailed: #terminate!
resume
	<primitive: 215>
	^self primitiveFailed: #resume!
suspend
	<primitive: 214>
	^self primitiveFailed: #suspend! !

!ProcessorScheduler methodsFor: 'accessing'!
activeProcess
	^activeProcess!
yield
	<primitive: 213>
	^self primitiveFailed: #yield! !

!Semaphore class methodsFor: 'creation'!
new
	| s |
	s := super new.
	s initSignals.
	^s! !

!Semaphore methodsFor: 'signaling'!
initSignals
	excessSignals := 0!
wait
	<primitive: 211>
	^self primitiveFailed: #wait!
signal
	<primitive: 212>
	^self primitiveFailed: #signal! !

!Delay class methodsFor: 'creation'!
forMilliseconds: n
	| d |
	d := self new.
	d setMs: n.
	^d! !

!Delay methodsFor: 'waiting'!
setMs: n
	ms := n!
primSignal: sem atMs: t
	<primitive: 322>
	^self primitiveFailed: #primSignal:atMs:!
primClockMs
	<primitive: 320>
	^self primitiveFailed: #primClockMs!
wait
	| sem |
	sem := Semaphore new.
	self primSignal: sem atMs: self primClockMs + ms.
	sem wait! !

!TranscriptStream class methodsFor: 'writing'!
primWrite: aString to: fd
	<primitive: 310>
	^self halt: 'stdio write failed'!
show: aString
	self primWrite: aString to: 1.
	^self!
cr
	| s |
	s := String new: 1.
	s at: 1 put: 10.
	self primWrite: s to: 1.
	^self!
showCr: aString
	self show: aString.
	self cr.
	^self! !

!Profiler class methodsFor: 'primitives'!
primStart: intervalMs
	<primitive: 420>
	^self primitiveFailed: #primStart:!
primStop
	<primitive: 421>
	^self primitiveFailed: #primStop!
primReport
	<primitive: 422>
	^self primitiveFailed: #primReport!
primCounters
	<primitive: 424>
	^self primitiveFailed: #primCounters!
primCountersReset
	<primitive: 425>
	^self primitiveFailed: #primCountersReset!
primGate: aBoolean
	<primitive: 426>
	^self primitiveFailed: #primGate:!
primClockMs
	<primitive: 320>
	^self primitiveFailed: #primClockMs! !

!Profiler class methodsFor: 'profiling'!
spy: aBlock
	^self spy: aBlock interval: 1!
spy: aBlock interval: ms
	| result |
	self primStart: ms.
	result := aBlock value.
	self primStop.
	self printReport: self primReport.
	^result!
counters
	self primCounters do: [:row |
		Transcript showCr: (row at: 1) , ' = ' , (row at: 2) printString].
	^self!
resetCounters
	self primCountersReset.
	^self!
gate: aBoolean
	self primGate: aBoolean.
	^self!
printReport: report
	| total rows |
	total := report at: 1.
	Transcript showCr: 'Profile: ' , total printString , ' samples at ' , (report at: 2) printString , ' ms'.
	total = 0 ifTrue: [^self].
	Transcript showCr: ' self%  total%  name'.
	rows := report at: 3.
	rows do: [:row |
		Transcript showCr: (self pad: (self percent: (row at: 2) of: total) to: 6)
			, (self pad: (self percent: (row at: 3) of: total) to: 8)
			, '  ' , (row at: 1)].
	^self!
percent: n of: total
	| tenths |
	tenths := n * 1000 // total.
	^(tenths // 10) printString , '.' , (tenths \\ 10) printString , '%'!
pad: aString to: n
	| s |
	s := aString.
	[s size < n] whileTrue: [s := ' ' , s].
	^s! !
