"Widget toolkit L4 (UI.md §8): the base View — a rectangle inside a viewer's
 content frame that paints itself onto a Canvas and routes events. A deliberately
 small MVC-lite kit: controllers are light strategy objects (Controller), since
 the event loop is centralized in the UISupervisor (§6). Composable: a browser is
 a PanedView of sub-views."

Object subclass: #View
	instanceVariableNames: 'bounds model controller children'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Widgets'!

!View class methodsFor: 'creation'!
bounds: aRectangle
	| v |
	v := self new.
	v setBounds: aRectangle.
	^v! !

!View methodsFor: 'initializing'!
setBounds: aRectangle
	bounds := aRectangle.
	children := OrderedCollection new! !

!View methodsFor: 'accessing'!
bounds
	^bounds!
bounds: aRectangle
	bounds := aRectangle!
model
	^model!
model: anObject
	model := anObject!
controller
	^controller!
controller: aController
	controller := aController.
	aController notNil ifTrue: [aController view: self]!
children
	^children!
addSubview: aView
	children add: aView.
	^aView! !

!View methodsFor: 'geometry'!
containsPoint: aPoint
	^bounds containsPoint: aPoint!
viewAt: aPoint
	"deepest descendant containing aPoint, or self, or nil if outside."
	(self containsPoint: aPoint) ifFalse: [^nil].
	children reverseDo: [:child |
		(child containsPoint: aPoint) ifTrue: [^child viewAt: aPoint]].
	^self! !

!View methodsFor: 'drawing'!
displayOn: aCanvas
	"paint self, then children. Subclasses override to draw their content;
	 the default just composes children."
	aCanvas clipTo: bounds.
	children do: [:child | child displayOn: aCanvas].
	^self!
invalidate
	"mark this view as needing repaint (damage tracking arrives with the
	 real redraw loop; for now a repaint is a full display)."
	^self! !

!View methodsFor: 'events'!
handleEvent: anEvent
	"route to the controller strategy if present; answer whether handled."
	controller isNil ifTrue: [^false].
	^controller handleEvent: anEvent!
wantsFocus
	controller isNil ifTrue: [^false].
	^controller wantsFocus! !
