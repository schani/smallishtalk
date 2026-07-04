"The cross-compiler driver (SPEC §18): files kernel chunk sources into an
 StImageWriter and emits image files. In cross-compile mode, doIt chunks
 are restricted to class definitions (`Super subclass: #Name
 instanceVariableNames: '...' classVariableNames: '...' poolDictionaries:
 '...' category: '...'`, with `nil subclass: #Object ...` for the root);
 all runtime initialization happens in SystemDictionary>>startUp."

Object subclass: #StImageBuilder
	instanceVariableNames: 'writer'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Compiler'!

!StImageBuilder class methodsFor: 'building'!
new
	| b |
	b := super new.
	b init.
	^b! !

!StImageBuilder methodsFor: 'building'!
init
	writer := StImageWriter new.
	writer init!

writer ^writer!

fileInSource: chunkText
	(StChunkReader eventsFrom: chunkText) do: [:ev |
		ev first = #doIt ifTrue: [self handleDoIt: (ev at: 2)].
		ev first = #methods ifTrue: [
			(ev at: 5) do: [:src |
				writer addMethodSource: src
					toClass: (ev at: 2)
					classSide: (ev at: 3)
					protocol: (ev at: 4)]]]!

fileInFile: aPath
	self fileInSource: (Platform readFile: aPath)!

handleDoIt: source
	"cross-compile mode understands only class definitions"
	| m stmt sup name ivars |
	m := StParser parseDoIt: source.
	m statements isEmpty ifTrue: [^self]. "comment-only chunk"
	(m statements size = 1 and: [(m statements first isKindOf: StMessageNode)])
		ifFalse: [self error: 'unsupported doIt chunk: ' , source].
	stmt := m statements first.
	(stmt selector = #subclass:instanceVariableNames:classVariableNames:poolDictionaries:category:)
		ifFalse: [self error: 'unsupported doIt (not a class definition): ' , source].
	((stmt receiver isKindOf: StVariableNode))
		ifFalse: [self error: 'bad class definition receiver'].
	sup := stmt receiver name = 'nil' ifTrue: [nil] ifFalse: [stmt receiver name].
	name := (stmt arguments at: 1) value asString.
	ivars := self splitWords: (stmt arguments at: 2) value.
	writer defineClass: name superclass: sup instVarNames: ivars
		category: (stmt arguments at: 5) value asString!

splitWords: aString
	| out ws inWord k c |
	out := OrderedCollection new.
	ws := WriteStream on: String new.
	inWord := false.
	k := 1.
	[k <= aString size] whileTrue: [
		c := aString charAt: k.
		(c value = 32 or: [c value = 9 or: [c value = 10 or: [c value = 13]]])
			ifTrue: [
				inWord ifTrue: [out add: ws contents. ws := WriteStream on: String new].
				inWord := false]
			ifFalse: [ws nextPut: c. inWord := true].
		k := k + 1].
	inWord ifTrue: [out add: ws contents].
	^out!

programSource: sourceText
	"compile the corpus program as SystemDictionary>>startUp and poise
	 the initial process on it"
	| spec d startUp |
	d := writer findClass: 'SystemDictionary'.
	d isNil ifTrue: [self error: 'kernel must define SystemDictionary'].
	spec := StCodeGen compileDoItSource: sourceText
		inClass: (writer classScopeFor: 'SystemDictionary' classSide: false).
	spec selector: #startUp.
	startUp := writer materializeMethod: spec forClassDesc: d.
	d methodSelectors add: #startUp.
	d methodObjs add: startUp.
	writer wireClassObjects.
	writer buildSpecialsWithStartUp: startUp!

imageBytes
	^writer imageBytes!

writeTo: aPath
	writer writeTo: aPath! !
