"UI-Graphics L2 (UI.md §6.1): a message-y wrapper over the raw event array
 `#(type a b c d)` that primNextEvent (330) returns. Type tags match the host
 encoding in src/host_ui.rs: 1 move, 2 button, 3 keyDown, 4 keyUp, 5 scroll,
 6 resize, 7 close."

Object subclass: #Event
	instanceVariableNames: 'type a b c d'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Graphics'!

!Event class methodsFor: 'reading'!
primNextEvent
	"harvest + pop one host event, or nil when the queue is empty."
	<primitive: 330>
	^nil!
next
	"the next host Event, or nil if none is pending."
	| arr |
	arr := self primNextEvent.
	arr isNil ifTrue: [^nil].
	^self fromArray: arr!
fromArray: arr
	| e |
	e := self new.
	e setType: (arr at: 1) a: (arr at: 2) b: (arr at: 3) c: (arr at: 4) d: (arr at: 5).
	^e! !

!Event methodsFor: 'initializing'!
setType: t a: aa b: bb c: cc d: dd
	type := t.
	a := aa.
	b := bb.
	c := cc.
	d := dd! !

!Event methodsFor: 'accessing'!
type
	^type!
position
	^Point x: a y: b!
keyCode
	^a!
keyCharacter
	^Character value: c!
modifiers
	^b!
buttons
	^c!
scrollDx
	^c!
scrollDy
	^d!
newExtent
	^Point x: a y: b! !

!Event methodsFor: 'testing'!
isMouseMove
	^type = 1!
isMouseButton
	^type = 2!
isMouseDown
	^type = 2 and: [d = 1]!
isMouseUp
	^type = 2 and: [d = 0]!
isKeyDown
	^type = 3!
isKeyUp
	^type = 4!
isKeystroke
	^type = 3!
isScroll
	^type = 5!
isResize
	^type = 6!
isClose
	^type = 7! !
