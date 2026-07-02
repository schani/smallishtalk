"Platform facade (SPEC §18): all host divergence lives here. This is the
 GNU Smalltalk implementation; the self-hosted image provides another class
 with the same class-side protocol over the VM's file/stdio primitives."

Object subclass: #Platform
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

!Platform class methodsFor: 'files'!
readFile: aPathString
	"Answer the file's contents as a String (bytes, not decoded)."
	| f contents |
	f := FileStream open: aPathString mode: FileStream read.
	contents := f contents.
	f close.
	^contents!

writeBytes: aByteCollection toFile: aPathString
	"Write integers 0..255 to the file, replacing it."
	| f |
	f := FileStream open: aPathString mode: FileStream write.
	aByteCollection do: [:b | f nextPutByte: b].
	f close!

log: aString
	Transcript showCr: aString!
bytesOf: aString
	"a collection whose at:/do: yield byte VALUES (self-hosted Strings
	 already do; GST needs the ByteArray view)"
	^aString asByteArray! !
