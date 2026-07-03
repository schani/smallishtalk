"Window-manager L3 (UI.md §7): a Track — a vertical column of the display,
 holding non-overlapping Viewers stacked top-to-bottom that always tile to fill
 the track height. Invariant: the viewers' heights always sum to the track
 height; open/close/resize only redistribute, so laying out is a running sum."

Object subclass: #Track
	instanceVariableNames: 'left width height viewers'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-WindowManager'!

!Track class methodsFor: 'creation'!
left: l width: w height: h
	| t |
	t := self new.
	t setLeft: l width: w height: h.
	^t! !

!Track methodsFor: 'initializing'!
setLeft: l width: w height: h
	left := l.
	width := w.
	height := h.
	viewers := OrderedCollection new! !

!Track methodsFor: 'accessing'!
viewers
	^viewers!
left
	^left!
width
	^width!
height
	^height! !

!Track methodsFor: 'managing viewers'!
open: aViewer
	"Oberon convention: split the focused (last) viewer, giving the newcomer
	 the lower half; an empty track gives it the whole height."
	| focused newH |
	viewers isEmpty
		ifTrue: [
			aViewer height: height.
			viewers add: aViewer]
		ifFalse: [
			focused := viewers last.
			newH := focused height // 2.
			focused height: focused height - newH.
			aViewer height: newH.
			viewers add: aViewer].
	self retile.
	^aViewer!
close: aViewer
	"the freed height is absorbed by the neighbor above (or below if first)."
	| i kept |
	i := viewers indexOf: aViewer.
	i = 0 ifTrue: [^self].
	viewers size = 1
		ifFalse: [
			i > 1
				ifTrue: [(viewers at: i - 1) height: (viewers at: i - 1) height + aViewer height]
				ifFalse: [(viewers at: i + 1) height: (viewers at: i + 1) height + aViewer height]].
	kept := OrderedCollection new.
	viewers do: [:v | v == aViewer ifFalse: [kept add: v]].
	viewers := kept.
	self retile.
	^self!
resize: aViewer by: delta
	"move the split between aViewer and its upper neighbor by delta pixels:
	 the neighbor grows, aViewer shrinks (no-op for the topmost viewer)."
	| i |
	i := viewers indexOf: aViewer.
	(i <= 1) ifTrue: [^self].
	(aViewer height - delta) < 1 ifTrue: [^self].
	((viewers at: i - 1) height + delta) < 1 ifTrue: [^self].
	(viewers at: i - 1) height: (viewers at: i - 1) height + delta.
	aViewer height: aViewer height - delta.
	self retile.
	^self! !

!Track methodsFor: 'layout'!
retile
	"assign each viewer its bounds from the running height sum."
	| y |
	y := 0.
	viewers do: [:v |
		v bounds: (Rectangle origin: (Point x: left y: y)
			corner: (Point x: left + width y: y + v height)).
		y := y + v height].
	^self!
drawOn: aCanvas
	viewers do: [:v | v drawOn: aCanvas].
	^self! !
