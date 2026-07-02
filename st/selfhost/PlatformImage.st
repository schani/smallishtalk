"The self-hosted Platform (SPEC §18): the same class-side protocol as the
 GST Platform, over the VM's file primitives. Filed into the self-host
 image instead of Platform.st/Compat.st."

Object subclass: #Platform
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Selfhost'!

!Platform class methodsFor: 'primitives'!
primOpen: path mode: m
	<primitive: 300>
	^self error: 'cannot open ' , path!
primClose: fd
	<primitive: 301>
	^self error: 'close failed'!
primRead: fd count: n
	<primitive: 302>
	^self error: 'read failed'!
primWrite: fd bytes: b
	<primitive: 303>
	^self error: 'write failed'!
primSize: fd
	<primitive: 306>
	^self error: 'size failed'! !

!Platform class methodsFor: 'files'!
readFile: aPathString
	| fd size data s i |
	fd := self primOpen: aPathString mode: 0.
	size := self primSize: fd.
	data := self primRead: fd count: size.
	self primClose: fd.
	s := String new: data size.
	i := 1.
	[i <= data size] whileTrue: [
		s at: i put: (data at: i).
		i := i + 1].
	^s!
writeBytes: aByteCollection toFile: aPathString
	| fd s i |
	s := String new: aByteCollection size.
	i := 1.
	aByteCollection do: [:b |
		s at: i put: b.
		i := i + 1].
	fd := self primOpen: aPathString mode: 1.
	self primWrite: fd bytes: s.
	self primClose: fd!
log: aString
	Transcript showCr: aString!
bytesOf: aString
	"self-hosted Strings are byte-indexed already (§15)"
	^aString! !
