"Widget toolkit L4 (UI.md §8): a view that splits its bounds among N child panes
 along one axis (used for the 5-pane browser: a horizontal row of list panes over
 a text pane). Equal split for now; the browser composes PanedViews to get its
 layout. Hit-testing/redraw come from View."

View subclass: #PanedView
	instanceVariableNames: 'orientation'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Widgets'!

!PanedView methodsFor: 'initializing'!
setBounds: aRectangle
	super setBounds: aRectangle.
	orientation := #horizontal! !

!PanedView methodsFor: 'accessing'!
horizontal
	orientation := #horizontal!
vertical
	orientation := #vertical!
addPane: aView
	^self addSubview: aView! !

!PanedView methodsFor: 'layout'!
layout
	"assign each child an equal share of the split axis; the last absorbs the
	 rounding remainder so the panes exactly fill the bounds."
	| n avail pos sz start stop |
	n := children size.
	n = 0 ifTrue: [^self].
	orientation == #horizontal
		ifTrue: [avail := bounds width. pos := bounds left]
		ifFalse: [avail := bounds height. pos := bounds top].
	1 to: n do: [:i |
		sz := avail // n.
		i = n ifTrue: [
			stop := orientation == #horizontal ifTrue: [bounds right] ifFalse: [bounds bottom].
			sz := stop - pos].
		start := pos.
		(children at: i) bounds: (orientation == #horizontal
			ifTrue: [Rectangle origin: (Point x: start y: bounds top)
				corner: (Point x: start + sz y: bounds bottom)]
			ifFalse: [Rectangle origin: (Point x: bounds left y: start)
				corner: (Point x: bounds right y: start + sz)]).
		pos := pos + sz].
	^self! !

!PanedView methodsFor: 'drawing'!
displayOn: aCanvas
	self layout.
	children do: [:child | child displayOn: aCanvas].
	^self! !
