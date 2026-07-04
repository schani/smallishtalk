"Lexer (SPEC §18): classic Smalltalk-80 tokens, scanned over integer BYTES.

 The scanner never represents source text as host Character objects:
 `Platform bytesOf:` answers integer-indexable bytes on both hosts, all
 structural tests are ASCII byte compares, and — since UTF-8 continuation
 bytes (>= 128) can never collide with ASCII — bytes >= 128 pass through
 opaquely inside string literals, comments and quoted symbols. The one
 place UTF-8 is *decoded* is a `$` character literal, whose token value is
 an StCharCode carrying the code point as a SmallInteger (full scalar
 validation: overlongs, surrogates, > U+10FFFF, truncation all rejected).
 Identifiers remain ASCII by language policy.

 Token types: #identifier #keyword (foo:), #binary, #integer, #string,
 #symbol, #character, #assign (:=), #caret, #period, #semicolon, #lparen,
 #rparen, #lbracket, #rbracket, #blockArg (:x), #hashParen (#( ),
 #hashBracket (#[ ), #verticalBar (|), #lt (< pragma open handled by
 parser via binary '<'), #eof.

 The | character is lexed as #verticalBar; the parser treats it as a
 binary selector where an operator is expected. Likewise unary minus:
 '-' immediately followed by a digit produces a negative number token
 only when the previous token cannot end an operand."

Object subclass: #StToken
	instanceVariableNames: 'type value position'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Compiler'!

!StToken class methodsFor: 'instance creation'!
type: aSymbol value: aValue position: anInteger
	| t |
	t := self new.
	t setType: aSymbol value: aValue position: anInteger.
	^t! !

!StToken methodsFor: 'accessing'!
setType: aSymbol value: aValue position: anInteger
	type := aSymbol. value := aValue. position := anInteger!
type ^type!
value ^value!
position ^position!
printOn: aStream
	aStream nextPutAll: type; nextPut: $:; print: value! !

"A growable byte string builder over the portable byteAt:put: accessor —
 the byte-level replacement for WriteStream-of-Characters token assembly."
Object subclass: #StByteBuf
	instanceVariableNames: 'str n'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Compiler'!

!StByteBuf class methodsFor: 'instance creation'!
new: capacity
	| b |
	b := self new.
	b setCapacity: capacity.
	^b! !

!StByteBuf methodsFor: 'building'!
setCapacity: c
	str := String new: (c max: 8).
	n := 0!
addByte: b
	n = str size ifTrue: [self grow].
	n := n + 1.
	str byteAt: n put: b!
addAll: aString
	1 to: aString size do: [:k | self addByte: (aString byteAt: k)]!
grow
	| s |
	s := String new: str size * 2.
	1 to: n do: [:k | s byteAt: k put: (str byteAt: k)].
	str := s!
size
	^n!
contents
	^str copyFrom: 1 to: n! !

Object subclass: #StLexer
	instanceVariableNames: 'source bytes binBytes pos lastType'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Compiler'!

!StLexer class methodsFor: 'instance creation'!
on: aString
	| l |
	l := self new.
	l setSource: aString.
	^l!

