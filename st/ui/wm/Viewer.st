"Window-manager L3 (UI.md §7): a Viewer — a titled, non-overlapping region
 inside a Track. The WM sets `bounds` during tiling; the viewer draws its own
 border, a one-line menu strip carrying its name (Oberon's menu-frame), and its
 content lines. `height` is the viewer's allocated share of its track, the unit
 the tiling redistributes on open/close/resize."

Object subclass: #Viewer
	instanceVariableNames: 'name bounds height lines'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-WindowManager'!

!Viewer class methodsFor: 'creation'!
named: aString
	| v |
	v := self new.
	v setName: aString.
	^v! !

!Viewer methodsFor: 'initializing'!
setName: aString
	name := aString.
	height := 0.
	lines := OrderedCollection new! !

!Viewer methodsFor: 'accessing'!
name
	^name!
name: aString
	name := aString!
bounds
	^bounds!
bounds: aRectangle
	bounds := aRectangle!
height
	^height!
height: n
	height := n!
lines
	^lines!
lines: aCollection
	lines := aCollection!
addLine: aString
	lines add: aString! !

!Viewer methodsFor: 'geometry'!
menuBottom
	"the y just below the menu strip (font height + 1px padding + rule)."
	^bounds top + StrikeFont default height + 2!
contentTop
	^self menuBottom + 1!

!Viewer methodsFor: 'drawing'!
drawOn: aCanvas
	| fh y |
	fh := StrikeFont default height.
	aCanvas frameRectangle: bounds value: 1.
	aCanvas drawString: name at: (Point x: bounds left + 2 y: bounds top + 1).
	aCanvas line: (Point x: bounds left y: self menuBottom)
		to: (Point x: bounds right - 1 y: self menuBottom) value: 1.
	y := self contentTop + 1.
	lines do: [:each |
		aCanvas drawString: each at: (Point x: bounds left + 2 y: y).
		y := y + fh + 1].
	^self! !
