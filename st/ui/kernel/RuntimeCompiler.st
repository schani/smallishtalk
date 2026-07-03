"Runtime compilation (UI.md §9.3): reify the in-image compiler's output (an
 StCodeSpec) into a LIVE CompiledMethod/CompiledBlock that PRIM_METHOD_INSTALL
 (402) can install into the running image. This is what makes the Class Browser
 live: 'accept' recompiles a method and do-it/print-it evaluate an expression.

 Globals compile to `LOADK <Association>` + a #value send (CodeGen.st), so a
 global reference reifies to a fresh `name -> resolvedValue` Association — reads
 resolve correctly (the browser recompiles bodies that reference existing
 classes). Requires the compiler (StParser/StCodeGen/StEncoder + Treaty) filed
 into the image, and CompiledMethod/CompiledBlock declaring their real ivars so
 `new` allocates the right slot count."

Object subclass: #RuntimeCompiler
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Reflection'!

!RuntimeCompiler class methodsFor: 'compiling'!
compile: sourceString in: aClass
	"a live instance-side CompiledMethod for aClass."
	| spec |
	spec := StCodeGen compileMethodSource: sourceString inClass: (self scopeFor: aClass).
	^self reify: spec in: aClass!
compileDoIt: sourceString in: aClass
	"a live 0-arg #DoIt method evaluating sourceString with self=receiver."
	| spec |
	spec := StCodeGen compileDoItSource: sourceString inClass: (self scopeFor: aClass).
	spec selector: #DoIt.
	^self reify: spec in: aClass!
scopeFor: aClass
	^StClassScope name: aClass name asString instVarNames: (self allInstVarNamesOf: aClass)!
allInstVarNamesOf: aClass
	| out sup |
	out := OrderedCollection new.
	sup := aClass superclass.
	sup notNil ifTrue: [out addAll: (self allInstVarNamesOf: sup)].
	aClass instVarNames notNil ifTrue: [out addAll: aClass instVarNames].
	^out! !

!RuntimeCompiler class methodsFor: 'reifying'!
reify: spec in: aClass
	| m |
	m := CompiledMethod new.
	m instVarAt: 1 put: (self packHeader: spec).
	m instVarAt: 2 put: (self bytecodesFor: spec).
	m instVarAt: 3 put: (self literalsFor: spec outer: m in: aClass).
	m instVarAt: 4 put: (self sitesFor: spec in: aClass).
	m instVarAt: 5 put: spec selector.
	m instVarAt: 6 put: aClass.
	m instVarAt: 7 put: nil.
	^m!
reifyBlock: spec outer: outerObj in: aClass
	| b info |
	b := CompiledBlock new.
	b instVarAt: 1 put: (self packHeader: spec).
	b instVarAt: 2 put: (self bytecodesFor: spec).
	b instVarAt: 3 put: (self literalsFor: spec outer: b in: aClass).
	b instVarAt: 4 put: (self sitesFor: spec in: aClass).
	b instVarAt: 5 put: outerObj.
	info := spec numCaptured bitOr:
		((spec hasNlr ifTrue: [1] ifFalse: [0]) bitShift: Treaty biHasNlrShift).
	b instVarAt: 6 put: info.
	^b!
packHeader: spec
	| prim hasPrim |
	prim := spec primitive isNil ifTrue: [0] ifFalse: [spec primitive].
	hasPrim := spec primitive isNil ifTrue: [0] ifFalse: [1].
	^spec frameSlots
		+ (spec argc bitShift: Treaty mhArgcShift)
		+ (prim bitShift: Treaty mhPrimitiveShift)
		+ (hasPrim bitShift: Treaty mhHasPrimitiveShift)
		+ (spec handlerSlotBase bitShift: Treaty mhHandlerSlotBaseShift)
		+ (spec mhFlags bitShift: Treaty mhFlagsShift)!
bytecodesFor: spec
	| bytes ba i |
	bytes := StEncoder bytesForAll: spec insns.
	ba := ByteArray new: bytes size.
	i := 1.
	bytes do: [:b | ba at: i put: b. i := i + 1].
	^ba!
literalsFor: spec outer: methodObj in: aClass
	| arr |
	arr := Array new: spec literals size.
	1 to: spec literals size do: [:i |
		arr at: i put: (self reifyLiteral: (spec literals at: i) outer: methodObj in: aClass)].
	^arr!
reifyLiteral: v outer: methodObj in: aClass
	v isNil ifTrue: [^nil].
	v == true ifTrue: [^true].
	v == false ifTrue: [^false].
	(v isKindOf: StBlockSpec) ifTrue: [^self reifyBlock: v outer: methodObj in: aClass].
	(v isKindOf: StGlobalRef) ifTrue: [^self associationFor: v name].
	(v isKindOf: StCharCode) ifTrue: [^Character value: v code].
	(v isKindOf: Integer) ifTrue: [^v].
	(v isKindOf: Symbol) ifTrue: [^v].
	(v isKindOf: String) ifTrue: [^v].
	(v isKindOf: Array) ifTrue: [
		(v size = 2 and: [(v at: 1) == #byteArray]) ifTrue: [^self byteArrayLiteral: (v at: 2)].
		^self reifyArray: v outer: methodObj in: aClass].
	^self error: 'cannot reify literal ' , v printString!
reifyArray: v outer: methodObj in: aClass
	| arr |
	arr := Array new: v size.
	1 to: v size do: [:i |
		arr at: i put: (self reifyLiteral: (v at: i) outer: methodObj in: aClass)].
	^arr!
byteArrayLiteral: src
	| ba j |
	ba := ByteArray new: src size.
	j := 1.
	src do: [:b | ba at: j put: b. j := j + 1].
	^ba!
associationFor: aName
	| a |
	a := Association new.
	a key: aName asSymbol.
	a value: (self resolveGlobal: aName).
	^a!
resolveGlobal: aName
	| c |
	c := Smalltalk classNamed: aName.
	c notNil ifTrue: [^c].
	aName = 'Smalltalk' ifTrue: [^Smalltalk].
	aName = 'Transcript' ifTrue: [^Smalltalk classNamed: 'TranscriptStream'].
	^nil!
sitesFor: spec in: aClass
	| arr stride base site |
	stride := Treaty siteStride.
	arr := Array new: spec sites size * stride.
	1 to: spec sites size do: [:i |
		site := spec sites at: i.
		base := (i - 1) * stride.
		arr at: base + Treaty siteSelector + 1 put: site selector.
		arr at: base + Treaty siteArgc + 1 put: site argc.
		arr at: base + Treaty siteCacheClass + 1 put: 0.
		arr at: base + Treaty siteCacheMethod + 1 put: nil.
		arr at: base + Treaty siteStaticClass + 1 put: (site isSuper ifTrue: [aClass] ifFalse: [nil])].
	^arr! !
