"Widget toolkit L4 (UI.md §8): a pop-up list menu (the right-button viewer menu
 and the browser's operate menus). Holds command symbols; `commandAtPoint:`
 resolves a click to the chosen command (or nil outside). Drawn as a framed
 reverse-video list."

View subclass: #MenuPane
	instanceVariableNames: 'commands'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Widgets'!

!MenuPane methodsFor: 'initializing'!
setBounds: aRectangle
	super setBounds: aRectangle.
	commands := OrderedCollection new! !

!MenuPane methodsFor: 'accessing'!
commands
	^commands!
commands: aCollection
	commands := aCollection asOrderedCollection!
lineHeight
	^StrikeFont default height + 1! !

!MenuPane methodsFor: 'querying'!
commandAtPoint: aPoint
	"the command word under aPoint, or nil."
	| row |
	(self containsPoint: aPoint) ifFalse: [^nil].
	row := ((aPoint y - bounds top - 1) // self lineHeight) + 1.
	(row >= 1 and: [row <= commands size]) ifTrue: [^commands at: row].
	^nil! !

!MenuPane methodsFor: 'drawing'!
displayOn: aCanvas
	| y i |
	aCanvas clipTo: bounds.
	aCanvas fillRectangle: bounds value: 0.
	aCanvas frameRectangle: bounds value: 1.
	y := bounds top + 1.
	i := 1.
	[i <= commands size] whileTrue: [
		aCanvas drawString: (commands at: i) asString at: (Point x: bounds left + 2 y: y).
		y := y + self lineHeight.
		i := i + 1].
	^self! !
