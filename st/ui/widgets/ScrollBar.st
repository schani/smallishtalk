"Widget toolkit L4 (UI.md §8): a thin vertical scroll bar that attaches to a
 ListPane/TextPane. Models total/visible/position and draws a proportional thumb;
 `thumbRect` is the draggable thumb the WM hit-tests."

View subclass: #ScrollBar
	instanceVariableNames: 'total visible position'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Widgets'!

!ScrollBar methodsFor: 'initializing'!
setBounds: aRectangle
	super setBounds: aRectangle.
	total := 1.
	visible := 1.
	position := 0! !

!ScrollBar methodsFor: 'accessing'!
total: t visible: v position: p
	total := t max: 1.
	visible := v max: 1.
	position := p max: 0!
position
	^position! !

!ScrollBar methodsFor: 'geometry'!
thumbRect
	"the thumb: height proportional to visible/total, offset by position/total."
	| h top thumbH thumbTop |
	h := bounds height.
	thumbH := (visible * h // total) max: 2.
	thumbTop := bounds top + (position * h // total).
	top := thumbTop min: (bounds bottom - thumbH).
	^Rectangle origin: (Point x: bounds left y: top)
		corner: (Point x: bounds right y: top + thumbH)! !

!ScrollBar methodsFor: 'drawing'!
displayOn: aCanvas
	aCanvas frameRectangle: bounds value: 1.
	aCanvas fillRectangle: self thumbRect value: 1.
	^self! !
