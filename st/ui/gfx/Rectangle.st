"UI-Graphics L2 (UI.md §5.1): a rectangle as origin (top-left, inclusive) and
 corner (bottom-right, exclusive) Points. Half-open, like the BitBlt clip."

Object subclass: #Rectangle
	instanceVariableNames: 'origin corner'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Graphics'!

!Rectangle class methodsFor: 'creation'!
origin: o corner: c
	| r |
	r := self new.
	r setOrigin: o corner: c.
	^r!
left: l top: t right: rt bottom: b
	^self origin: (Point x: l y: t) corner: (Point x: rt y: b)! !

!Rectangle methodsFor: 'accessing'!
setOrigin: o corner: c
	origin := o.
	corner := c!
origin
	^origin!
corner
	^corner!
left
	^origin x!
top
	^origin y!
right
	^corner x!
bottom
	^corner y!
width
	^corner x - origin x!
height
	^corner y - origin y!
extent
	^Point x: self width y: self height!
area
	^self width * self height! !

!Rectangle methodsFor: 'testing'!
isEmpty
	^self width <= 0 or: [self height <= 0]!
containsPoint: aPoint
	^aPoint x >= origin x
		and: [aPoint y >= origin y
		and: [aPoint x < corner x
		and: [aPoint y < corner y]]]! !

!Rectangle methodsFor: 'combining'!
intersect: other
	^Rectangle origin: (origin max: other origin) corner: (corner min: other corner)!
merge: other
	^Rectangle origin: (origin min: other origin) corner: (corner max: other corner)! !

!Rectangle methodsFor: 'printing'!
printString
	^origin printString , ' corner: ' , corner printString! !
