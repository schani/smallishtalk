"Heap writer (SPEC §18 part 3, §17): builds the initial object graph —
 classes, metaclasses, method dictionaries, symbols, literals, the special
 objects array, the poised initial Process and its stack — and emits a
 STIM image file.

 Layout: old space starts at savedBaseAddress 16r10000. All 64-bit word
 emission goes through two 32-bit halves with bitAnd:/bitShift: so the
 identical code runs on the self-hosted 63-bit-SmallInteger image (no
 LargeIntegers in v1).

 Determinism (§18): objects are emitted in creation order; symbols intern
 in first-encounter order; classes in definition order; nil, true, false
 are the first three objects."

Object subclass: #StObj
	instanceVariableNames: 'classIndex format slots bytes address immutable'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

"format: #fixed #ptrs #bytes. slots hold Integers (tagged as SmallInts)
 or StObj references; bytes hold 0..255."

!StObj class methodsFor: 'creation'!
fixed: classIndex slots: n
	| o |
	o := self new.
	o setClass: classIndex format: #fixed.
	o initSlots: n.
	^o!

ptrs: classIndex slots: n
	| o |
	o := self new.
	o setClass: classIndex format: #ptrs.
	o initSlots: n.
	^o!

bytes: classIndex contents: aByteCollection
	| o |
	o := self new.
	o setClass: classIndex format: #bytes.
	o setBytes: aByteCollection.
	^o! !

!StObj methodsFor: 'accessing'!
setClass: idx format: f
	classIndex := idx.
	format := f.
	immutable := false!
