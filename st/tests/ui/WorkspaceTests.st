"M6 — the Workspace (UI.md §10.3), tested on the VM with the in-image
 harness, through the REAL event pipeline: type via posted keystrokes,
 drag-select with the mouse, right-click the selection to pop up the
 do-it / print-it menu, and click a menu row to evaluate. Scripted events
 go through primitive 334 -> UISupervisor -> Workspace, the same path the
 windowed UI feeds."

TestCase subclass: #WorkspaceTests
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Testing'!

!WorkspaceTests methodsFor: 'helpers'!
openWorkspace
	"a 200x100 workspace wired to a live supervisor, as the windowed driver
	 builds it. Answers (workspace supervisor)."
	| d w s |
	d := Display width: 200 height: 100.
	w := Workspace bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 200 y: 100)).
	s := UISupervisor on: d.
	s rootView: w.
	^Array with: w with: s!
xAtOffset: k in: aString
	"the pixel x where character offset k of aString's first line sits
	 (TextPane draws text at bounds left + 1)."
	^1 + (StrikeFont default widthOfString: (aString copyFrom: 1 to: k))! !

!WorkspaceTests methodsFor: 'tests'!
testWorkspaceDoItAndPrintIt
	"the programmatic contract: with no selection, do-it / print-it work on
	 the whole buffer, and print-it appends the result."
	| w |
	w := Workspace bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 100 y: 50)).
	w contents: '3 * 14'.
	self assert: w doIt equals: 42.
	w contents: '10 - 3'.
	w printIt.
	self assert: w contents equals: '10 - 3 7'!
testDoItEvaluatesTheSelectionWhenPresent
	"with a selection, do-it evaluates ONLY the selected text — the rest of
	 the buffer ('zork' would not compile) is never touched."
	| w |
	w := Workspace bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 100 y: 50)).
	w contents: '2 + 2. zork zork'.
	w textPane selectFrom: 0 to: 5.
	self assert: w doIt equals: 4!
testPrintItInsertsAfterTheSelectionAndSelectsTheResult
	"the classic print-it: the printString lands right after the selection
	 (not at the buffer's end) and becomes the new selection, so the next
	 keystroke replaces it."
	| w |
	w := Workspace bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 100 y: 50)).
	w contents: '10 - 3. 5'.
	w textPane selectFrom: 0 to: 6.
	w printIt.
	self assert: w contents equals: '10 - 3 7. 5'.
	self assert: w textPane selectedText equals: ' 7'!
testTypingThroughTheEventQueue
	"keystrokes posted on the host queue (the pipeline real key events feed)
	 edit the buffer: printables insert, 8 backspaces, 10 breaks the line."
	| ws w s |
	ws := self openWorkspace.
	w := ws at: 1.
	s := ws at: 2.
	Event postType: 3 a: 0 b: 0 c: 104 d: 0.
	Event postType: 3 a: 0 b: 0 c: 105 d: 0.
	s pumpEvents.
	self assert: w contents equals: 'hi'.
	Event postType: 3 a: 0 b: 0 c: 8 d: 0.
	s pumpEvents.
	self assert: w contents equals: 'h'.
	Event postType: 3 a: 0 b: 0 c: 10 d: 0.
	Event postType: 3 a: 0 b: 0 c: 120 d: 0.
	s pumpEvents.
	self assert: w contents equals: 'h
x'.
	"left arrow (control code 28, the host's translation of Key::Left)
	 moves the caret, so the next insert lands before the x"
	Event postType: 3 a: 0 b: 0 c: 28 d: 0.
	Event postType: 3 a: 0 b: 0 c: 121 d: 0.
	s pumpEvents.
	self assert: w contents equals: 'h
