"L5 — the Workspace (UI.md §10.3, M6): an editable TextPane with do-it/print-it.
 Falls out of the toolkit almost for free and makes the system usable — you type
 an expression and evaluate it live via the runtime compiler (§9.3)."

View subclass: #Workspace
	instanceVariableNames: 'textPane lastResult'
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
	^lastResult! !

!Workspace methodsFor: 'commands'!
doIt
	"evaluate the whole buffer (or, later, the selection) as an expression."
	lastResult := Smalltalk evaluate: textPane contents.
	^lastResult!
printIt
	"do-it, then append the result's printString after the expression."
	self doIt.
	textPane contents: textPane contents , ' ' , lastResult printString.
	^lastResult! !

!Workspace methodsFor: 'drawing'!
displayOn: aCanvas
	textPane displayOn: aCanvas.
	^self!

!Workspace methodsFor: 'events'!
handleEvent: anEvent
	^textPane handleEvent: anEvent! !
