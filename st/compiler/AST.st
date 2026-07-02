"AST nodes (SPEC §18): the parser's output, consumed by capture analysis
 and the code generator. Every node prints a canonical s-expression via
 #sexp — the structural-equality representation the tests compare.

 Literal values are host objects: Integer, String, Symbol, Boolean, nil,
 Array (of literal values), ByteArray-as-Array-of-integers tagged by
 StLiteralNode kind - and StCharCode for character literals: the code
 point travels as a SmallInteger end to end, never as a host Character
 (GST's Character/UnicodeCharacter split and its equality semantics
 differ from the self-hosted kernel's; integers are host-identical)."

Object subclass: #StCharCode
	instanceVariableNames: 'code'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

!StCharCode class methodsFor: 'instance creation'!
code: anInteger
	| c |
	c := self new.
	c setCode: anInteger.
	^c! !

!StCharCode methodsFor: 'accessing'!
setCode: anInteger
	code := anInteger!
code
	^code!
= other
	^(other isKindOf: StCharCode) and: [code = other code]!
hash
	^code!
printOn: aStream
	aStream nextPutAll: '$<' , code printString , '>'! !

Object subclass: #StNode
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

!StNode methodsFor: 'printing'!
sexp
	self subclassResponsibility!
printOn: aStream
	aStream nextPutAll: self sexp! !

StNode subclass: #StMethodNode
	instanceVariableNames: 'selector arguments temporaries pragmas statements'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

!StMethodNode methodsFor: 'accessing'!
selector ^selector!
selector: aSymbol selector := aSymbol!
arguments ^arguments!
arguments: aCollection arguments := aCollection!
temporaries ^temporaries!
temporaries: aCollection temporaries := aCollection!
pragmas ^pragmas!
pragmas: aCollection pragmas := aCollection!
statements ^statements!
statements: aCollection statements := aCollection! !

!StMethodNode methodsFor: 'printing'!
sexp
	| ws |
	ws := WriteStream on: String new.
	ws nextPutAll: '(method '; nextPutAll: selector.
	ws nextPutAll: ' ('.
	arguments do: [:a | ws nextPutAll: a] separatedBy: [ws nextPut: $ ].
	ws nextPutAll: ') ('.
	temporaries do: [:t | ws nextPutAll: t] separatedBy: [ws nextPut: $ ].
	ws nextPutAll: ')'.
	pragmas do: [:p | ws nextPut: $ ; nextPutAll: p sexp].
	statements do: [:s | ws nextPut: $ ; nextPutAll: s sexp].
	ws nextPut: $).
	^ws contents! !

StNode subclass: #StPragmaNode
	instanceVariableNames: 'selector arguments'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

!StPragmaNode methodsFor: 'accessing'!
selector ^selector!
selector: aSymbol selector := aSymbol!
arguments ^arguments!
arguments: aCollection arguments := aCollection! !

!StPragmaNode methodsFor: 'printing'!
sexp
	| ws |
	ws := WriteStream on: String new.
	ws nextPutAll: '(pragma '; nextPutAll: selector.
	arguments do: [:a | ws nextPut: $ ; nextPutAll: (StLiteralNode printValue: a)].
	ws nextPut: $).
	^ws contents! !

StNode subclass: #StReturnNode
	instanceVariableNames: 'expression'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

!StReturnNode methodsFor: 'accessing'!
expression ^expression!
expression: aNode expression := aNode! !

!StReturnNode methodsFor: 'printing'!
sexp
	^'(return ' , expression sexp , ')'! !

StNode subclass: #StAssignmentNode
	instanceVariableNames: 'variable expression'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

!StAssignmentNode methodsFor: 'accessing'!
variable ^variable!
variable: aString variable := aString!
expression ^expression!
expression: aNode expression := aNode! !

!StAssignmentNode methodsFor: 'printing'!
sexp
	^'(assign ' , variable , ' ' , expression sexp , ')'! !

StNode subclass: #StVariableNode
	instanceVariableNames: 'name'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

!StVariableNode methodsFor: 'accessing'!
name ^name!
name: aString name := aString! !

!StVariableNode methodsFor: 'printing'!
sexp
	^'(var ' , name , ')'! !

StNode subclass: #StLiteralNode
	instanceVariableNames: 'value kind'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

"kind is one of: #integer #string #symbol #character #array #byteArray"