yx'!
testMouseDragSelectsText
	"press-drag-release selects the swept range; after the release, further
	 moves must NOT keep extending the selection."
	| ws w s x0 x5 |
	ws := self openWorkspace.
	w := ws at: 1.
	s := ws at: 2.
	w contents: 'hello world'.
	x0 := self xAtOffset: 0 in: 'hello world'.
	x5 := self xAtOffset: 5 in: 'hello world'.
	Event postType: 2 a: x0 b: 4 c: 1 d: 1.
	Event postType: 1 a: x5 b: 4 c: 0 d: 0.
	s pumpEvents.
	self assert: w textPane selectedText equals: 'hello'.
	Event postType: 2 a: x5 b: 4 c: 1 d: 0.
	Event postType: 1 a: (self xAtOffset: 11 in: 'hello world') b: 4 c: 0 d: 0.
	s pumpEvents.
	self assert: w textPane selectedText equals: 'hello'.
	"a backward drag selects the same range swept right-to-left"
	Event postType: 2 a: x5 b: 4 c: 1 d: 1.
	Event postType: 1 a: x0 b: 4 c: 0 d: 0.
	Event postType: 2 a: x0 b: 4 c: 1 d: 0.
	s pumpEvents.
	self assert: w textPane selectedText equals: 'hello'!
testRightClickOnSelectionOpensTheMenu
	"right-down ON the selected text pops up the do-it / print-it menu; a
	 left click outside the menu dismisses it without evaluating anything."
	| ws w s |
	ws := self openWorkspace.
	w := ws at: 1.
	s := ws at: 2.
	w contents: '3 * 14'.
	w textPane selectAll.
	Event postType: 2 a: 5 b: 4 c: 2 d: 1.
	s pumpEvents.
	self assert: w menuVisible.
	self assert: (w menu commands includes: 'do it').
	self assert: (w menu commands includes: 'print it').
	Event postType: 2 a: 190 b: 90 c: 1 d: 1.
	s pumpEvents.
	self deny: w menuVisible.
	self assert: w contents equals: '3 * 14'!
testRightClickNeedsTheSelectionUnderThePointer
	"no selection, or a right-click somewhere OFF the selected range,
	 opens nothing."
	| ws w s |
	ws := self openWorkspace.
	w := ws at: 1.
	s := ws at: 2.
	w contents: '3 * 14'.
	Event postType: 2 a: 5 b: 4 c: 2 d: 1.
	s pumpEvents.
	self deny: w menuVisible.
	w textPane selectFrom: 0 to: 1.
	Event postType: 2 a: (self xAtOffset: 5 in: '3 * 14') b: 4 c: 2 d: 1.
	s pumpEvents.
	self deny: w menuVisible!
testMenuDoItEvaluatesTheSelection
	"the headline gesture: select an expression, right-click it, click
	 'do it' — it is evaluated, the menu closes, the text is untouched."
	| ws w s m |
	ws := self openWorkspace.
	w := ws at: 1.
	s := ws at: 2.
	w contents: '3 * 14. zork'.
	w textPane selectFrom: 0 to: 6.
	Event postType: 2 a: 3 b: 4 c: 2 d: 1.
	s pumpEvents.
	m := w menu.
	self deny: m isNil.
	"menu rows are 15px apart below the 1px frame: top+2 is row 1 = do it"
	Event postType: 2 a: m bounds left + 2 b: m bounds top + 2 c: 1 d: 1.
	s pumpEvents.
	self assert: w lastResult equals: 42.
	self deny: w menuVisible.
	self assert: w contents equals: '3 * 14. zork'!
testMenuPrintItInsertsTheResult
	"select, right-click, click 'print it': the result's printString lands
	 after the selection and is left selected."
	| ws w s m |
	ws := self openWorkspace.
	w := ws at: 1.
	s := ws at: 2.
	w contents: '10 - 3. zork'.
	w textPane selectFrom: 0 to: 6.
	Event postType: 2 a: 3 b: 4 c: 2 d: 1.
	s pumpEvents.
	m := w menu.
	self deny: m isNil.
	"row 2 = print it"
	Event postType: 2 a: m bounds left + 2 b: m bounds top + 2 + 15 c: 1 d: 1.
	s pumpEvents.
	self assert: w contents equals: '10 - 3 7. zork'.
	self assert: w textPane selectedText equals: ' 7'.
	self deny: w menuVisible!
testMenuStaysInsideTheWorkspace
	"a right-click near the right edge must not hang the menu off-screen:
	 its bounds are clamped into the workspace."
	| ws w s text |
	ws := self openWorkspace.
	w := ws at: 1.
	s := ws at: 2.
	text := 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'.
	w contents: text.
	w textPane selectAll.
	Event postType: 2 a: 195 b: 4 c: 2 d: 1.
	s pumpEvents.
	self assert: w menuVisible.
	self assert: w menu bounds right <= 200.
	self assert: w menu bounds bottom <= 100! !
