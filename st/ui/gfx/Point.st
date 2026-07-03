"UI-Graphics L2 (UI.md §5.1): an integer 2-D point. No Float in the kernel,
 so coordinates are SmallIntegers throughout the graphics stack."

Object subclass: #Point
	instanceVariableNames: 'x y'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Graphics'!

!Point class methodsFor: 'creation'!
x: ax y: ay
	| p |
	p := self new.
	p setX: ax y: ay.
	^p! !

!Point methodsFor: 'accessing'!
x
	^x!
y
	^y!
setX: ax y: ay
	x := ax.
	y := ay! !

!Point methodsFor: 'arithmetic'!
+ aPoint
	^Point x: x + aPoint x y: y + aPoint y!
- aPoint
	^Point x: x - aPoint x y: y - aPoint y!
* aScale
	^Point x: x * aScale y: y * aScale! !

!Point methodsFor: 'comparing'!
< aPoint
	^x < aPoint x and: [y < aPoint y]!
> aPoint
	^x > aPoint x and: [y > aPoint y]!
<= aPoint
	^x <= aPoint x and: [y <= aPoint y]!
= aPoint
	^(aPoint isKindOf: Point) and: [x = aPoint x and: [y = aPoint y]]!
hash
	^x hash bitXor: y hash!
max: aPoint
	^Point x: (x max: aPoint x) y: (y max: aPoint y)!
min: aPoint
	^Point x: (x min: aPoint x) y: (y min: aPoint y)! !

!Point methodsFor: 'printing'!
printString
	^x printString , '@' , y printString! !
