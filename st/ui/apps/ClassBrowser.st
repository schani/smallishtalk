"L5 — the Class Browser (UI.md §10): the deliverable. A classic five-pane System
 Browser — category / class / protocol / selector list panes over an editable
 source TextPane — wired to the reflection API (§9). Navigate, view retained
 source, edit and 'accept' to recompile a method LIVE, and do-it/print-it.

 Driving helpers (selectCategoryNamed: etc.) let the headless driver and tests
 operate it by name; the same selections fire when a real click hits a pane."

View subclass: #ClassBrowser
	instanceVariableNames: 'categoryPane classPane protocolPane selectorPane sourcePane classObjects selectorObjects currentCategory currentClass currentProtocol currentSelector lastResult'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Apps'!

!ClassBrowser methodsFor: 'initializing'!
setBounds: aRectangle
	super setBounds: aRectangle.
	classObjects := OrderedCollection new.
	selectorObjects := OrderedCollection new.
	self buildPanes.
	self refreshCategories!
zeroRect
	^Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 1 y: 1)!
buildPanes
	categoryPane := ListPane bounds: self zeroRect.
	classPane := ListPane bounds: self zeroRect.
	protocolPane := ListPane bounds: self zeroRect.
	selectorPane := ListPane bounds: self zeroRect.
	sourcePane := TextPane bounds: self zeroRect.
	categoryPane onSelect: [:i | self categorySelected: i].
	classPane onSelect: [:i | self classSelected: i].
	protocolPane onSelect: [:i | self protocolSelected: i].
	selectorPane onSelect: [:i | self selectorSelected: i].
	self addSubview: categoryPane.
	self addSubview: classPane.
	self addSubview: protocolPane.
	self addSubview: selectorPane.
	self addSubview: sourcePane.
	self layoutPanes!
layoutPanes
	| listBottom colW l |
	listBottom := bounds top + (bounds height * 2 // 5).
	colW := bounds width // 4.
	l := bounds left.
	categoryPane bounds: (Rectangle left: l top: bounds top right: l + colW bottom: listBottom).
	classPane bounds: (Rectangle left: l + colW top: bounds top right: l + (colW * 2) bottom: listBottom).
	protocolPane bounds: (Rectangle left: l + (colW * 2) top: bounds top right: l + (colW * 3) bottom: listBottom).
	selectorPane bounds: (Rectangle left: l + (colW * 3) top: bounds top right: bounds right bottom: listBottom).
	sourcePane bounds: (Rectangle left: l top: listBottom right: bounds right bottom: bounds bottom)! !

!ClassBrowser methodsFor: 'accessing'!
sourcePane
	^sourcePane!
categoryNames
	^categoryPane items!
classNames
	^classPane items!
protocolNames
	^protocolPane items!
selectorNames
	^selectorPane items!
currentClass
	^currentClass!
currentSelector
	^currentSelector!
lastResult
	^lastResult! !

!ClassBrowser methodsFor: 'navigation'!
refreshCategories
	| names |
	names := OrderedCollection new.
	Smalltalk organization categories do: [:c | names add: c].
	categoryPane items: names!
categorySelected: i
	| names |
	currentCategory := categoryPane selectedItem.
	classObjects := Smalltalk organization classesIn: currentCategory.
	names := OrderedCollection new.
	classObjects do: [:c | names add: c name asString].
	classPane items: names.
	protocolPane items: OrderedCollection new.
	selectorPane items: OrderedCollection new.
	selectorObjects := OrderedCollection new.
	sourcePane contents: ''!
classSelected: i
	| names |
	currentClass := classObjects at: i.
	names := OrderedCollection new.
	(Smalltalk organization protocolsFor: currentClass) do: [:p | names add: p asString].
	protocolPane items: names.
	selectorPane items: OrderedCollection new.
	selectorObjects := OrderedCollection new.
	sourcePane contents: self classDefinitionText!
protocolSelected: i
	currentProtocol := protocolPane selectedItem asSymbol.
	self refreshSelectors!
refreshSelectors
	| names |
	selectorObjects := Smalltalk organization selectorsIn: currentProtocol of: currentClass.
	names := OrderedCollection new.
	selectorObjects do: [:s | names add: s asString].
	selectorPane items: names!
selectorSelected: i
	currentSelector := selectorObjects at: i.
	sourcePane contents: (currentClass sourceCodeAt: currentSelector)!
classDefinitionText
	| sup |
	sup := currentClass superclass isNil
		ifTrue: ['nil']
		ifFalse: [currentClass superclass name asString].
	^sup , ' subclass: #' , currentClass name asString! !

!ClassBrowser methodsFor: 'driving'!
indexOfName: aName in: aListPane
	| i |
	i := 1.
	[i <= aListPane items size] whileTrue: [
		(aListPane items at: i) = aName ifTrue: [^i].
		i := i + 1].
	^0!
selectCategoryNamed: aName
	categoryPane select: (self indexOfName: aName in: categoryPane)!
selectClassNamed: aName
	classPane select: (self indexOfName: aName in: classPane)!
selectProtocolNamed: aName
	protocolPane select: (self indexOfName: aName in: protocolPane)!
selectSelectorNamed: aName
	selectorPane select: (self indexOfName: aName in: selectorPane)! !

!ClassBrowser methodsFor: 'commands'!
accept
	"the classic 'accept': recompile the source pane into the current class
	 under the current protocol, live (UI.md §10.2)."
	currentClass isNil ifTrue: [^nil].
	currentSelector := currentClass compile: sourcePane contents
		classified: (currentProtocol isNil ifTrue: [#unclassified] ifFalse: [currentProtocol]).
	self refreshSelectors.
	^currentSelector!
doIt
	"evaluate the source pane's text (or selection) as an expression."
	lastResult := Smalltalk evaluate: sourcePane contents.
	^lastResult!
printIt
	"do-it, then append the result's printString to the source pane."
	self doIt.
	sourcePane contents: sourcePane contents , ' ' , lastResult printString.
	^lastResult! !

!ClassBrowser methodsFor: 'events'!
handleEvent: anEvent
	"route a click to the list/source pane under the pointer (which selects,
	 firing navigation); route keystrokes to the source pane."
	| target |
	anEvent isKeystroke ifTrue: [^sourcePane handleEvent: anEvent].
	(anEvent isMouseButton or: [anEvent isMouseMove]) ifTrue: [
		target := self viewAt: anEvent position.
		(target notNil and: [target ~~ self]) ifTrue: [^target handleEvent: anEvent]].
	^false! !

!ClassBrowser methodsFor: 'drawing'!
displayOn: aCanvas
	aCanvas clipTo: bounds.
	aCanvas fillRectangle: bounds value: 0.
	categoryPane displayOn: aCanvas.
	classPane displayOn: aCanvas.
	protocolPane displayOn: aCanvas.
	selectorPane displayOn: aCanvas.
	sourcePane displayOn: aCanvas.
	self drawFramesOn: aCanvas.
	^self!
drawFramesOn: aCanvas
	| colW listBottom |
	aCanvas clipTo: bounds.
	aCanvas frameRectangle: bounds value: 1.
	listBottom := categoryPane bounds bottom.
	colW := bounds width // 4.
	aCanvas line: (Point x: bounds left y: listBottom) to: (Point x: bounds right - 1 y: listBottom) value: 1.
	1 to: 3 do: [:k |
		aCanvas line: (Point x: bounds left + (colW * k) y: bounds top)
			to: (Point x: bounds left + (colW * k) y: listBottom) value: 1]! !
