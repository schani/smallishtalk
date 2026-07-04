"Widget toolkit L4 (UI.md §8): a scrollable list of strings with single
 selection. Selection highlights via reverse-video (a black bar with XOR glyphs);
 click selects the row under the pointer; selectNext/selectPrevious drive
 keyboard navigation. `onSelect` is a one-arg block fired on selection change."

View subclass: #ListPane
	instanceVariableNames: 'items selection topIndex onSelect scrollBar dragOffset'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Widgets'!

!ListPane methodsFor: 'initializing'!
setBounds: aRectangle
	super setBounds: aRectangle.
	items := OrderedCollection new.
	selection := 0.
	topIndex := 1! !

!ListPane methodsFor: 'accessing'!
items
	^items!
items: aCollection
	items := aCollection asOrderedCollection.
	selection := 0.
	topIndex := 1!
selectionIndex
	^selection!
selectedItem
	^selection = 0 ifTrue: [nil] ifFalse: [items at: selection]!
onSelect: aBlock
	onSelect := aBlock!
lineHeight
	^StrikeFont default height + 1!
topInset
	"blank rows between the pane's top edge and the first row of text; without
	 this the first line's cap height sits flush on the frame line above it
	 and reads as cut off."
	^2!
visibleRows
	^(bounds height - self topInset) // self lineHeight!
topIndex
	^topIndex! !

!ListPane methodsFor: 'scrolling'!
scrollBarWidth
	^6!
needsScrollBar
	^items size > self visibleRows!
contentRight
	"rows stop short of the scroll bar strip when one is shown."
	^self needsScrollBar
		ifTrue: [bounds right - self scrollBarWidth]
		ifFalse: [bounds right]!
scrollBarRect
	^Rectangle left: bounds right - self scrollBarWidth top: bounds top
		right: bounds right bottom: bounds bottom!
scrollBar
	"the attached bar, repositioned to the current bounds and synced to the
	 current scroll state (bounds: not setBounds: — the latter resets it)."
	scrollBar isNil ifTrue: [scrollBar := ScrollBar bounds: self scrollBarRect].
	scrollBar bounds: self scrollBarRect.
	scrollBar total: items size visible: (self visibleRows max: 1) position: topIndex - 1.
	^scrollBar!
maxTopIndex
	^(items size - self visibleRows + 1) max: 1!
scrollBarClickAt: aPoint
	"a press on the thumb grabs it where pressed (no jump); a press in the
	 trough jumps there, centering the thumb under the pointer. Either way
	 the thumb is grabbed: mouse moves drag it until mouse-up releases it
	 (handleEvent). Selection is untouched."
	| thumb |
	items isEmpty ifTrue: [^true].
	thumb := self scrollBar thumbRect.
	(aPoint y >= thumb top and: [aPoint y < thumb bottom])
		ifTrue: [dragOffset := aPoint y - thumb top]
		ifFalse: [
			dragOffset := thumb height // 2.
			self dragThumbTo: aPoint y].
	^true!
dragThumbTo: y
	"scroll so the grabbed point of the thumb sits at y."
	| t |
	t := (y - dragOffset - bounds top) * items size // (bounds height max: 1) + 1.
	topIndex := (t min: self maxTopIndex) max: 1!
isDraggingThumb
	^dragOffset notNil! !

!ListPane methodsFor: 'selecting'!
select: anIndex
	"set the selection (0 = none) and fire onSelect."
	(anIndex >= 0 and: [anIndex <= items size]) ifFalse: [^self].
	selection := anIndex.
	self scrollToSelection.
	onSelect notNil ifTrue: [onSelect value: anIndex].
	^self!
selectNext
	selection < items size ifTrue: [self select: selection + 1]!
selectPrevious
	selection > 1 ifTrue: [self select: selection - 1]!
selectAtPoint: aPoint
	"select the row under aPoint; answer true if it hit an item. A click in
	 the scroll-bar strip scrolls instead of selecting."
	| row |
	(self containsPoint: aPoint) ifFalse: [^false].
	(self needsScrollBar and: [aPoint x >= self contentRight])
		ifTrue: [^self scrollBarClickAt: aPoint].
	row := topIndex + (((aPoint y - bounds top - self topInset) max: 0) // self lineHeight).
	(row >= 1 and: [row <= items size]) ifTrue: [self select: row. ^true].
	^false!
scrollToSelection
	"keep the selection visible."
	selection < topIndex ifTrue: [topIndex := selection max: 1].
	selection >= (topIndex + self visibleRows)
		ifTrue: [topIndex := selection - self visibleRows + 1]! !

!ListPane methodsFor: 'drawing'!
displayOn: aCanvas
	| fh y i font rowRect |
	aCanvas clipTo: bounds.
	font := StrikeFont default.
	fh := font height.
	y := bounds top + self topInset.
	i := topIndex.
	[i <= items size and: [y + fh <= bounds bottom]] whileTrue: [
		i = selection
			ifTrue: [
				rowRect := Rectangle origin: (Point x: bounds left y: y)
					corner: (Point x: self contentRight y: y + fh).
				aCanvas fillRectangle: rowRect value: 1.
				font displayString: (items at: i) on: aCanvas form
					at: (Point x: bounds left + 1 y: y) rule: 6]
			ifFalse: [
				aCanvas drawString: (items at: i) at: (Point x: bounds left + 1 y: y)].
		y := y + self lineHeight.
		i := i + 1].
	self needsScrollBar ifTrue: [
		aCanvas fillRectangle: self scrollBarRect value: 0.
		self scrollBar displayOn: aCanvas].
	^self! !

!ListPane methodsFor: 'events'!
handleEvent: anEvent
	"rows select on mouse-down; the scroll-bar strip arms a thumb drag that
	 tracks mouse moves (even outside the pane — the browser captures the
	 mouse for us) until mouse-up releases it."
	anEvent isMouseDown ifTrue: [^self selectAtPoint: anEvent position].
	(anEvent isMouseMove and: [dragOffset notNil])
		ifTrue: [self dragThumbTo: anEvent position y. ^true].
	anEvent isMouseUp ifTrue: [dragOffset := nil].
	^false! !
