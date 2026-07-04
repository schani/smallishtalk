"Widget toolkit L4 (UI.md §8): the editable text view — the workhorse behind the
 browser's source pane and the Workspace. Caret + selection are character offsets
 0..size (positions between characters); [selStart,selStop) is the selection, a
 caret when equal. Fixed-width font, so offset<->pixel mapping is exact.

 `selectFrom:to:` (set selection to a range) is REQUIRED by UI.md §10.2 so the
 browser can land the caret on a compile error's offending token. `accept` runs
 the acceptAction hook with the current contents (the browser recompiles)."

View subclass: #TextPane
	instanceVariableNames: 'contents selStart selStop acceptAction dragAnchor'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Widgets'!

!TextPane methodsFor: 'initializing'!
setBounds: aRectangle
	super setBounds: aRectangle.
	contents := ''.
	selStart := 0.
	selStop := 0! !

!TextPane methodsFor: 'accessing'!
contents
	^contents!
contents: aString
	contents := aString.
	selStart := 0.
	selStop := 0!
selectionStart
	^selStart!
selectionStop
	^selStop!
selectedText
	^contents copyFrom: selStart + 1 to: selStop!
acceptAction: aBlock
	acceptAction := aBlock!
lineHeight
	^StrikeFont default height + 1!
topInset
	"blank rows between the pane's top edge and the first line of text; without
	 this the first line's cap height sits flush on the frame line above it
	 and reads as cut off."
	^2! !

!TextPane methodsFor: 'selecting'!
clampOffset: k
	^(k max: 0) min: contents size!
caretAt: k
	| c |
	c := self clampOffset: k.
	selStart := c.
	selStop := c!
selectFrom: i to: j
	"set the selection to the range [i,j] (UI.md §10.2). Clamped and ordered."
	| a b |
	a := self clampOffset: (i min: j).
	b := self clampOffset: (i max: j).
	selStart := a.
	selStop := b!
selectAll
	self selectFrom: 0 to: contents size!
hasSelection
	^selStop > selStart! !

!TextPane methodsFor: 'editing'!
replaceSelectionWith: aString
	| left right |
	left := contents copyFrom: 1 to: selStart.
	right := contents copyFrom: selStop + 1 to: contents size.
	contents := left , aString , right.
	self caretAt: selStart + aString size!
insertString: aString
	self replaceSelectionWith: aString!
insertCharacter: aCode
	| s |
	s := String new: 1.
	s at: 1 put: aCode.
	self replaceSelectionWith: s!
backspace
	self hasSelection
		ifTrue: [self replaceSelectionWith: '']
		ifFalse: [
			selStart > 0 ifTrue: [
				selStart := selStart - 1.
				self replaceSelectionWith: '']]!
delete
	self hasSelection
		ifTrue: [self replaceSelectionWith: '']
		ifFalse: [
			selStop < contents size ifTrue: [
				selStop := selStop + 1.
				self replaceSelectionWith: '']]! !

!TextPane methodsFor: 'clipboard'!
cut
	TextPasteBuffer := self selectedText.
	self replaceSelectionWith: ''!
copySelection
	TextPasteBuffer := self selectedText!
paste
	TextPasteBuffer isNil ifTrue: [^self].
	self replaceSelectionWith: TextPasteBuffer! !

!TextPane methodsFor: 'navigation'!
moveLeft
	self caretAt: selStart - 1!
moveRight
	self caretAt: selStop + 1! !

!TextPane methodsFor: 'accepting'!
accept
	"the classic 'accept' gesture: run the hook with the current contents."
	acceptAction isNil ifTrue: [^self].
	^acceptAction value: contents! !

!TextPane methodsFor: 'geometry'!
lineOfOffset: k
	"answer the 0-based line number that offset k falls on."
	| line i |
	line := 0.
	i := 1.
	[i <= k] whileTrue: [
		(contents at: i) = 10 ifTrue: [line := line + 1].
		i := i + 1].
	^line!
lineStartOfOffset: k
	"answer the offset of the start of the line containing offset k."
	| start i |
	start := 0.
	i := 1.
	[i <= k] whileTrue: [
		(contents at: i) = 10 ifTrue: [start := i].
		i := i + 1].
	^start!
pointForOffset: k
	"pixel position of the caret at offset k: measure the line's prefix
	 through the (proportional) font instead of multiplying a column count."
	| start |
	start := self lineStartOfOffset: k.
	^Point x: bounds left + 1
			+ (StrikeFont default widthOfString: (contents copyFrom: start + 1 to: k))
		y: bounds top + self topInset + ((self lineOfOffset: k) * self lineHeight)! !

