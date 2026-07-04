"Reflection & source retention (UI.md §9): the Browser's data model. Enumerate
 classes and selectors, read retained method source, and hold class/method
 metadata in SystemOrganization SIDE DICTIONARIES (not new class-object slots,
 §9.2 — that would be invasive bootstrap work). Filed into UI images. The class
 list is reached through the SystemClassList global the image writer binds."

Object subclass: #SystemOrganization
	instanceVariableNames: 'categories comments protocols'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Reflection'!

!MethodDictionary methodsFor: 'reflection'!
keys
	^keys!
values
	^values!
indexOfKey: aSelector
	| i |
	i := 1.
	[i <= keys size] whileTrue: [
		(keys at: i) == aSelector ifTrue: [^i].
		i := i + 1].
	^0!
includesKey: aSelector
	^(self indexOfKey: aSelector) > 0!
at: aSelector ifAbsent: aBlock
	| i |
	i := self indexOfKey: aSelector.
	i = 0 ifTrue: [^aBlock value].
	^values at: i! !

!CompiledMethod methodsFor: 'reflection'!
sourceInfo
	"the retained source slot (METHOD_SOURCE_INFO, 0-based 6 => instVarAt: 7)."
	^self instVarAt: 7!
sourceString
	| info |
	info := self sourceInfo.
	info isNil ifTrue: [^nil].
	(info isKindOf: Array) ifTrue: [^info at: 1].
	^info!
methodSelector
	^self instVarAt: 5! !

!Behavior methodsFor: 'reflection'!
classIndex
	^classIndex!
methodDictionary
	^methodDictionary!
selectors
	^methodDictionary keys!
includesSelector: aSelector
	^methodDictionary includesKey: aSelector!
compiledMethodAt: aSelector
	^methodDictionary at: aSelector ifAbsent: [nil]!
sourceCodeAt: aSelector
	| m |
	m := self compiledMethodAt: aSelector.
	m isNil ifTrue: [^nil].
	^m sourceString!
subclasses
	| out |
	out := OrderedCollection new.
	Smalltalk allClassesDo: [:c | c superclass == self ifTrue: [out add: c]].
	^out!
allSubclasses
	| out |
	out := OrderedCollection new.
	self subclasses do: [:c | out add: c. out addAll: c allSubclasses].
	^out!
category
	^Smalltalk organization categoryOf: self!
comment
	^Smalltalk organization commentOf: self! !

!Behavior methodsFor: 'live compilation'!
installSelector: aSelector method: aMethod
	"install a compiled method for aSelector on self (PRIM_METHOD_INSTALL,
	 which flushes the global and inline caches — the live-accept contract,
	 UI.md §9.3/§14.1)."
	<primitive: 402>
	^self primitiveFailed: #installSelector:method:!
compile: sourceString classified: aProtocol
	"the browser's 'accept': recompile sourceString into a live method, install
	 it (caches flush), retain its source, and file it under aProtocol. Answers
	 the selector."
	| method selector |
	method := RuntimeCompiler compile: sourceString in: self.
	selector := method methodSelector.
	self installSelector: selector method: method.
	method instVarAt: 7 put: sourceString.
	Smalltalk organization classify: selector under: aProtocol for: self.
	^selector! !

!SystemDictionary methodsFor: 'classes'!
allClassesDo: aBlock
	"walk the SystemClassList global (the image writer's class-list Array),
	 skipping nil gaps and metaclasses."
	SystemClassList isNil ifTrue: [^self].
	SystemClassList do: [:each |
		(each notNil and: [each class ~~ Metaclass]) ifTrue: [aBlock value: each]]!
allClasses
	| out |
	out := OrderedCollection new.
	self allClassesDo: [:c | out add: c].
	^out!
classNamed: aString
	self allClassesDo: [:c | c name asString = aString ifTrue: [^c]].
	^nil!
classNamesDo: aBlock
	self allClassesDo: [:c | aBlock value: c name asString]!
organization
	SystemOrganizationInstance isNil
		ifTrue: [SystemOrganizationInstance := SystemOrganization new].
	^SystemOrganizationInstance!
evaluate: sourceString
	"do-it: compile sourceString as a 0-arg method and run it with self=nil
	 (the Workspace / browser do-it mechanism, UI.md §10.2). Answers the value."
	| method |
	method := RuntimeCompiler compileDoIt: '^(' , sourceString , ')' in: UndefinedObject.
	UndefinedObject installSelector: #DoIt method: method.
	^nil perform: #DoIt withArguments: (Array new: 0)! !

!SystemOrganization methodsFor: 'categories'!
categoriesDict
	categories isNil ifTrue: [categories := Dictionary new].
	^categories!
protocolsDict
	protocols isNil ifTrue: [protocols := Dictionary new].
	^protocols!
commentsDict
	comments isNil ifTrue: [comments := Dictionary new].
	^comments!
categoryOf: aClass
	^self categoriesDict at: aClass ifAbsent: [self defaultCategoryOf: aClass]!
defaultCategoryOf: aClass
	"the class's source-declared category, from the SystemClassCategories
	 table the image writer binds (parallel to SystemClassList)."
	| idx cat |
	SystemClassCategories isNil ifTrue: [^'Kernel'].
	idx := aClass classIndex.
	(idx notNil and: [idx + 1 <= SystemClassCategories size]) ifTrue: [
		cat := SystemClassCategories at: idx + 1.
		cat notNil ifTrue: [^cat]].
	^'Kernel'!
classify: aClass under: aCategory
	self categoriesDict at: aClass put: aCategory!
categories
	"the category names in use, alphabetically."
	| out cat |
	out := OrderedCollection new.
	Smalltalk allClassesDo: [:c |
		cat := self categoryOf: c.
		(out includes: cat) ifFalse: [out add: cat]].
	out sort: [:a :b | a < b].
	^out!
classesIn: aCategory
	| out |
	out := OrderedCollection new.
	Smalltalk allClassesDo: [:c |
		(self categoryOf: c) = aCategory ifTrue: [out add: c]].
	out sort: [:a :b | a name asString < b name asString].
	^out! !

!SystemOrganization methodsFor: 'comments'!
commentOf: aClass
	^self commentsDict at: aClass ifAbsent: ['']!
comment: aString for: aClass
	self commentsDict at: aClass put: aString! !

!SystemOrganization methodsFor: 'protocols'!
protocolKeyFor: aClass selector: aSelector
	^Array with: aClass with: aSelector!
protocolOf: aClass selector: aSelector
	^self protocolsDict at: (self protocolKeyFor: aClass selector: aSelector)
		ifAbsent: [#unclassified]!
classify: aSelector under: aProtocol for: aClass
	self protocolsDict at: (self protocolKeyFor: aClass selector: aSelector)
		put: aProtocol!
protocolsFor: aClass
	"the distinct protocols of aClass's selectors, alphabetically."
	| out p |
	out := OrderedCollection new.
	aClass selectors do: [:sel |
		p := self protocolOf: aClass selector: sel.
		(out includes: p) ifFalse: [out add: p]].
	out sort: [:a :b | a asString < b asString].
	^out!
selectorsIn: aProtocol of: aClass
	| out |
	out := OrderedCollection new.
	aClass selectors do: [:sel |
		(self protocolOf: aClass selector: sel) = aProtocol ifTrue: [out add: sel]].
	out sort: [:a :b | a asString < b asString].
	^out! !
