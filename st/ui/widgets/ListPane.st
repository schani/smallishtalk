"Widget toolkit L4 (UI.md §8): a scrollable list of strings with single
 selection. Selection highlights via reverse-video (a black bar with XOR glyphs);
 click selects the row under the pointer; selectNext/selectPrevious drive
 keyboard navigation. `onSelect` is a one-arg block fired on selection change."

View subclass: #ListPane
	instanceVariableNames: 'items selection topIndex onSelect'
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
	^(bounds height - self topInset) // self lineHeight! !

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
	"select the row under aPoint; answer true if it hit an item."
	| row |
	(self containsPoint: aPoint) ifFalse: [^false].
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
					corner: (Point x: bounds right y: y + fh).
				aCanvas fillRectangle: rowRect value: 1.
				font displayString: (items at: i) on: aCanvas form
					at: (Point x: bounds left + 1 y: y) rule: 6]
			ifFalse: [
				aCanvas drawString: (items at: i) at: (Point x: bounds left + 1 y: y)].
		y := y + self lineHeight.
		i := i + 1].
	^self! !

!ListPane methodsFor: 'events'!
handleEvent: anEvent
	anEvent isMouseDown ifTrue: [^self selectAtPoint: anEvent position].
	^false! !