!TextPane methodsFor: 'drawing'!
displayOn: aCanvas
	| font lines y |
	aCanvas clipTo: bounds.
	font := StrikeFont default.
	"draw each line of text."
	lines := self splitLines.
	y := bounds top + self topInset.
	lines do: [:line |
		aCanvas drawString: line at: (Point x: bounds left + 1 y: y).
		y := y + self lineHeight].
	self hasSelection
		ifTrue: [self drawSelectionOn: aCanvas]
		ifFalse: [self drawCaretOn: aCanvas].
	^self!
splitLines
	| out ws i c |
	out := OrderedCollection new.
	ws := WriteStream on: (String new: 8).
	i := 1.
	[i <= contents size] whileTrue: [
		c := contents at: i.
		c = 10
			ifTrue: [out add: ws contents. ws := WriteStream on: (String new: 8)]
			ifFalse: [ws nextPut: c].
		i := i + 1].
	out add: ws contents.
	^out!
drawCaretOn: aCanvas
	| p fh |
	p := self pointForOffset: selStart.
	fh := StrikeFont default height.
	aCanvas line: p to: (Point x: p x y: p y + fh - 1) value: 1!
drawSelectionOn: aCanvas
	"reverse-video each selected character cell (skip newlines); cells are as
	 wide as their character's advance."
	| font cw fh k p |
	font := StrikeFont default.
	fh := font height.
	k := selStart.
	[k < selStop] whileTrue: [
		(contents at: k + 1) = 10 ifFalse: [
			p := self pointForOffset: k.
			cw := font widthOfCharacter: (contents at: k + 1).
			aCanvas fillRectangle: (Rectangle origin: p corner: (Point x: p x + cw y: p y + fh)) value: 1.
			font displayString: (contents copyFrom: k + 1 to: k + 1) on: aCanvas form at: p rule: 6].
		k := k + 1].
	^self! !

!TextPane methodsFor: 'events'!
handleEvent: anEvent
	"left press places the caret and anchors a drag-selection; moves while
	 the button is held sweep it (the router's mouse capture keeps them
	 coming wherever the pointer wanders); the release drops the anchor.
	 The right button is NOT ours — the owner (e.g. the Workspace) pops its
	 operate menu on it."
	anEvent isKeystroke ifTrue: [^self handleKey: anEvent].
	anEvent isMouseDown ifTrue: [
		anEvent isRightMouseDown ifTrue: [^false].
		^self beginSelectionAt: anEvent position].
	anEvent isMouseMove ifTrue: [^self dragSelectionTo: anEvent position].
	anEvent isMouseUp ifTrue: [dragAnchor := nil. ^false].
	^false!
handleKey: anEvent
	"insert only what a byte String can hold (32..255, and 10 for a line
	 break) — anything else (e.g. a code point > 255) must be ignored, not
	 crash the store. 8 is backspace; 28/29 are the classic left/right
	 arrow control codes the host translates arrow keys to."
	| code |
	code := anEvent keyCharacter value.
	code = 8 ifTrue: [self backspace. ^true].
	code = 28 ifTrue: [self moveLeft. ^true].
	code = 29 ifTrue: [self moveRight. ^true].
	code = 10 ifTrue: [self insertCharacter: code. ^true].
	(code >= 32 and: [code <= 255]) ifTrue: [self insertCharacter: code. ^true].
	^false!
beginSelectionAt: aPoint
	(self containsPoint: aPoint) ifFalse: [^false].
	dragAnchor := self offsetForPoint: aPoint.
	self caretAt: dragAnchor.
	^true!
dragSelectionTo: aPoint
	dragAnchor isNil ifTrue: [^false].
	self selectFrom: dragAnchor to: (self offsetForPoint: aPoint).
	^true!
placeCaretAt: aPoint
	(self containsPoint: aPoint) ifFalse: [^false].
	self caretAt: (self offsetForPoint: aPoint).
	^true!
offsetForPoint: aPoint
	"the character offset nearest aPoint: walk the point's line accumulating
	 (proportional) advances until it falls before a character's midpoint.
	 No containment check — a drag tracks past the edges and clamps."
	| line off i lineNo font x cw |
	line := ((aPoint y - bounds top - self topInset) max: 0) // self lineHeight.
	"walk to the start of that line"
	off := 0.
	lineNo := 0.
	i := 1.
	[i <= contents size and: [lineNo < line]] whileTrue: [
		(contents at: i) = 10 ifTrue: [lineNo := lineNo + 1].
		i := i + 1].
	off := i - 1.
	font := StrikeFont default.
	x := bounds left + 1.
	[off < contents size and: [(contents at: off + 1) ~= 10]] whileTrue: [
		cw := font widthOfCharacter: (contents at: off + 1).
		aPoint x < (x + (cw // 2)) ifTrue: [^off].
		x := x + cw.
		off := off + 1].
	^off! !
