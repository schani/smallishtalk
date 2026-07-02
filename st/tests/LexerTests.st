"Phase 2 lexer tests (SPEC §20)."

TestCase subclass: #StLexerTests
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Tests'!

!StLexerTests methodsFor: 'helpers'!
typesOf: source
	| ws |
	ws := WriteStream on: String new.
	(StLexer tokenize: source) do: [:t | ws nextPutAll: t type]
		separatedBy: [ws nextPut: $ ].
	^ws contents!

! !

!StLexerTests methodsFor: 'tests'!
testIdentifiersAndKeywords
	self assert: (self typesOf: 'foo foo: fooBar2')
		= 'identifier keyword identifier eof'!

testAssignVersusKeyword
	self assert: (self typesOf: 'a := b at: 1')
		= 'identifier assign identifier keyword integer eof'!

testNumbers
	| toks |
	toks := StLexer tokenize: '42 16rFF 2r1011 0'.
	self assert: (toks at: 1) value = 42.
	self assert: (toks at: 2) value = 255.
	self assert: (toks at: 3) value = 11.
	self assert: (toks at: 4) value = 0!

testNegativeNumbers
	| toks |
	"after an operand a '-' is binary; before a bare digit it's a sign"
	toks := StLexer tokenize: 'x - 3'.
	self assert: (toks at: 2) type = #binary.
	toks := StLexer tokenize: 'x := -3'.
	self assert: (toks at: 3) type = #integer.
	self assert: (toks at: 3) value = -3.
	toks := StLexer tokenize: '3 -4'.
	"3 - 4: after an integer the '-' stays binary"
	self assert: (toks at: 2) type = #binary!

testStrings
	| toks |
	toks := StLexer tokenize: '''hello'' ''it''''s'''.
	self assert: (toks at: 1) value = 'hello'.
	self assert: (toks at: 2) value = 'it''s'!

testCharacters
	"character tokens carry StCharCode (integer code points), never host
	 Character objects"
	| toks |
	toks := StLexer tokenize: '$a $  $$'.
	self assert: (toks at: 1) value code = 97.
	self assert: (toks at: 2) value code = 32.
	self assert: (toks at: 3) value code = 36!

testSymbols
	| toks |
	toks := StLexer tokenize: '#foo #at:put: #+ #''with space'''.
	self assert: (toks at: 1) value = #foo.
	self assert: (toks at: 2) value = #at:put:.
	self assert: (toks at: 3) value = #+.
	self assert: (toks at: 4) value = ('with space' asSymbol)!

testBinarySelectors
	self assert: (self typesOf: 'a + b // c ~= d')
		= 'identifier binary identifier binary identifier binary identifier eof'.
	self assert: ((StLexer tokenize: 'a ~= b') at: 2) value = '~='!

testComments
	self assert: (self typesOf: 'a "comment ""nested quotes"" more" b')
		= 'identifier identifier eof'!

testPunctuation
	self assert: (self typesOf: '^ x . ; ( ) [ ] | :y')
		= 'caret identifier period semicolon lparen rparen lbracket rbracket verticalBar blockArg eof'!

testHashForms
	self assert: (self typesOf: '#( #[')
		= 'hashParen hashBracket eof'!

testUnicodeCharacterLiterals
	"2-, 3- and 4-byte UTF-8 sequences decode to their scalars"
	| toks |
	toks := StLexer tokenize: '$é $✓ $😀'.
	self assert: (toks at: 1) value code = 233.
	self assert: (toks at: 2) value code = 10003.
	self assert: (toks at: 3) value code = 128512!

testUnicodeStringPassthrough
	"string literals preserve source UTF-8 bytes verbatim (byte count!)"
	| toks v |
	toks := StLexer tokenize: '''héllo'''.
	v := (toks at: 1) value.
	self assert: v size = 6.
	self assert: (v byteAt: 2) = 195.
	self assert: (v byteAt: 3) = 169!

testUnicodeSymbolAndComment
	"UTF-8 passes through quoted symbols and comments untouched"
	| toks |
	toks := StLexer tokenize: '"café comment" #''s✓'''.
	self assert: (toks at: 1) type = #symbol.
	self assert: (toks at: 1) value size = 4!

malformedChar: byteList
	"build '$' followed by the given raw bytes and tokenize it"
	| s |
	s := String new: byteList size + 1.
	s byteAt: 1 put: 36.
	1 to: byteList size do: [:k | s byteAt: k + 1 put: (byteList at: k)].
	^StLexer tokenize: s!

testMalformedUtf8CharacterLiterals
	"full scalar validation: continuation lead, overlongs, surrogates,
	 out of range, truncation, bad continuation byte"
	self should: [self malformedChar: #(128)] raise: Error.
	self should: [self malformedChar: #(192 128)] raise: Error.
	self should: [self malformedChar: #(224 128 128)] raise: Error.
	self should: [self malformedChar: #(237 160 128)] raise: Error.
	self should: [self malformedChar: #(244 144 128 128)] raise: Error.
	self should: [self malformedChar: #(195)] raise: Error.
	self should: [self malformedChar: #(195 65)] raise: Error.
	self should: [self malformedChar: #(245 128 128 128)] raise: Error! !