tokenize: aString
	"All tokens including the trailing #eof — the tests' entry point."
	| lexer tokens tok |
	lexer := self on: aString.
	tokens := OrderedCollection new.
	[tok := lexer next. tokens add: tok. tok type = #eof] whileFalse.
	^tokens! !

!StLexer methodsFor: 'private'!
setSource: aString
	source := aString.
	bytes := Platform bytesOf: aString.
	binBytes := Platform bytesOf: '+-*/~<>=&|@%,\'.
	pos := 1.
	lastType := nil!

atEnd
	^pos > bytes size!

peekByte
	^self atEnd ifTrue: [nil] ifFalse: [bytes at: pos]!

peekByte2
	^pos + 1 > bytes size ifTrue: [nil] ifFalse: [bytes at: pos + 1]!

nextByte
	| b |
	b := bytes at: pos.
	pos := pos + 1.
	^b!

sliceFrom: start
	"the source text from byte offset `start` up to (not including) pos —
	 a host String slice, byte-identical on both hosts"
	^source copyFrom: start to: pos - 1!

isDigitByte: b
	b isNil ifTrue: [^false].
	^b >= 48 and: [b <= 57]!

isLetterByte: b
	b isNil ifTrue: [^false].
	(b >= 65 and: [b <= 90]) ifTrue: [^true].
	(b >= 97 and: [b <= 122]) ifTrue: [^true].
	^b = 95!

isIdentByte: b
	^(self isLetterByte: b) or: [self isDigitByte: b]!

isSpaceByte: b
	b isNil ifTrue: [^false].
	^b = 32 or: [b = 9 or: [b = 10 or: [b = 13 or: [b = 12]]]]!

isBinaryByte: b
	| k |
	b isNil ifTrue: [^false].
	k := 1.
	[k <= binBytes size] whileTrue: [
		(binBytes at: k) = b ifTrue: [^true].
		k := k + 1].
	^false!

isUppercaseDigit: b
	b isNil ifTrue: [^false].
	^b >= 65 and: [b <= 90]!

skipSeparators
	[self atEnd] whileFalse: [
		(self isSpaceByte: self peekByte)
			ifTrue: [pos := pos + 1]
			ifFalse: [
				self peekByte = 34 "double quote"
					ifTrue: [self skipComment]
					ifFalse: [^self]]]!

skipComment
	pos := pos + 1.
	[self atEnd] whileFalse: [
		self nextByte = 34 ifTrue: [
			(self peekByte = 34)
				ifTrue: [pos := pos + 1]
				ifFalse: [^self]]].
	self error: 'unterminated comment'!

lastTokenEndsOperand
	"After these, a '-' is the binary minus, not a negative literal."
	^#(#identifier #integer #string #symbol #character #rparen #rbracket) includes: lastType! !

!StLexer methodsFor: 'lexing'!
next
	| t |
	t := self scan.
	lastType := t type.
	^t!

scan
	| start b |
	self skipSeparators.
	start := pos.
	self atEnd ifTrue: [^StToken type: #eof value: nil position: start].
	b := self peekByte.
	(self isLetterByte: b) ifTrue: [^self scanIdentifier: start].
	(self isDigitByte: b) ifTrue: [^self scanNumber: start negative: false].
	b = 39 "'" ifTrue: [^self scanString: start].
	b = 36 "$" ifTrue: [^self scanCharacter: start].
	b = 35 "#" ifTrue: [^self scanHash: start].
	b = 58 ":" ifTrue: [
		self peekByte2 = 61 "=" ifTrue: [
			pos := pos + 2.
			^StToken type: #assign value: ':=' position: start].
		"block argument :name"
		pos := pos + 1.
		(self isLetterByte: self peekByte) ifFalse: [self error: 'bad colon'].
		^StToken type: #blockArg value: (self scanIdentName) position: start].
	b = 94 "^" ifTrue: [pos := pos + 1. ^StToken type: #caret value: '^' position: start].
	b = 46 "." ifTrue: [pos := pos + 1. ^StToken type: #period value: '.' position: start].
	b = 59 ";" ifTrue: [pos := pos + 1. ^StToken type: #semicolon value: ';' position: start].
	b = 40 "(" ifTrue: [pos := pos + 1. ^StToken type: #lparen value: '(' position: start].
	b = 41 ")" ifTrue: [pos := pos + 1. ^StToken type: #rparen value: ')' position: start].
	b = 91 "[" ifTrue: [pos := pos + 1. ^StToken type: #lbracket value: '[' position: start].
	b = 93 "]" ifTrue: [pos := pos + 1. ^StToken type: #rbracket value: ']' position: start].
	b = 124 "|" ifTrue: [pos := pos + 1. ^StToken type: #verticalBar value: '|' position: start].
	"negative number literal: '-' directly before a digit, where the
	 previous token cannot end an operand"
	(b = 45 and: [(self isDigitByte: self peekByte2) and: [self lastTokenEndsOperand not]])
		ifTrue: [pos := pos + 1. ^self scanNumber: start negative: true].
	(self isBinaryByte: b) ifTrue: [^self scanBinary: start].
	self error: 'unexpected character (byte ' , b printString , ')'!

scanIdentName
	| start |
	start := pos.
	[self isIdentByte: self peekByte] whileTrue: [pos := pos + 1].
	^self sliceFrom: start!

scanIdentifier: start
	| name |
	name := self scanIdentName.
	self peekByte = 58 ":" ifTrue: [
		"keyword — but not ':=' (assignment)"
		self peekByte2 = 61 "=" ifFalse: [
			pos := pos + 1.
			^StToken type: #keyword value: name , ':' position: start]].
	^StToken type: #identifier value: name position: start!

scanNumber: start negative: neg
	| n radix |
	n := self scanDigits: 10.
	(self peekByte = 114 "r" and: [(self digitValue: self peekByte2) notNil])
		ifTrue: [
			radix := n.
			pos := pos + 1.
			n := self scanDigits: radix].
	neg ifTrue: [n := 0 - n].
	^StToken type: #integer value: n position: start!

scanDigits: radix
	| n b v any |
	n := 0. any := false.
	[b := self peekByte.
	 v := self digitValue: b.
	 v notNil and: [v < radix]] whileTrue: [
		n := n * radix + v.
		any := true.
		pos := pos + 1].
	any ifFalse: [self error: 'malformed number'].
	^n!

digitValue: b
	b isNil ifTrue: [^nil].
	(self isDigitByte: b) ifTrue: [^b - 48].
	(self isUppercaseDigit: b) ifTrue: [^b - 65 + 10].
	^nil!

scanString: start
	| s |
	pos := pos + 1.
	s := StByteBuf new: 16.
	[self atEnd ifTrue: [self error: 'unterminated string'].
	 self peekByte = 39 and: [self peekByte2 ~= 39]] whileFalse: [
		self peekByte = 39
			ifTrue: [s addByte: 39. pos := pos + 2]
			ifFalse: [s addByte: self nextByte]].
	pos := pos + 1.
	^StToken type: #string value: s contents position: start!

scanCharacter: start
	"one ASCII byte, or one full UTF-8 sequence decoded to its scalar.
	 The token value is an StCharCode (SmallInteger code point inside) —
	 never a host Character."
	| b code |
	pos := pos + 1.
	self atEnd ifTrue: [self error: 'missing character after $'].
	b := self nextByte.
	code := b < 128 ifTrue: [b] ifFalse: [self decodeUtf8: b].
	^StToken type: #character value: (StCharCode code: code) position: start!

decodeUtf8: lead
	"decode the remainder of one UTF-8 sequence whose lead byte (>= 128)
	 is already consumed; answer the Unicode scalar. Rejects continuation
	 or overlong leads (0x80-0xC1), > 4-byte leads (0xF5+), truncated
	 sequences, bad continuation bytes, overlong encodings, surrogates,
	 and code points above U+10FFFF."
	| n code |
	lead < 194 ifTrue: [self error: 'malformed UTF-8 after $'].
	lead <= 223
		ifTrue: [n := 1. code := lead - 192]
		ifFalse: [
			lead <= 239
				ifTrue: [n := 2. code := lead - 224]
				ifFalse: [
					lead <= 244
						ifTrue: [n := 3. code := lead - 240]
						ifFalse: [self error: 'malformed UTF-8 after $']]].
	1 to: n do: [:k |
		| c |
		self atEnd ifTrue: [self error: 'truncated UTF-8 after $'].
		c := self nextByte.
		(c >= 128 and: [c <= 191])
			ifFalse: [self error: 'malformed UTF-8 continuation after $'].
		code := code * 64 + (c - 128)].
	(n = 2 and: [code < 2048]) ifTrue: [self error: 'overlong UTF-8 after $'].
	(n = 3 and: [code < 65536]) ifTrue: [self error: 'overlong UTF-8 after $'].
	code > 1114111 ifTrue: [self error: 'code point out of range after $'].
	(code >= 55296 and: [code <= 57343])
		ifTrue: [self error: 'UTF-8 surrogate after $'].
	^code!

scanBinary: start
	| s |
	s := pos.
	[self isBinaryByte: self peekByte] whileTrue: [pos := pos + 1].
	^StToken type: #binary value: (self sliceFrom: s) position: start!

scanHash: start
	pos := pos + 1.
	self peekByte = 40 "(" ifTrue: [
		pos := pos + 1.
		^StToken type: #hashParen value: '#(' position: start].
	self peekByte = 91 "[" ifTrue: [
		pos := pos + 1.
		^StToken type: #hashBracket value: '#[' position: start].
	self peekByte = 39 "'" ifTrue: [
		| str |
		str := self scanString: start.
		^StToken type: #symbol value: str value asSymbol position: start].
	(self isLetterByte: self peekByte) ifTrue: [
		"identifier or keyword sequence: #foo, #foo:bar:"
		| s |
		s := StByteBuf new: 16.
		[self isLetterByte: self peekByte] whileTrue: [
			s addAll: self scanIdentName.
			self peekByte = 58 ":"
				ifTrue: [s addByte: 58. pos := pos + 1]
				ifFalse: [^StToken type: #symbol value: s contents asSymbol position: start]].
		^StToken type: #symbol value: s contents asSymbol position: start].
	(self isBinaryByte: self peekByte) ifTrue: [
		| b |
		b := self scanBinary: start.
		^StToken type: #symbol value: b value asSymbol position: start].
	self peekByte = 124 "|" ifTrue: [
		pos := pos + 1.
		^StToken type: #symbol value: #| position: start].
	self error: 'malformed literal after #'! !
