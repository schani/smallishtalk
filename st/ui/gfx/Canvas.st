"UI-Graphics L2 (UI.md §5.6): the drawing facade a view is handed to paint
 itself. All output is clipped to `clip` and composed of Form pixels, BitBlt
 and the StrikeFont. Views talk to a Canvas, never to primitives directly."

Object subclass: #Canvas
	instanceVariableNames: 'form clip font'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Graphics'!

!Canvas class methodsFor: 'creation'!
on: aForm
	| c |
	c := self new.
	c setForm: aForm.
	^c! !

!Canvas methodsFor: 'initializing'!
setForm: aForm
	form := aForm.
	clip := aForm boundingBox.
	font := StrikeFont default! !

!Canvas methodsFor: 'accessing'!
form
	^form!
clip
	^clip!
clipTo: aRectangle
	clip := aRectangle!
font: aFont
	font := aFont! !

!Canvas methodsFor: 'drawing'!
fillRectangle: aRect value: v
	| r |
	r := aRect intersect: clip.
	r top to: r bottom - 1 do: [:py |
		r left to: r right - 1 do: [:px |
			form pixelAtX: px y: py put: v]].
	^self!
frameRectangle: aRect value: v
	"one-pixel border (half-open rect: right/bottom columns are corner-1)."
	| pen l t rt b |
	l := aRect left. t := aRect top. rt := aRect right - 1. b := aRect bottom - 1.
	pen := Pen on: form.
	pen color: v.
	pen place: (Point x: l y: t).
	pen goto: (Point x: rt y: t).
	pen goto: (Point x: rt y: b).
	pen goto: (Point x: l y: b).
	pen goto: (Point x: l y: t).
	^self!
line: p1 to: p2 value: v
	| pen |
	pen := Pen on: form.
	pen color: v.
	pen place: p1.
	pen goto: p2.
	^self!
drawString: aString at: aPoint
	font displayString: aString on: form at: aPoint.
	^self!
drawString: aString at: aPoint font: aFont
	aFont displayString: aString on: form at: aPoint.
	^self! !