!StLiteralNode class methodsFor: 'printing'!
printValue: v
	v isNil ifTrue: [^'nil'].
	v == true ifTrue: [^'true'].
	v == false ifTrue: [^'false'].
	(v isKindOf: Integer) ifTrue: [^v printString].
	(v isKindOf: Symbol) ifTrue: [^'#' , v].
	(v isKindOf: String) ifTrue: [^'''' , v , ''''].
	(v isKindOf: StCharCode) ifTrue: [
		(v code >= 32 and: [v code <= 126])
			ifTrue: [^'$' , (String with: (Character value: v code))]
			ifFalse: [^'$<' , v code printString , '>']].
	(v isKindOf: Array) ifTrue: [
		| ws |
		ws := WriteStream on: String new.
		ws nextPutAll: '#('.
		v do: [:e | ws nextPutAll: (self printValue: e)] separatedBy: [ws nextPut: $ ].
		ws nextPut: $).
		^ws contents].
	^v printString! !

!StLiteralNode methodsFor: 'accessing'!
value ^value!
value: v value := v!
kind ^kind!
kind: aSymbol kind := aSymbol! !

!StLiteralNode methodsFor: 'printing'!
sexp
	kind = #byteArray ifTrue: [
		| ws |
		ws := WriteStream on: String new.
		ws nextPutAll: '(lit #['.
		value do: [:e | ws nextPutAll: e printString] separatedBy: [ws nextPut: $ ].
		ws nextPutAll: '])'.
		^ws contents].
	^'(lit ' , (StLiteralNode printValue: value) , ')'! !

StNode subclass: #StMessageNode
	instanceVariableNames: 'receiver selector arguments'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

!StMessageNode methodsFor: 'accessing'!
receiver ^receiver!
receiver: aNode receiver := aNode!
selector ^selector!
selector: aSymbol selector := aSymbol!
arguments ^arguments!
arguments: aCollection arguments := aCollection! !

!StMessageNode methodsFor: 'testing'!
isSuperSend
	^(receiver isKindOf: StVariableNode) and: [receiver name = 'super']! !

!StMessageNode methodsFor: 'printing'!
sexp
	| ws |
	ws := WriteStream on: String new.
	ws nextPutAll: '(send '; nextPutAll: receiver sexp;
		nextPut: $ ; nextPutAll: selector.
	arguments do: [:a | ws nextPut: $ ; nextPutAll: a sexp].
	ws nextPut: $).
	^ws contents! !

StNode subclass: #StCascadeNode
	instanceVariableNames: 'receiver messages'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

"messages: OrderedCollection of StCascadeMessage (selector + arguments),
 all sent to the same receiver value; the cascade's value is the last's."

!StCascadeNode methodsFor: 'accessing'!
receiver ^receiver!
receiver: aNode receiver := aNode!
messages ^messages!
messages: aCollection messages := aCollection! !

!StCascadeNode methodsFor: 'printing'!
sexp
	| ws |
	ws := WriteStream on: String new.
	ws nextPutAll: '(cascade '; nextPutAll: receiver sexp.
	messages do: [:m | ws nextPut: $ ; nextPutAll: m sexp].
	ws nextPut: $).
	^ws contents! !

StNode subclass: #StCascadeMessage
	instanceVariableNames: 'selector arguments'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

!StCascadeMessage methodsFor: 'accessing'!
selector ^selector!
selector: aSymbol selector := aSymbol!
arguments ^arguments!
arguments: aCollection arguments := aCollection! !

!StCascadeMessage methodsFor: 'printing'!
sexp
	| ws |
	ws := WriteStream on: String new.
	ws nextPutAll: '('; nextPutAll: selector.
	arguments do: [:a | ws nextPut: $ ; nextPutAll: a sexp].
	ws nextPut: $).
	^ws contents! !

StNode subclass: #StBlockNode
	instanceVariableNames: 'arguments temporaries statements capturedNames hasNlr'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

"capturedNames and hasNlr are filled by capture analysis (codegen phase)."

!StBlockNode methodsFor: 'accessing'!
arguments ^arguments!
arguments: aCollection arguments := aCollection!
temporaries ^temporaries!
temporaries: aCollection temporaries := aCollection!
statements ^statements!
statements: aCollection statements := aCollection!
capturedNames ^capturedNames!
capturedNames: aCollection capturedNames := aCollection!
hasNlr ^hasNlr!
hasNlr: aBoolean hasNlr := aBoolean! !

!StBlockNode methodsFor: 'printing'!
sexp
	| ws |
	ws := WriteStream on: String new.
	ws nextPutAll: '(block ('.
	arguments do: [:a | ws nextPutAll: a] separatedBy: [ws nextPut: $ ].
	ws nextPutAll: ') ('.
	temporaries do: [:t | ws nextPutAll: t] separatedBy: [ws nextPut: $ ].
	ws nextPut: $).
	statements do: [:s | ws nextPut: $ ; nextPutAll: s sexp].
	ws nextPut: $).
	^ws contents! !
