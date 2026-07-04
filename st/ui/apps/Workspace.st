"L5 — the Workspace (UI.md §10.3, M6): an editable TextPane with do-it/print-it.
 Type and edit text, drag-select an expression, and right-click the selection
 for the operate menu ('do it' / 'print it'), which evaluates it live via the
 runtime compiler (§9.3). With no selection, the commands act on the whole
 buffer."

View subclass: #Workspace
	instanceVariableNames: 'textPane lastResult menu'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Apps'!

!Workspace methodsFor: 'initializing'!
setBounds: aRectangle
	super setBounds: aRectangle.
	textPane := TextPane bounds: aRectangle.
	self addSubview: textPane! !

!Workspace methodsFor: 'accessing'!
textPane
	^textPane!
contents
	^textPane contents!
contents: aString
	textPane contents: aString!
lastResult
	^lastResult!
menu
	^menu!
menuVisible
	^menu notNil! !

!Workspace methodsFor: 'commands'!
currentExpression
	"what do-it acts on: the selection when there is one, else the buffer."
	^textPane hasSelection
		ifTrue: [textPane selectedText]
		ifFalse: [textPane contents]!
doIt
	lastResult := Smalltalk evaluate: self currentExpression.
	^lastResult!
printIt
	"do-it, then insert the result's printString right after the evaluated
	 text and leave it selected — the classic gesture: the next keystroke
	 (or the next print-it) replaces the old result."
	| printed pos |
	self doIt.
	printed := ' ' , lastResult printString.
	pos := textPane hasSelection
		ifTrue: [textPane selectionStop]
		ifFalse: [textPane contents size].
	textPane caretAt: pos.
	textPane insertString: printed.
	textPane selectFrom: pos to: pos + printed size.
	^lastResult! !

!Workspace methodsFor: 'menu'!
rightClickAt: aPoint
	"pop up the operate menu when the press lands ON the selected text."
	| off |
	textPane hasSelection ifFalse: [^false].
	off := textPane offsetForPoint: aPoint.
	(off >= textPane selectionStart and: [off <= textPane selectionStop])
		ifFalse: [^false].
	self openMenuAt: aPoint.
	^true!
openMenuAt: aPoint
	"a two-row MenuPane at the click, clamped inside the workspace."
	| font w h left top |
	font := StrikeFont default.
	w := (font widthOfString: 'print it') + 5.
	h := (2 * (font height + 1)) + 2.
	left := aPoint x min: bounds right - w.
	top := aPoint y min: bounds bottom - h.
	menu := MenuPane bounds: (Rectangle origin: (Point x: left y: top)
		corner: (Point x: left + w y: top + h)).
	menu commands: (Array with: 'do it' with: 'print it')!
menuEvent: anEvent
	"while the menu is up it owns the input: any press picks the row under
	 the pointer (or, outside, nothing) and dismisses; the rest is ignored."
	| cmd |
	anEvent isMouseDown ifFalse: [^false].
	cmd := menu commandAtPoint: anEvent position.
	menu := nil.
	cmd = 'do it' ifTrue: [self doIt].
	cmd = 'print it' ifTrue: [self printIt].
	^true! !

!Workspace methodsFor: 'drawing'!
displayOn: aCanvas
	textPane displayOn: aCanvas.
	menu notNil ifTrue: [menu displayOn: aCanvas].
	^self! !

!Workspace methodsFor: 'events'!
handleEvent: anEvent
	menu notNil ifTrue: [^self menuEvent: anEvent].
	anEvent isRightMouseDown ifTrue: [^self rightClickAt: anEvent position].
	^textPane handleEvent: anEvent! !
