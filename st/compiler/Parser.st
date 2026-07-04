"Parser (SPEC §18): classic Smalltalk-80 grammar, recursive descent over
 the token list. Produces AST.st nodes.

 Portable-dialect restrictions (enforced here, documented in SPEC §18's
 discipline): no floats or scaled decimals; each cascade part is a single
 message (unary, binary, or one keyword message); no brace arrays;
 thisContext does not exist."

Object subclass: #StParser
	instanceVariableNames: 'tokens index'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Compiler'!

!StParser class methodsFor: 'parsing'!
parseMethod: aString
	"source of a full method (pattern, pragmas, temps, body) → StMethodNode"
	| p |
	p := self new.
	p setTokens: (StLexer tokenize: aString).
	^p parseMethod!

parseDoIt: aString
	"an expression sequence (chunk / corpus program) → StMethodNode with
	 selector #DoIt, no arguments"
	| p |
	p := self new.
	p setTokens: (StLexer tokenize: aString).
	^p parseDoIt! !

!StParser methodsFor: 'private'!
setTokens: aCollection
	tokens := aCollection asArray.
	index := 1!

current
	^tokens at: index!

currentType
	^self current type!

currentValue
	^self current value!

peekType
	^index + 1 <= tokens size
		ifTrue: [(tokens at: index + 1) type]
		ifFalse: [#eof]!

advance
	| t |
	t := self current.
	index := index + 1.
	^t!

expect: aType
	self currentType = aType
		ifFalse: [self error: 'expected ' , aType , ' but got ' , self current printString].
	^self advance!

isBinaryOperator
	"a binary-selector position accepts #binary and the lone vertical bar"
	^self currentType = #binary or: [self currentType = #verticalBar]! !

!StParser methodsFor: 'parsing'!
parseMethod
	| node |
	node := StMethodNode new.
	node pragmas: OrderedCollection new.
	self parsePatternInto: node.
	self parsePragmasAndTempsInto: node.
	node statements: (self parseStatementsUntil: #eof).
	^node!

parseDoIt
	| node |
	node := StMethodNode new.
	node selector: #DoIt.
	node arguments: OrderedCollection new.
	node pragmas: OrderedCollection new.
	node temporaries: OrderedCollection new.
	self currentType = #verticalBar ifTrue: [
		node temporaries: self parseTempNames].
	node statements: (self parseStatementsUntil: #eof).
	^node!

parsePatternInto: node
	| sel args |
	args := OrderedCollection new.
	self currentType = #identifier ifTrue: [
		sel := self advance value.
		node selector: sel asSymbol.
		node arguments: args.
		^self].
	self isBinaryOperator ifTrue: [
		sel := self advance value.
		args add: (self expect: #identifier) value.
		node selector: sel asSymbol.
		node arguments: args.
		^self].
	self currentType = #keyword ifTrue: [
		| ws |
		ws := WriteStream on: String new.
		[self currentType = #keyword] whileTrue: [
			ws nextPutAll: self advance value.
			args add: (self expect: #identifier) value].
		node selector: ws contents asSymbol.
		node arguments: args.
		^self].
	self error: 'malformed method pattern: ' , self current printString!

parsePragmasAndTempsInto: node
	node temporaries: OrderedCollection new.
	[true] whileTrue: [
		(self currentType = #binary and: [self currentValue = '<'])
			ifTrue: [node pragmas add: self parsePragma]
			ifFalse: [
				self currentType = #verticalBar
					ifTrue: [node temporaries addAll: self parseTempNames]
					ifFalse: [^self]]]!

parsePragma
	| prag sel args ws |
	self advance. "the <"
	prag := StPragmaNode new.
	args := OrderedCollection new.
	self currentType = #identifier
		ifTrue: [
			sel := self advance value asSymbol]
		ifFalse: [
			ws := WriteStream on: String new.
			[self currentType = #keyword] whileTrue: [
				ws nextPutAll: self advance value.
				args add: self parsePragmaLiteral].
			ws contents isEmpty ifTrue: [self error: 'malformed pragma'].
			sel := ws contents asSymbol].
	prag selector: sel.
	prag arguments: args.
	(self currentType = #binary and: [self currentValue = '>'])
		ifFalse: [self error: 'pragma not closed'].
	self advance.
	^prag!

parsePragmaLiteral
	self currentType = #integer ifTrue: [^self advance value].
	self currentType = #string ifTrue: [^self advance value].
	self currentType = #symbol ifTrue: [^self advance value].
	self currentType = #identifier ifTrue: [
		self currentValue = 'true' ifTrue: [self advance. ^true].
		self currentValue = 'false' ifTrue: [self advance. ^false].
		self currentValue = 'nil' ifTrue: [self advance. ^nil]].
	self error: 'bad pragma literal'!

parseTempNames
	| names |
	self expect: #verticalBar.
	names := OrderedCollection new.
	[self currentType = #identifier] whileTrue: [names add: self advance value].
	self expect: #verticalBar.
	^names!

parseStatementsUntil: endType
	| stmts |
	stmts := OrderedCollection new.
	[self currentType = endType] whileFalse: [
		self currentType = #caret
			ifTrue: [
				self advance.
				stmts add: (StReturnNode new expression: self parseExpression; yourself).
				self currentType = #period ifTrue: [self advance].
				self currentType = endType
					ifFalse: [self error: 'statements after return'].
				^stmts]
			ifFalse: [
				stmts add: self parseExpression.
				self currentType = #period
					ifTrue: [self advance]
					ifFalse: [
						self currentType = endType
							ifFalse: [self error: 'expected period or end, got ' , self current printString].
						^stmts]]].
	^stmts!

parseExpression
	(self currentType = #identifier and: [self peekType = #assign]) ifTrue: [
		| name |
		name := self advance value.
		self advance. "':='"
		^StAssignmentNode new variable: name; expression: self parseExpression; yourself].
	^self parseCascade!

parseCascade
	| e cascade first |
	e := self parseKeywordMessage.
	self currentType = #semicolon ifFalse: [^e].
	(e isKindOf: StMessageNode)
		ifFalse: [self error: 'cascade must follow a message send'].
	cascade := StCascadeNode new.
	cascade receiver: e receiver.
	cascade messages: OrderedCollection new.
	first := StCascadeMessage new.
	first selector: e selector.
	first arguments: e arguments.
	cascade messages add: first.
	[self currentType = #semicolon] whileTrue: [
		self advance.
		cascade messages add: self parseCascadePart].
	^cascade!

parseCascadePart
	"portable-dialect restriction: one message per cascade part"
	| msg args ws |
	msg := StCascadeMessage new.
	self currentType = #identifier ifTrue: [
		msg selector: self advance value asSymbol.
		msg arguments: OrderedCollection new.
		^msg].
	self isBinaryOperator ifTrue: [
		msg selector: self advance value asSymbol.
		msg arguments: (OrderedCollection with: self parseUnaryMessage).
		^msg].
	self currentType = #keyword ifTrue: [
		ws := WriteStream on: String new.
		args := OrderedCollection new.
		[self currentType = #keyword] whileTrue: [
			ws nextPutAll: self advance value.
			args add: self parseBinaryMessage].
		msg selector: ws contents asSymbol.
		msg arguments: args.
		^msg].
	self error: 'malformed cascade part'!

parseKeywordMessage
	| recv ws args |
	recv := self parseBinaryMessage.
	self currentType = #keyword ifFalse: [^recv].
	ws := WriteStream on: String new.
	args := OrderedCollection new.
	[self currentType = #keyword] whileTrue: [
		ws nextPutAll: self advance value.
		args add: self parseBinaryMessage].
	^StMessageNode new
		receiver: recv;
		selector: ws contents asSymbol;
		arguments: args;
		yourself!

parseBinaryMessage
	| recv op arg |
	recv := self parseUnaryMessage.
	[self isBinaryOperator] whileTrue: [
		op := self advance value asSymbol.
		arg := self parseUnaryMessage.
		recv := StMessageNode new
			receiver: recv;
			selector: op;
			arguments: (OrderedCollection with: arg);
			yourself].
	^recv!

parseUnaryMessage
	| recv |
	recv := self parsePrimary.
	[self currentType = #identifier] whileTrue: [
		recv := StMessageNode new
			receiver: recv;
			selector: self advance value asSymbol;
			arguments: OrderedCollection new;
			yourself].
	^recv!

parsePrimary
	| t |
	t := self current.
	t type = #identifier ifTrue: [
		self advance.
		^StVariableNode new name: t value; yourself].
	t type = #integer ifTrue: [
		self advance.
		^StLiteralNode new value: t value; kind: #integer; yourself].
	t type = #string ifTrue: [
		self advance.
		^StLiteralNode new value: t value; kind: #string; yourself].
	t type = #symbol ifTrue: [
		self advance.
		^StLiteralNode new value: t value; kind: #symbol; yourself].
	t type = #character ifTrue: [
		self advance.
		^StLiteralNode new value: t value; kind: #character; yourself].
	t type = #lparen ifTrue: [
		| e |
		self advance.
		e := self parseExpression.
		self expect: #rparen.
		^e].
	t type = #lbracket ifTrue: [^self parseBlock].
	t type = #hashParen ifTrue: [
		self advance.
		^StLiteralNode new value: self parseLiteralArrayBody; kind: #array; yourself].
	t type = #hashBracket ifTrue: [
		self advance.
		^StLiteralNode new value: self parseByteArrayBody; kind: #byteArray; yourself].
	self error: 'unexpected token: ' , t printString!

parseBlock
	| node |
	self expect: #lbracket.
	node := StBlockNode new.
	node arguments: OrderedCollection new.
	node temporaries: OrderedCollection new.
	[self currentType = #blockArg] whileTrue: [
		node arguments add: self advance value].
	node arguments isEmpty ifFalse: [
		self currentType = #rbracket ifFalse: [self expect: #verticalBar]].
	self currentType = #verticalBar ifTrue: [
		node temporaries: self parseTempNames].
	node statements: (self parseStatementsUntil: #rbracket).
	self expect: #rbracket.
	^node!

parseLiteralArrayBody
	| items |
	items := OrderedCollection new.
	[self currentType = #rparen] whileFalse: [
		items add: self parseLiteralArrayElement].
	self advance. "the )"
	^items asArray!

parseLiteralArrayElement
	| t |
	t := self current.
	t type = #integer ifTrue: [self advance. ^t value].
	t type = #string ifTrue: [self advance. ^t value].
	t type = #symbol ifTrue: [self advance. ^t value].
	t type = #character ifTrue: [self advance. ^t value].
	(t type = #binary and: [t value = '-' and: [self peekType = #integer]])
		ifTrue: [
			self advance.
			^0 - self advance value].
	t type = #keyword ifTrue: [
		"adjacent keyword tokens merge into one symbol: #(at:put:)"
		| ws |
		ws := WriteStream on: String new.
		[self currentType = #keyword] whileTrue: [ws nextPutAll: self advance value].
		^ws contents asSymbol].
	t type = #binary ifTrue: [self advance. ^t value asSymbol].
	t type = #verticalBar ifTrue: [self advance. ^#|].
	t type = #identifier ifTrue: [
		self advance.
		t value = 'true' ifTrue: [^true].
		t value = 'false' ifTrue: [^false].
		t value = 'nil' ifTrue: [^nil].
		^t value asSymbol].
	(t type = #hashParen or: [t type = #lparen]) ifTrue: [
		self advance.
		^self parseLiteralArrayBody].
	t type = #hashBracket ifTrue: [
		self advance.
		^self parseByteArrayBody].
	self error: 'bad literal array element: ' , t printString!

parseByteArrayBody
	"answers an OrderedCollection of integers 0..255 (kind #byteArray)"
	| items t |
	items := OrderedCollection new.
	[self currentType = #rbracket] whileFalse: [
		t := self expect: #integer.
		(t value >= 0 and: [t value <= 255])
			ifFalse: [self error: 'byte array element out of range'].
		items add: t value].
	self advance. "the ]"
	^items! !
