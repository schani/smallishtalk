"Lexer (SPEC §18): classic Smalltalk-80 tokens. Character classification is
 done with explicit code-point checks (portable-subset discipline: no
 reliance on host Character protocol beyond #value and #value:).

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
	category: 'Smallishtalk-Compiler'!

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

Object subclass: #StLexer
	instanceVariableNames: 'source pos lastType'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

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
	pos := 1.
	lastType := nil!

atEnd
	^pos > source size!

peekChar
	^self atEnd ifTrue: [nil] ifFalse: [source charAt: pos]!

peekChar2
	^pos + 1 > source size ifTrue: [nil] ifFalse: [source charAt: pos + 1]!

nextChar
	| c |
	c := source charAt: pos.
	pos := pos + 1.
	^c!

isDigitChar: c
	c isNil ifTrue: [^false].
	^c value >= 48 and: [c value <= 57]!

isLetterChar: c
	c isNil ifTrue: [^false].
	(c value >= 65 and: [c value <= 90]) ifTrue: [^true].
	(c value >= 97 and: [c value <= 122]) ifTrue: [^true].
	^c value = 95!

isIdentChar: c
	^(self isLetterChar: c) or: [self isDigitChar: c]!

isSpaceChar: c
	c isNil ifTrue: [^false].
	^c value = 32 or: [c value = 9 or: [c value = 10 or: [c value = 13 or: [c value = 12]]]]!

isBinaryChar: c
	| chars k |
	c isNil ifTrue: [^false].
	chars := '+-*/~<>=&|@%,\'.
	k := 1.
	[k <= chars size] whileTrue: [
		(chars charAt: k) = c ifTrue: [^true].
		k := k + 1].
	^false!

isUppercaseDigit: c
	c isNil ifTrue: [^false].
	^c value >= 65 and: [c value <= 90]!

skipSeparators
	[self atEnd] whileFalse: [
		(self isSpaceChar: self peekChar)
			ifTrue: [pos := pos + 1]
			ifFalse: [
				self peekChar = $"
					ifTrue: [self skipComment]
					ifFalse: [^self]]]!

skipComment
	pos := pos + 1.
	[self atEnd] whileFalse: [
		self nextChar = $" ifTrue: [
			(self peekChar = $")
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
	| start c |
	self skipSeparators.
	start := pos.
	self atEnd ifTrue: [^StToken type: #eof value: nil position: start].
	c := self peekChar.
	(self isLetterChar: c) ifTrue: [^self scanIdentifier: start].
	(self isDigitChar: c) ifTrue: [^self scanNumber: start negative: false].
	c = $' ifTrue: [^self scanString: start].
	c = $$ ifTrue: [^self scanCharacter: start].
	c = $# ifTrue: [^self scanHash: start].
	c = $: ifTrue: [
		self peekChar2 = $= ifTrue: [
			pos := pos + 2.
			^StToken type: #assign value: ':=' position: start].
		"block argument :name"
		pos := pos + 1.
		(self isLetterChar: self peekChar) ifFalse: [self error: 'bad colon'].
		^StToken type: #blockArg value: (self scanIdentName) position: start].
	c = $^ ifTrue: [pos := pos + 1. ^StToken type: #caret value: '^' position: start].
	c = $. ifTrue: [pos := pos + 1. ^StToken type: #period value: '.' position: start].
	c = $; ifTrue: [pos := pos + 1. ^StToken type: #semicolon value: ';' position: start].
	c = $( ifTrue: [pos := pos + 1. ^StToken type: #lparen value: '(' position: start].
	c = $) ifTrue: [pos := pos + 1. ^StToken type: #rparen value: ')' position: start].
	c = $[ ifTrue: [pos := pos + 1. ^StToken type: #lbracket value: '[' position: start].
	c = $] ifTrue: [pos := pos + 1. ^StToken type: #rbracket value: ']' position: start].
	c = $| ifTrue: [pos := pos + 1. ^StToken type: #verticalBar value: '|' position: start].
	"negative number literal: '-' directly before a digit, where the
	 previous token cannot end an operand"
	(c = $- and: [(self isDigitChar: self peekChar2) and: [self lastTokenEndsOperand not]])
		ifTrue: [pos := pos + 1. ^self scanNumber: start negative: true].
	(self isBinaryChar: c) ifTrue: [^self scanBinary: start].
	self error: 'unexpected character: ' , (String with: c)!

scanIdentName
	| s |
	s := WriteStream on: String new.
	[self isIdentChar: self peekChar] whileTrue: [s nextPut: self nextChar].
	^s contents!

scanIdentifier: start
	| name |
	name := self scanIdentName.
	self peekChar = $: ifTrue: [
		"keyword — but not ':=' (assignment)"
		self peekChar2 = $= ifFalse: [
			pos := pos + 1.
			^StToken type: #keyword value: name , ':' position: start]].
	^StToken type: #identifier value: name position: start!

scanNumber: start negative: neg
	| n radix |
	n := self scanDigits: 10.
	(self peekChar = $r and: [(self digitValue: self peekChar2) notNil])
		ifTrue: [
			radix := n.
			pos := pos + 1.
			n := self scanDigits: radix].
	neg ifTrue: [n := 0 - n].
	^StToken type: #integer value: n position: start!

scanDigits: radix
	| n c v any |
	n := 0. any := false.
	[c := self peekChar.
	 v := self digitValue: c.
	 v notNil and: [v < radix]] whileTrue: [
		n := n * radix + v.
		any := true.
		pos := pos + 1].
	any ifFalse: [self error: 'malformed number'].
	^n!

digitValue: c
	c isNil ifTrue: [^nil].
	(self isDigitChar: c) ifTrue: [^c value - 48].
	(self isUppercaseDigit: c) ifTrue: [^c value - 65 + 10].
	^nil!

scanString: start
	| s |
	pos := pos + 1.
	s := WriteStream on: String new.
	[self atEnd ifTrue: [self error: 'unterminated string'].
	 self peekChar = $' and: [self peekChar2 ~= $']] whileFalse: [
		self peekChar = $'
			ifTrue: [s nextPut: $'. pos := pos + 2]
			ifFalse: [s nextPut: self nextChar]].
	pos := pos + 1.
	^StToken type: #string value: s contents position: start!

scanCharacter: start
	pos := pos + 1.
	self atEnd ifTrue: [self error: 'missing character after $'].
	^StToken type: #character value: self nextChar position: start!

scanBinary: start
	| s |
	s := WriteStream on: String new.
	[self isBinaryChar: self peekChar] whileTrue: [s nextPut: self nextChar].
	^StToken type: #binary value: s contents position: start!

scanHash: start
	pos := pos + 1.
	self peekChar = $( ifTrue: [
		pos := pos + 1.
		^StToken type: #hashParen value: '#(' position: start].
	self peekChar = $[ ifTrue: [
		pos := pos + 1.
		^StToken type: #hashBracket value: '#[' position: start].
	self peekChar = $' ifTrue: [
		| str |
		str := self scanString: start.
		^StToken type: #symbol value: str value asSymbol position: start].
	(self isLetterChar: self peekChar) ifTrue: [
		"identifier or keyword sequence: #foo, #foo:bar:"
		| s |
		s := WriteStream on: String new.
		[self isLetterChar: self peekChar] whileTrue: [
			s nextPutAll: self scanIdentName.
			self peekChar = $:
				ifTrue: [s nextPut: $:. pos := pos + 1]
				ifFalse: [^StToken type: #symbol value: s contents asSymbol position: start]].
		^StToken type: #symbol value: s contents asSymbol position: start].
	(self isBinaryChar: self peekChar) ifTrue: [
		| b |
		b := self scanBinary: start.
		^StToken type: #symbol value: b value asSymbol position: start].
	self peekChar = $| ifTrue: [
		pos := pos + 1.
		^StToken type: #symbol value: #| position: start].
	self error: 'malformed literal after #'! !
