"UI-Graphics L2 (UI.md §5.4): a minimal line pen over a Form. Integer
 Bresenham (no Float in the kernel). Enough for viewer borders, separators,
 list-highlight rules and the caret; turtle heading is deferred."

Object subclass: #Pen
	instanceVariableNames: 'form location penColor penDown'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Graphics'!

!Pen class methodsFor: 'creation'!
on: aForm
	| p |
	p := self new.
	p setForm: aForm.
	^p! !

!Pen methodsFor: 'initializing'!
setForm: aForm
	form := aForm.
	location := Point x: 0 y: 0.
	penColor := 1.
	penDown := true! !

!Pen methodsFor: 'accessing'!
color: c
	penColor := c!
up
	penDown := false!
down
	penDown := true!
location
	^location!
place: aPoint
	location := aPoint! !

!Pen methodsFor: 'drawing'!
goto: aPoint
	penDown ifTrue: [self lineFrom: location to: aPoint].
	location := aPoint!
lineFrom: p1 to: p2
	"Bresenham; sets pixels to penColor."
	| x0 y0 x1 y1 dx dy sx sy err e2 |
	x0 := p1 x. y0 := p1 y. x1 := p2 x. y1 := p2 y.
	dx := (x1 - x0) abs.
	dy := (y1 - y0) abs.
	sx := x0 < x1 ifTrue: [1] ifFalse: [-1].
	sy := y0 < y1 ifTrue: [1] ifFalse: [-1].
	err := dx - dy.
	[true] whileTrue: [
		form pixelAtX: x0 y: y0 put: penColor.
		(x0 = x1 and: [y0 = y1]) ifTrue: [^self].
		e2 := 2 * err.
		e2 > (0 - dy) ifTrue: [err := err - dy. x0 := x0 + sx].
		e2 < dx ifTrue: [err := err + dx. y0 := y0 + sy]]! !