initSlots: n
	slots := OrderedCollection new.
	n timesRepeat: [slots add: #nilSlot]!
setBytes: aByteCollection
	bytes := aByteCollection!
classIndex ^classIndex!
setClassIndex: idx classIndex := idx!
format ^format!
slots ^slots!
bytesContents ^bytes!
at: i put: v
	slots at: i put: v!
at: i
	^slots at: i!
address ^address!
address: a address := a!
beImmutable immutable := true!
isImmutable ^immutable!
numSlots
	format = #bytes ifTrue: [^(bytes size + 7) // 8].
	^slots size!
byteSize
	^bytes size!
footprintWords
	"header + body (+ overflow word for >= 255 slots)"
	| n |
	n := self numSlots.
	^n >= 255 ifTrue: [n + 2] ifFalse: [n + 1]! !

Object subclass: #StClassDesc
	instanceVariableNames: 'name superName classIndex instVarNames format
classObj metaObj metaIndex methodSelectors methodObjs classMethodSelectors classMethodObjs'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

!StClassDesc methodsFor: 'accessing'!
name ^name!
name: n name := n!
superName ^superName!
superName: n superName := n!
classIndex ^classIndex!
classIndex: i classIndex := i!
metaIndex ^metaIndex!
metaIndex: i metaIndex := i!
instVarNames ^instVarNames!
instVarNames: c instVarNames := c!
format ^format!
format: f format := f!
classObj ^classObj!
classObj: o classObj := o!
metaObj ^metaObj!
metaObj: o metaObj := o!
methodSelectors ^methodSelectors!
methodObjs ^methodObjs!
classMethodSelectors ^classMethodSelectors!
classMethodObjs ^classMethodObjs!
initMethods
	methodSelectors := OrderedCollection new.
	methodObjs := OrderedCollection new.
	classMethodSelectors := OrderedCollection new.
	classMethodObjs := OrderedCollection new! !

Object subclass: #StImageWriter
	instanceVariableNames: 'objects nilObj trueObj falseObj symbolNames symbolObjs
classes classOrder nextClassIndex globalNames globalAssocs smalltalkObj
processObj stackObj specialsObj charObjs'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

!StImageWriter class methodsFor: 'constants'!
savedBase
	^16r10000!

treatyIndexFor: className
	className = 'Object' ifTrue: [^Treaty classObject].
	className = 'Behavior' ifTrue: [^Treaty classBehavior].
	className = 'Class' ifTrue: [^Treaty classClass].
	className = 'Metaclass' ifTrue: [^Treaty classMetaclass].
	className = 'UndefinedObject' ifTrue: [^Treaty classUndefinedObject].
	className = 'True' ifTrue: [^Treaty classTrue].
	className = 'False' ifTrue: [^Treaty classFalse].
	className = 'SmallInteger' ifTrue: [^Treaty classSmallinteger].
	className = 'Float' ifTrue: [^Treaty classFloat].
	className = 'Character' ifTrue: [^Treaty classCharacter].
	className = 'String' ifTrue: [^Treaty classString].
	className = 'ByteString' ifTrue: [^Treaty classBytestring].
	className = 'Symbol' ifTrue: [^Treaty classSymbol].
	className = 'LargePositiveInteger' ifTrue: [^Treaty classLargePositiveInteger].
	className = 'LargeNegativeInteger' ifTrue: [^Treaty classLargeNegativeInteger].
	className = 'Array' ifTrue: [^Treaty classArray].
	className = 'ByteArray' ifTrue: [^Treaty classBytearray].
	className = 'OrderedCollection' ifTrue: [^Treaty classOrderedCollection].
	className = 'Association' ifTrue: [^Treaty classAssociation].
	className = 'Box' ifTrue: [^Treaty classBox].
	className = 'BlockClosure' ifTrue: [^Treaty classBlockclosure].
	className = 'CompiledMethod' ifTrue: [^Treaty classCompiledmethod].
	className = 'CompiledBlock' ifTrue: [^Treaty classCompiledblock].
	className = 'Process' ifTrue: [^Treaty classProcess].
	className = 'Semaphore' ifTrue: [^Treaty classSemaphore].
	className = 'MethodDictionary' ifTrue: [^Treaty classMethoddictionary].
	className = 'ProcessorScheduler' ifTrue: [^Treaty classProcessorScheduler].
	className = 'Message' ifTrue: [^Treaty classMessage].
	className = 'SystemDictionary' ifTrue: [^Treaty classSystemDictionary].
	className = 'Stack' ifTrue: [^Treaty classStack].
	className = 'LinkedList' ifTrue: [^Treaty classLinkedList].
	^nil!

builtinFormatFor: className
	"instance format for treaty classes that are not plain fixed"
	(#('String' 'ByteString' 'Symbol' 'ByteArray' 'Float'
	   'LargePositiveInteger' 'LargeNegativeInteger') includes: className)
		ifTrue: [^#bytes].
	(#('Array' 'Stack') includes: className) ifTrue: [^#ptrs].
	^#fixed! !

!StImageWriter methodsFor: 'initialization'!
init
	objects := OrderedCollection new.
	symbolNames := OrderedCollection new.
	symbolObjs := OrderedCollection new.
	classes := OrderedCollection new.	"of StClassDesc, definition order"
	nextClassIndex := Treaty firstUnreservedClassIndex.
	globalNames := OrderedCollection new.
	globalAssocs := OrderedCollection new.
	charObjs := OrderedCollection new.	"(code . obj) pairs"
	nilObj := self addObj: (StObj fixed: Treaty classUndefinedObject slots: 0).
	trueObj := self addObj: (StObj fixed: Treaty classTrue slots: 0).
	falseObj := self addObj: (StObj fixed: Treaty classFalse slots: 0)!

addObj: obj
	objects add: obj.
	^obj!

nilObj ^nilObj!
trueObj ^trueObj!
falseObj ^falseObj!
objects ^objects!
classes ^classes! !

!StImageWriter methodsFor: 'symbols'!
symbolFor: aStringOrSymbol
	"intern in first-encounter order"
	| s i o |
	s := aStringOrSymbol asString.
	i := 1.
	symbolNames do: [:n |
		n = s ifTrue: [^symbolObjs at: i].
		i := i + 1].
	o := self addObj: (StObj bytes: Treaty classSymbol contents: (Platform bytesOf: s)).
	o beImmutable.
	symbolNames add: s.
	symbolObjs add: o.
	^o!

stringFor: aString
	| o |
	o := self addObj: (StObj bytes: Treaty classBytestring contents: (Platform bytesOf: aString)).
	o beImmutable.
	^o!

characterFor: aCharacter
	| o |
	charObjs do: [:pair |
		(pair at: 1) = aCharacter value ifTrue: [^pair at: 2]].
	o := self addObj: (StObj fixed: Treaty classCharacter slots: 1).
	o at: 1 put: aCharacter value.
	charObjs add: (Array with: aCharacter value with: o).
	^o!

globalAssocFor: aName
	"one shared Association per global name; key is the Symbol"
	| i a |
	i := 1.
	globalNames do: [:n |
		n = aName ifTrue: [^globalAssocs at: i].
		i := i + 1].
	a := self addObj: (StObj fixed: Treaty classAssociation slots: 2).
	a at: 1 put: (self symbolFor: aName).
	a at: 2 put: nilObj.
	globalNames add: aName.
	globalAssocs add: a.
	^a! !

!StImageWriter methodsFor: 'classes'!
findClass: aName
	classes do: [:d | d name = aName ifTrue: [^d]].
	^nil!

defineClass: aName superclass: superName instVarNames: ivarList
	| d treatyIdx fmt |
	(self findClass: aName) notNil ifTrue: [self error: 'class redefined: ' , aName].
	d := StClassDesc new.
	d name: aName.
	d superName: superName.
	d instVarNames: ivarList asOrderedCollection.
	d initMethods.
	treatyIdx := StImageWriter treatyIndexFor: aName.
	d classIndex: (treatyIdx isNil ifTrue: [
			| idx | idx := nextClassIndex. nextClassIndex := nextClassIndex + 1. idx]
		ifFalse: [treatyIdx]).
	fmt := StImageWriter builtinFormatFor: aName.
	d format: fmt.
	d metaIndex: nextClassIndex.
	nextClassIndex := nextClassIndex + 1.
	"class and metaclass objects exist from definition time so methods
	 can reference them; slots are wired in wireClassObjects"
	d classObj: (self addObj: (StObj fixed: d metaIndex slots: 6)).
	d metaObj: (self addObj: (StObj fixed: Treaty classMetaclass slots: 6)).
	classes add: d.
	^d!

allInstVarNamesOf: d
	"inherited first, then own — the codegen scope order"
	| out sup |
	out := OrderedCollection new.
	sup := d superName isNil ifTrue: [nil] ifFalse: [self findClass: d superName].
	sup notNil ifTrue: [out addAll: (self allInstVarNamesOf: sup)].
	out addAll: d instVarNames.
	^out!

classScopeFor: aName classSide: isMeta
	| d |
	d := self findClass: aName.
	d isNil ifTrue: [self error: 'unknown class ' , aName].
	isMeta ifTrue: [
		"class-side methods: the receiver is the class object; its
		 named slots (Behavior layout) are not addressed by name in v1"
		^StClassScope name: aName , ' class' instVarNames: OrderedCollection new].
	^StClassScope name: aName instVarNames: (self allInstVarNamesOf: d)!

addMethodSource: source toClass: aName classSide: isMeta
	| d spec obj |
	d := self findClass: aName.
	d isNil ifTrue: [self error: 'unknown class ' , aName].
	spec := StCodeGen compileMethodSource: source
		inClass: (self classScopeFor: aName classSide: isMeta).
	obj := self materializeMethod: spec forClassDesc: d classSide: isMeta.
	isMeta
		ifTrue: [
			d classMethodSelectors add: spec selector.
			d classMethodObjs add: obj]
		ifFalse: [
			d methodSelectors add: spec selector.
			d methodObjs add: obj].
	^obj! !

!StImageWriter methodsFor: 'materializing'!
materializeMethod: spec forClassDesc: d
	^self materializeMethod: spec forClassDesc: d classSide: false!

materializeMethod: spec forClassDesc: d classSide: isMeta
	| m header |
	m := self addObj: (StObj fixed: Treaty classCompiledmethod slots: Treaty methodNumSlots).
	header := self packHeaderFor: spec.
	m at: Treaty methodHeader + 1 put: header.
	m at: Treaty methodBytecodes + 1 put: (self bytecodesObjFor: spec).
	m at: Treaty methodLiterals + 1 put: (self literalsObjFor: spec method: m classDesc: d classSide: isMeta).
	m at: Treaty methodSendSites + 1 put: (self sitesObjFor: spec classDesc: d classSide: isMeta).
	m at: Treaty methodSelector + 1
		put: (spec selector isNil ifTrue: [nilObj] ifFalse: [self symbolFor: spec selector]).
	m at: Treaty methodClass + 1
		put: (d isNil
			ifTrue: [nilObj]
			ifFalse: [isMeta ifTrue: [d metaObj] ifFalse: [d classObj]]).
	m at: Treaty methodSourceInfo + 1 put: nilObj.
	^m!

materializeBlock: spec outer: outerObj classDesc: d classSide: isMeta
	| b info |
	b := self addObj: (StObj fixed: Treaty classCompiledblock slots: Treaty blockNumSlots).
	b at: Treaty blockHeader + 1 put: (self packHeaderFor: spec).
	b at: Treaty blockBytecodes + 1 put: (self bytecodesObjFor: spec).
	b at: Treaty blockLiterals + 1 put: (self literalsObjFor: spec method: b classDesc: d classSide: isMeta).
	b at: Treaty blockSendSites + 1 put: (self sitesObjFor: spec classDesc: d classSide: isMeta).
	b at: Treaty blockOuterMethod + 1 put: outerObj.
	info := spec numCaptured bitOr:
		((spec hasNlr ifTrue: [1] ifFalse: [0]) bitShift: Treaty biHasNlrShift).
	b at: Treaty blockInfo + 1 put: info.
	^b!

packHeaderFor: spec
	| bits prim hasPrim |
	prim := spec primitive isNil ifTrue: [0] ifFalse: [spec primitive].
	hasPrim := spec primitive isNil ifTrue: [0] ifFalse: [1].
	bits := spec frameSlots
		+ (spec argc bitShift: Treaty mhArgcShift)
		+ (prim bitShift: Treaty mhPrimitiveShift)
		+ (hasPrim bitShift: Treaty mhHasPrimitiveShift)
		+ (spec handlerSlotBase bitShift: Treaty mhHandlerSlotBaseShift)
		+ (spec mhFlags bitShift: Treaty mhFlagsShift).
	^bits!

bytecodesObjFor: spec
	| o |
	o := self addObj: (StObj bytes: Treaty classBytearray
		contents: (StEncoder bytesForAll: spec insns)).
	o beImmutable.
	^o!

literalsObjFor: spec method: methodObj classDesc: d classSide: isMeta
	| arr |
	arr := self addObj: (StObj ptrs: Treaty classArray slots: spec literals size).
	arr beImmutable.
	1 to: spec literals size do: [:i |
		arr at: i put: (self materializeLiteral: (spec literals at: i)
			method: methodObj classDesc: d classSide: isMeta)].
	^arr!

materializeLiteral: v method: methodObj classDesc: d classSide: isMeta
	v isNil ifTrue: [^nilObj].
	v == true ifTrue: [^trueObj].
	v == false ifTrue: [^falseObj].
	(v isKindOf: StBlockSpec) ifTrue: [
		^self materializeBlock: v outer: methodObj classDesc: d classSide: isMeta].
	(v isKindOf: StGlobalRef) ifTrue: [^self globalAssocFor: v name].
	(v isKindOf: Integer) ifTrue: [^v].
	(v isKindOf: Symbol) ifTrue: [^self symbolFor: v].
	(v isKindOf: String) ifTrue: [^self stringFor: v].
	(v isKindOf: Character) ifTrue: [^self characterFor: v].
	(v isKindOf: Array) ifTrue: [
		(v size = 2 and: [(v at: 1) == #byteArray]) ifTrue: [
			| o |
			o := self addObj: (StObj bytes: Treaty classBytearray
				contents: (v at: 2) asOrderedCollection).
			o beImmutable.
			^o].
		^self arrayLiteral: v method: methodObj classDesc: d classSide: isMeta].
	self error: 'cannot materialize literal ' , v printString!

arrayLiteral: v method: methodObj classDesc: d classSide: isMeta
	| arr |
	arr := self addObj: (StObj ptrs: Treaty classArray slots: v size).
	arr beImmutable.
	1 to: v size do: [:i |
		arr at: i put: (self materializeLiteral: (v at: i)
			method: methodObj classDesc: d classSide: isMeta)].
	^arr!

sitesObjFor: spec classDesc: d classSide: isMeta
	| arr base |
	arr := self addObj: (StObj ptrs: Treaty classArray
		slots: spec sites size * Treaty siteStride).
	1 to: spec sites size do: [:i |
		| site |
		site := spec sites at: i.
		base := (i - 1) * Treaty siteStride.
		arr at: base + Treaty siteSelector + 1 put: (self symbolFor: site selector).
		arr at: base + Treaty siteArgc + 1 put: site argc.
		arr at: base + Treaty siteCacheClass + 1 put: 0.
		arr at: base + Treaty siteCacheMethod + 1 put: nilObj.
		arr at: base + Treaty siteStaticClass + 1
			put: (site isSuper
				ifTrue: [d isNil
					ifTrue: [self error: 'super send outside a class']
					ifFalse: [isMeta ifTrue: [d metaObj] ifFalse: [d classObj]]]
				ifFalse: [nilObj])].
	^arr! !

!StImageWriter methodsFor: 'finalizing'!
formatCodeFor: d
	d format = #ptrs ifTrue: [^Treaty fmtPtrs].
	d format = #bytes ifTrue: [^Treaty fmtBytesBase].
	^Treaty fmtFixed!

wireClassObjects
	classes do: [:d |
		| c m sup nIvars |
		c := d classObj.
		m := d metaObj.
		sup := d superName isNil ifTrue: [nil] ifFalse: [self findClass: d superName].
		nIvars := (self allInstVarNamesOf: d) size.
		c at: Treaty behaviorSuperclass + 1
			put: (sup isNil ifTrue: [nilObj] ifFalse: [sup classObj]).
		c at: Treaty behaviorMethodDictionary + 1
			put: (self mdictFor: d methodSelectors objs: d methodObjs).
		c at: Treaty behaviorFormatAndSlots + 1
			put: ((self formatCodeFor: d) bitShift: Treaty formatAndSlotsFormatShift) + nIvars.
		c at: Treaty behaviorClassIndex + 1 put: d classIndex.
		c at: 5 put: (self symbolFor: d name).
		c at: 6 put: (self ivarNamesArrayFor: d).
		"the metaclass"
		m at: Treaty behaviorSuperclass + 1
			put: (sup isNil
				ifTrue: [(self findClass: 'Class') notNil
					ifTrue: [(self findClass: 'Class') classObj]
					ifFalse: [nilObj]]
				ifFalse: [sup metaObj]).
		m at: Treaty behaviorMethodDictionary + 1
			put: (self mdictFor: d classMethodSelectors objs: d classMethodObjs).
		m at: Treaty behaviorFormatAndSlots + 1
			put: (Treaty fmtFixed bitShift: Treaty formatAndSlotsFormatShift) + 6.
		m at: Treaty behaviorClassIndex + 1 put: d metaIndex.
		m at: 5 put: (self symbolFor: d name , ' class').
		m at: 6 put: (self addObj: (StObj ptrs: Treaty classArray slots: 0)).
		"class-name global binding"
		(self globalAssocFor: d name) at: 2 put: c]!

ivarNamesArrayFor: d
	| arr names |
	names := d instVarNames.
	arr := self addObj: (StObj ptrs: Treaty classArray slots: names size).
	1 to: names size do: [:i |
		arr at: i put: (self symbolFor: (names at: i))].
	^arr!

mdictFor: selectors objs: methodObjs
	| keys vals md |
	keys := self addObj: (StObj ptrs: Treaty classArray slots: selectors size).
	vals := self addObj: (StObj ptrs: Treaty classArray slots: selectors size).
	1 to: selectors size do: [:i |
		keys at: i put: (self symbolFor: (selectors at: i)).
		vals at: i put: (methodObjs at: i)].
	md := self addObj: (StObj fixed: Treaty classMethoddictionary
		slots: Treaty mdictNumVmSlots).
	md at: Treaty mdictKeys + 1 put: keys.
	md at: Treaty mdictValues + 1 put: vals.
	^md!

buildSpecialsWithStartUp: startUpObj
	"scheduler, semaphores, class list, symbol table, specialized
	 selectors, the poised process, and the special objects array"
	| sched queues classList maxIdx symtab specSels sels lowSem timerSem dnuSym mbbSym |
	dnuSym := self symbolFor: 'doesNotUnderstand:'.
	mbbSym := self symbolFor: 'mustBeBoolean'.
	smalltalkObj := self addObj: (StObj fixed: Treaty classSystemDictionary slots: 0).
	(self globalAssocFor: 'Smalltalk') at: 2 put: smalltalkObj.

	queues := self addObj: (StObj ptrs: Treaty classArray slots: Treaty numPriorities).
	1 to: Treaty numPriorities do: [:i |
		| q |
		q := self addObj: (StObj fixed: Treaty classLinkedList slots: Treaty listNumVmSlots).
		queues at: i put: q].
	sched := self addObj: (StObj fixed: Treaty classProcessorScheduler
		slots: Treaty schedulerNumVmSlots).
	sched at: Treaty schedulerQueues + 1 put: queues.
	(self globalAssocFor: 'Processor') at: 2 put: sched.
	"Transcript is the TranscriptStream class itself (class-side protocol)"
	(self findClass: 'TranscriptStream') notNil ifTrue: [
		(self globalAssocFor: 'Transcript') at: 2
			put: (self findClass: 'TranscriptStream') classObj].

	maxIdx := 0.
	classes do: [:d |
		d classIndex > maxIdx ifTrue: [maxIdx := d classIndex].
		d metaIndex > maxIdx ifTrue: [maxIdx := d metaIndex]].
	classList := self addObj: (StObj ptrs: Treaty classArray slots: maxIdx + 1).
	classes do: [:d |
		classList at: d classIndex + 1 put: d classObj.
		classList at: d metaIndex + 1 put: d metaObj].

	sels := OrderedCollection new.
	sels add: '+'; add: '-'; add: '*'; add: '//'; add: '\\'.
	sels add: '<'; add: '>'; add: '<='; add: '>='; add: '='; add: '=='.
	sels add: 'at:'; add: 'at:put:'; add: 'size'; add: 'class'; add: 'not'.
	specSels := self addObj: (StObj ptrs: Treaty classArray slots: sels size).
	1 to: sels size do: [:i |
		specSels at: i put: (self symbolFor: (sels at: i))].

	lowSem := self semaphoreObj.
	timerSem := self semaphoreObj.

	"the poised process: one frame at the base, at startUp's first insn"
	stackObj := self addObj: (StObj ptrs: Treaty classStack slots: 512).
	processObj := self addObj: (StObj fixed: Treaty classProcess
		slots: Treaty processNumVmSlots).
	stackObj at: Treaty stackOwner + 1 put: processObj.
	stackObj at: Treaty stackFramesBase + Treaty frameCaller + 1 put: 0.
	stackObj at: Treaty stackFramesBase + Treaty frameRetinfo + 1 put: 0.
	stackObj at: Treaty stackFramesBase + Treaty frameMethod + 1 put: startUpObj.
	stackObj at: Treaty stackFramesBase + Treaty frameFlags + 1
		put: (1 bitShift: Treaty serialShift).
	stackObj at: Treaty stackFramesBase + Treaty frameReceiver + 1 put: smalltalkObj.
	1 to: 512 do: [:i |
		(stackObj at: i) == #nilSlot ifTrue: [stackObj at: i put: nilObj]].
	processObj at: Treaty processStack + 1 put: stackObj.
	processObj at: Treaty processFrameOffset + 1 put: Treaty stackFramesBase.
	processObj at: Treaty processPc + 1 put: 0.
	processObj at: Treaty processPriority + 1 put: 4.
	processObj at: Treaty processSerialCounter + 1 put: 1.

	"symbol table: a plain Array of all symbols interned so far (the
	 kernel's asSymbol scans it; NOTE: symbols interned after this point
	 would be missed — build order places this last)"
	symtab := self addObj: (StObj ptrs: Treaty classArray slots: symbolObjs size).
	1 to: symbolObjs size do: [:i | symtab at: i put: (symbolObjs at: i)].
	"the kernel's Symbol>>intern: works over this global"
	(self globalAssocFor: 'SymbolTable') at: 2 put: symtab.

	specialsObj := self addObj: (StObj ptrs: Treaty classArray
		slots: Treaty specialObjectsCount).
	specialsObj at: Treaty specialNil + 1 put: nilObj.
	specialsObj at: Treaty specialTrue + 1 put: trueObj.
	specialsObj at: Treaty specialFalse + 1 put: falseObj.
	specialsObj at: Treaty specialSmalltalk + 1 put: smalltalkObj.
	specialsObj at: Treaty specialProcessor + 1 put: sched.
	specialsObj at: Treaty specialClassList + 1 put: classList.
	specialsObj at: Treaty specialSymbolTable + 1 put: symtab.
	specialsObj at: Treaty specialSpecializedSelectors + 1 put: specSels.
	specialsObj at: Treaty specialSelDoesNotUnderstand + 1 put: dnuSym.
	specialsObj at: Treaty specialSelMustBeBoolean + 1 put: mbbSym.
	specialsObj at: Treaty specialTerminateTrampoline + 1 put: self terminateTrampolineObj.
	specialsObj at: Treaty specialLowSpaceSemaphore + 1 put: lowSem.
	specialsObj at: Treaty specialTimerSemaphore + 1 put: timerSem!

terminateTrampolineObj
	"§11: the process-termination trampoline — a method whose primitive
	 is terminate with the process itself as receiver (the VM pushes it
	 onto the target's stack)"
	| spec |
	spec := StMethodSpec new.
	spec selector: #terminateTrampoline.
	spec argc: 0.
	spec frameSlots: 2.
	spec primitive: nil.
	spec handlerSlotBase: 0.
	spec mhFlags: 0.
	spec insns: (OrderedCollection new
		add: (Array with: 'PRIM' with: 216);
		add: (Array with: 'RETSELF');
		yourself).
	spec literals: OrderedCollection new.
	spec sites: OrderedCollection new.
	^self materializeMethod: spec forClassDesc: nil!

semaphoreObj
	| s |
	s := self addObj: (StObj fixed: Treaty classSemaphore
		slots: Treaty semaphoreNumVmSlots).
	s at: Treaty semaphoreExcessSignals + 1 put: 0.
	^s! !

!StImageWriter methodsFor: 'emitting'!
layout
	"assign addresses in creation order; the overflow word (for >= 255
	 slots) precedes the header"
	| addr |
	addr := StImageWriter savedBase.
	objects do: [:o |
		o numSlots >= 255
			ifTrue: [o address: addr + 8]
			ifFalse: [o address: addr].
		addr := addr + (o footprintWords * 8)]!

emitWord: w into: out
	"64-bit little-endian via two 32-bit halves (SmallInteger-safe)"
	| lo hi |
	lo := w bitAnd: 16rFFFFFFFF.
	hi := (w bitShift: -32) bitAnd: 16rFFFFFFFF.
	out add: (lo bitAnd: 255).
	out add: ((lo bitShift: -8) bitAnd: 255).
	out add: ((lo bitShift: -16) bitAnd: 255).
	out add: ((lo bitShift: -24) bitAnd: 255).
	out add: (hi bitAnd: 255).
	out add: ((hi bitShift: -8) bitAnd: 255).
	out add: ((hi bitShift: -16) bitAnd: 255).
	out add: ((hi bitShift: -24) bitAnd: 255)!

emitHeaderHalvesClass: classIndex nslotsField: nf format: fmtCode gc: gcBits into: out
	"header = class<<42 | hash(0)<<20 | nf<<12 | fmt<<8 | gc — assembled
	 as two 32-bit halves so the class shift never overflows SmallInteger"
	| lo hi |
	lo := (nf bitShift: Treaty hdrNslotsShift)
		+ (fmtCode bitShift: Treaty hdrFormatShift)
		+ gcBits.
	hi := classIndex bitShift: Treaty hdrClassShift - 32.
	out add: (lo bitAnd: 255).
	out add: ((lo bitShift: -8) bitAnd: 255).
	out add: ((lo bitShift: -16) bitAnd: 255).
	out add: ((lo bitShift: -24) bitAnd: 255).
	out add: (hi bitAnd: 255).
	out add: ((hi bitShift: -8) bitAnd: 255).
	out add: ((hi bitShift: -16) bitAnd: 255).
	out add: ((hi bitShift: -24) bitAnd: 255)!

slotWord: v
	"a slot value: StObj → address; Integer → tagged SmallInt; #nilSlot →
	 nil's address. Negative SmallInts emit via masked halves."
	v == #nilSlot ifTrue: [^nilObj address].
	(v isKindOf: StObj) ifTrue: [^v address].
	(v isKindOf: Integer) ifTrue: [^v * 2 + 1].
	self error: 'bad slot value ' , v printString!

emitObj: o into: out
	| fmtCode nf |
	o format = #bytes
		ifTrue: [
			| pad |
			pad := o numSlots * 8 - o byteSize.
			fmtCode := Treaty fmtBytesBase + pad]
		ifFalse: [
			fmtCode := o format = #ptrs
				ifTrue: [Treaty fmtPtrs] ifFalse: [Treaty fmtFixed]].
	o numSlots >= 255
		ifTrue: [
			self emitWord: o numSlots into: out.
			nf := 255]
		ifFalse: [nf := o numSlots].
	self emitHeaderHalvesClass: o classIndex nslotsField: nf format: fmtCode
		gc: (o isImmutable ifTrue: [Treaty gcBitImmutable] ifFalse: [0]) into: out.
	o format = #bytes
		ifTrue: [
			o bytesContents do: [:b | out add: b].
			(o numSlots * 8 - o byteSize) timesRepeat: [out add: 0]]
		ifFalse: [
			o slots do: [:v |
				self emitWord: (self slotWord: v) into: out]]!

imageBytes
	"the complete image file as an OrderedCollection of byte values"
	| out oldSize |
	self layout.
	out := OrderedCollection new.
	oldSize := 0.
	objects do: [:o | oldSize := oldSize + (o footprintWords * 8)].
	"header (A.5)"
	out add: 83; add: 84; add: 73; add: 77.	"STIM"
	out add: 1; add: 0; add: 0; add: 0.	"version 1"
	self emitWord: 0 into: out.	"flags"
	self emitWord: StImageWriter savedBase into: out.
	self emitWord: oldSize into: out.
	self emitWord: specialsObj address - StImageWriter savedBase into: out.
	self emitWord: (specialsObj at: Treaty specialClassList + 1) address
		- StImageWriter savedBase into: out.
	self emitWord: processObj address - StImageWriter savedBase into: out.
	self emitWord: 0 into: out.	"reserved"
	objects do: [:o | self emitObj: o into: out].
	^out!

writeTo: aPath
	Platform writeBytes: self imageBytes toFile: aPath! !
