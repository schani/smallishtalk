"Window-manager L3 (UI.md §7): the Display — the screen Form plus the desktop
 of Tracks that tile to fill it. Holds the cursor (a small Form blitted with XOR
 so it is reversible without saving background, §7.4). This is the distinguished
 Form the host ever presents (primPixelBlit)."

Object subclass: #Display
	instanceVariableNames: 'form width height tracks cursor cursorX cursorY cursorShown cursorVisible'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-WindowManager'!

!Display class methodsFor: 'creation'!
width: w height: h
	| d |
	d := self new.
	d setWidth: w height: h.
	^d! !

!Display methodsFor: 'initializing'!
setWidth: w height: h
	width := w.
	height := h.
	form := Form width: w height: h.
	tracks := OrderedCollection new.
	cursor := self buildCursor.
	cursorX := 0.
	cursorY := 0.
	cursorShown := false.
	cursorVisible := true!
buildCursor
	"a tiny 5x5 caret; ink where set."
	| c |
	c := Form width: 5 height: 5.
	0 to: 4 do: [:i | c pixelAtX: 0 y: i put: 1].
	0 to: 4 do: [:i | c pixelAtX: i y: 0 put: 1].
	0 to: 4 do: [:i | c pixelAtX: i y: i put: 1].
	^c! !

!Display methodsFor: 'accessing'!
form
	^form!
width
	^width!
height
	^height!
tracks
	^tracks!
cursorX
	^cursorX!
cursorY
	^cursorY! !

!Display methodsFor: 'tracks'!
addTrackLeft: l width: w
	| t |
	t := Track left: l width: w height: height.
	tracks add: t.
	^t! !

!Display methodsFor: 'cursor'!
cursorX: cx y: cy
	cursorX := cx.
	cursorY := cy!
cursorVisible
	^cursorVisible!
cursorVisible: aBoolean
	"windowed runs turn the software cursor off — the OS already draws one
	 over the window, and it tracks the mouse far better than a repaint can."
	cursorVisible := aBoolean!
blitCursorXor
	"XOR the cursor Form onto the screen; a second call erases it (rule 6).
	 A no-op when the software cursor is off (windowed runs)."
	| bb |
	cursorVisible ifFalse: [^self].
	bb := BitBlt new.
	bb destForm: form.
	bb sourceForm: cursor.
	bb combinationRule: 6.
	bb destX: cursorX.
	bb destY: cursorY.
	bb width: cursor width.
	bb height: cursor height.
	bb sourceX: 0.
	bb sourceY: 0.
	bb clipX: 0.
	bb clipY: 0.
	bb clipWidth: width.
	bb clipHeight: height.
	bb copyBits.
	^self! !

!Display methodsFor: 'drawing'!
draw
	"repaint the whole desktop: white background, every viewer, then the XOR
	 cursor on top."
	| canvas |
	form fill: 0.
	canvas := Canvas on: form.
	tracks do: [:t | t drawOn: canvas].
	self blitCursorXor.
	cursorShown := true.
	^self!
flush
	"present the Display on the host (windowed: a real window; headless: the
	 ARGB buffer). primPixelBlit, UI.md §4.1."
	form flush.
	^self!
asAsciiString
	^form asAsciiString! !
