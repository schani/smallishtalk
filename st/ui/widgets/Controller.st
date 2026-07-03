"Widget toolkit L4 (UI.md §8): a light Controller strategy. The real event loop
 lives in the UISupervisor (§6), so a Controller is just an event-handling
 strategy bound to one View — not the full Smalltalk-80 control loop."

Object subclass: #Controller
	instanceVariableNames: 'view'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Widgets'!

!Controller methodsFor: 'accessing'!
view
	^view!
view: aView
	view := aView! !

!Controller methodsFor: 'events'!
handleEvent: anEvent
	"answer true if consumed; the base strategy consumes nothing."
	^false!
wantsFocus
	^false! !
