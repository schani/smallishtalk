"M4 — source retention + the reflection API (UI.md §9), tested on the VM
 with the in-image harness. Enumerate classes, read retained method source,
 walk subclasses, query the SystemOrganization, and compile live."

TestCase subclass: #ReflectionTests
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Testing'!

!ReflectionTests methodsFor: 'tests'!
testEnumerateClassesAndSelectors
	| arr obj |
	self assert: Smalltalk allClasses size > 30.
	arr := Smalltalk classNamed: 'Array'.
	self assert: arr name asString equals: 'Array'.
	self assert: (arr includesSelector: #printString).
	self deny: (arr includesSelector: #fooBarBaz).
	obj := Smalltalk classNamed: 'Object'.
	self assert: (obj includesSelector: #yourself).
	self assert: (Smalltalk classNamed: 'Boolean') subclasses size equals: 2.
	self assert: obj category equals: 'Kernel'!
testRetainedMethodSourceIsReadable
	"the reserved source slot is no longer nilled: the exact chunk source of
	 a kernel method round-trips into the image and is readable by
	 reflection. Object>>yourself's retained chunk source is locked verbatim
	 (the leading newline is the chunk boundary)."
	| src |
	src := (Smalltalk classNamed: 'Object') sourceCodeAt: #yourself.
	self deny: src isNil.
	self assert: src equals: '
yourself
	^self'!
testOrganizationClassifiesAndQueries
	"uses OrderedCollection, NOT Array: the suite shares one image, and
	 BrowserTests navigate Array's protocols — reclassifying Array's only
	 own selector (printString) here would move it out of the 'printing'
	 protocol they select."
	| org oc |
	org := Smalltalk organization.
	oc := Smalltalk classNamed: 'OrderedCollection'.
	org classify: oc under: 'Collections-Sequenceable'.
	self assert: (org categoryOf: oc) equals: 'Collections-Sequenceable'.
	self assert: (org classesIn: 'Collections-Sequenceable') size equals: 1.
	"restore the default so the shared image's 'Collections' category (used
	 by BrowserTests) is intact whatever the suite order"
	org classify: oc under: 'Collections'.
	org classify: #grow under: #private for: oc.
	self assert: (org protocolOf: oc selector: #grow) equals: #private.
	self assert: (org protocolOf: oc selector: #size) equals: #accessing!
testSourceDeclaredMethodProtocols
	"method protocols default to the methodsFor: group declared in the .st
	 sources: the image writer records the protocol (with the source) in
	 each method's source-info slot, and the organization falls back to it.
	 Methods without one (e.g. startUp) stay unclassified; protocol and
	 selector lists come back alphabetically sorted."
	| org arr protos prev |
	org := Smalltalk organization.
	arr := Smalltalk classNamed: 'Array'.
	self assert: (org protocolOf: arr selector: #printString) equals: #printing.
	self assert: ((arr compiledMethodAt: #printString) protocol) equals: #printing.
	self assert: (org protocolOf: Object selector: #yourself) equals: #accessing.
	self assert: (org protocolOf: (Smalltalk classNamed: 'OrderedCollection')
		selector: #add:) equals: #accessing.
	self assert: (org protocolOf: (Smalltalk classNamed: 'SystemDictionary')
		selector: #startUp) equals: #unclassified.
	protos := org protocolsFor: Object.
	self assert: (protos includes: #accessing).
	prev := nil.
	protos do: [:p |
		prev notNil ifTrue: [
			self assert: prev asString < p asString description: 'protocols must be sorted'].
		prev := p].
	self assert: ((org selectorsIn: #printing of: arr) includes: #printString)!
testSourceDeclaredCategoriesAndSortedLists
	"class categories default to what the class definitions in the .st
	 sources declare, via the SystemClassCategories table the image writer
	 binds; category/class lists come back alphabetically sorted."
	| org cats classes prev |
	org := Smalltalk organization.
	self assert: (org categoryOf: Object) equals: 'Kernel'.
	self assert: (org categoryOf: (Smalltalk classNamed: 'Array')) equals: 'Collections'.
	self assert: (org categoryOf: (Smalltalk classNamed: 'SmallInteger')) equals: 'Magnitudes'.
	self assert: (org categoryOf: (Smalltalk classNamed: 'Error')) equals: 'Exceptions'.
	self assert: (org categoryOf: (Smalltalk classNamed: 'StLexer')) equals: 'Compiler'.
	self assert: (org categoryOf: (Smalltalk classNamed: 'ListPane')) equals: 'UI-Widgets'.
	self assert: (org categoryOf: (Smalltalk classNamed: 'ClassBrowser')) equals: 'UI-Apps'.
	cats := org categories.
	prev := nil.
	cats do: [:c |
		prev notNil ifTrue: [
			self assert: prev < c description: 'categories must be sorted'].
		prev := c].
	classes := org classesIn: 'Magnitudes'.
	self assert: classes size equals: 4.
	self assert: (classes at: 1) name asString equals: 'Integer'.
	self assert: (classes at: 2) name asString equals: 'Magnitude'.
	self assert: (classes at: 3) name asString equals: 'Number'.
	self assert: (classes at: 4) name asString equals: 'SmallInteger'!
testLiveCompileAndEvaluate
	"compile a method live, run it, recompile it, and see the NEW behavior;
	 read back the recompiled source; evaluate do-it expressions."
	Object compile: 'uiTestAnswer ^42' classified: #testing.
	self assert: Object new uiTestAnswer equals: 42.
	Object compile: 'uiTestAnswer ^99' classified: #testing.
	self assert: Object new uiTestAnswer equals: 99.
	self assert: (Object sourceCodeAt: #uiTestAnswer) equals: 'uiTestAnswer ^99'.
	self assert: (Smalltalk evaluate: '3 + 4') equals: 7.
	self assert: (Smalltalk evaluate: '[:a :b | a + b] value: 10 value: 5') equals: 15! !
