"Widget toolkit L4 (UI.md §8): the editable text view — the workhorse behind the
 browser's source pane and the Workspace. Caret + selection are character offsets
 0..size (positions between characters); [selStart,selStop) is the selection, a
 caret when equal. Fixed-width font, so offset<->pixel mapping is exact.

 `selectFrom:to:` (set selection to a range) is REQUIRED by UI.md §10.2 so the
 browser can land the caret on a compile error's offending token. `accept` runs
 the acceptAction hook with the current contents (the browser recompiles)."

View subclass: #TextPane
	instanceVariableNames: 'contents selStart selStop acceptAction'
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
charWidth
	^StrikeFont default width!
lineHeight
	^StrikeFont default height + 1! !

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
columnOfOffset: k
	"answer the 0-based column of offset k on its line."
	| col i |
	col := 0.
	i := 1.
	[i <= k] whileTrue: [
		(contents at: i) = 10 ifTrue: [col := 0] ifFalse: [col := col + 1].
		i := i + 1].
	^col!
pointForOffset: k
	^Point x: bounds left + 1 + ((self columnOfOffset: k) * self charWidth)
		y: bounds top + ((self lineOfOffset: k) * self lineHeight)! !

!TextPane methodsFor: 'drawing'!
displayOn: aCanvas
	| font lines y |
	aCanvas clipTo: bounds.
	font := StrikeFont default.
	"draw each line of text."
	lines := self splitLines.
	y := bounds top.
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
	"reverse-video each selected character cell (skip newlines)."
	| font cw fh k p |
	font := StrikeFont default.
	cw := self charWidth.
	fh := font height.
	k := selStart.
	[k < selStop] whileTrue: [
		(contents at: k + 1) = 10 ifFalse: [
			p := self pointForOffset: k.
			aCanvas fillRectangle: (Rectangle origin: p corner: (Point x: p x + cw y: p y + fh)) value: 1.
			font displayString: (contents copyFrom: k + 1 to: k + 1) on: aCanvas form at: p rule: 6].
		k := k + 1].
	^self! !

!TextPane methodsFor: 'events'!
handleEvent: anEvent
	anEvent isKeystroke ifTrue: [^self handleKey: anEvent].
	anEvent isMouseDown ifTrue: [^self placeCaretAt: anEvent position].
	^false!
handleKey: anEvent
	| code |
	code := anEvent keyCharacter value.
	code = 8 ifTrue: [self backspace. ^true].
	(code >= 32 or: [code = 10]) ifTrue: [self insertCharacter: code. ^true].
	^false!
placeCaretAt: aPoint
	"map a click to the nearest offset on its line."
	| line col off i lineNo |
	(self containsPoint: aPoint) ifFalse: [^false].
	line := (aPoint y - bounds top) // self lineHeight.
	col := (aPoint x - bounds left - 1) // self charWidth.
	col < 0 ifTrue: [col := 0].
	"walk to the start of that line, then advance col columns (clamped to EOL)."
	off := 0.
	lineNo := 0.
	i := 1.
	[i <= contents size and: [lineNo < line]] whileTrue: [
		(contents at: i) = 10 ifTrue: [lineNo := lineNo + 1].
		i := i + 1].
	off := i - 1.
	[col > 0 and: [off < contents size and: [(contents at: off + 1) ~= 10]]] whileTrue: [
		off := off + 1.
		col := col - 1].
	self caretAt: off.
	^true! !
