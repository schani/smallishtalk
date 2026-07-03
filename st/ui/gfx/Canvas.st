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

!Canvas class methodsFor: 'solid source'!
solidBlackWidth: w height: h
	"the shared all-ones Form used as the BitBlt source for value-1 fills and
	 lines (rule 3 store); grown on demand and cached in a global, like the
	 strike font."
	| nw nh |
	(SolidBlackForm isNil
		or: [SolidBlackForm width < w or: [SolidBlackForm height < h]])
		ifTrue: [
			nw := SolidBlackForm isNil ifTrue: [w] ifFalse: [w max: SolidBlackForm width].
			nh := SolidBlackForm isNil ifTrue: [h] ifFalse: [h max: SolidBlackForm height].
			SolidBlackForm := Form width: nw height: nh.
			SolidBlackForm fill: 1].
	^SolidBlackForm! !

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
	"clip, then fill through the BitBlt primitive: rule 0 (clear) for value 0,
	 rule 3 (store) from the shared solid-black source otherwise. The original
	 per-pixel loop dominated a profiled repaint (~97% of 115ms)."
	| r w h bb |
	r := aRect intersect: clip.
	w := r right - r left.
	h := r bottom - r top.
	(w <= 0 or: [h <= 0]) ifTrue: [^self].
	bb := BitBlt new.
	bb destForm: form.
	v = 0
		ifTrue: [bb sourceForm: nil. bb combinationRule: 0]
		ifFalse: [
			bb sourceForm: (Canvas solidBlackWidth: w height: h).
			bb combinationRule: 3].
	bb destX: r left.
	bb destY: r top.
	bb width: w.
	bb height: h.
	bb sourceX: 0.
	bb sourceY: 0.
	bb clipX: 0.
	bb clipY: 0.
	bb clipWidth: form width.
	bb clipHeight: form height.
	bb copyBits.
	^self!
frameRectangle: aRect value: v
	"one-pixel border (half-open rect: right/bottom columns are corner-1),
	 four 1px line fills."
	| l t rt b |
	l := aRect left. t := aRect top. rt := aRect right - 1. b := aRect bottom - 1.
	self line: (Point x: l y: t) to: (Point x: rt y: t) value: v.
	self line: (Point x: rt y: t) to: (Point x: rt y: b) value: v.
	self line: (Point x: l y: b) to: (Point x: rt y: b) value: v.
	self line: (Point x: l y: t) to: (Point x: l y: b) value: v.
	^self!
line: p1 to: p2 value: v
	"endpoints inclusive. Axis-aligned lines — every line the UI draws — are
	 1px rectangle fills (BitBlt); Bresenham only runs for diagonals."
	| pen |
	p1 x = p2 x ifTrue: [
		^self fillRectangle: (Rectangle
			origin: (Point x: p1 x y: (p1 y min: p2 y))
			corner: (Point x: p1 x + 1 y: (p1 y max: p2 y) + 1)) value: v].
	p1 y = p2 y ifTrue: [
		^self fillRectangle: (Rectangle
			origin: (Point x: (p1 x min: p2 x) y: p1 y)
			corner: (Point x: (p1 x max: p2 x) + 1 y: p1 y + 1)) value: v].
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
