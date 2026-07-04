"M5 — the Class Browser (UI.md §10), tested on the VM with the in-image
 harness. Navigate categories -> classes -> protocols -> selectors ->
 retained source; edit and ACCEPT to recompile live; do-it; click routing
 through the UISupervisor; and the full-stack screenshot to PNG (verified
 by reading the file back through Platform). The launcher driver sets the
 TestShotPath global to a scratch path for the PNG. The Workspace has its
 own suite (WorkspaceTests)."

TestCase subclass: #BrowserTests
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Testing'!

!BrowserTests methodsFor: 'tests'!
testBrowserNavigatesTheReflectionModel
	"the 'Collections' category comes from the class definitions in
	 kernel.st (the SystemClassCategories table — no manual classify:);
	 class lists are alphabetical, so Array is first. Array's own methods
	 sit in the source-declared 'printing' protocol; source is retained."
	| b |
	b := ClassBrowser bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 240 y: 120)).
	self assert: (b categoryNames includes: 'Collections').
	b selectCategoryNamed: 'Collections'.
	self assert: b classNames size >= 10.
	self assert: (b classNames at: 1) equals: 'Array'.
	self assert: (b classNames includes: 'String').
	b selectClassNamed: 'Array'.
	self assert: (b protocolNames includes: 'printing').
	b selectProtocolNamed: 'printing'.
	self assert: (b selectorNames includes: 'printString').
	b selectSelectorNamed: 'printString'.
	self assert: b sourcePane contents size > 0.
	self assert: b currentClass name asString equals: 'Array'!
testBrowserAcceptRecompilesLive
	"the headline gesture: edit the source pane, accept, and the method is
	 installed live — a subsequent send observes it, and the selector list
	 refreshes to include it."
	| b |
	b := ClassBrowser bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 240 y: 120)).
	b selectCategoryNamed: 'Collections'.
	b selectClassNamed: 'Array'.
	b selectProtocolNamed: 'printing'.
	b sourcePane contents: 'uiBrowserAccept ^777'.
	b accept.
	self assert: (Array new: 3) uiBrowserAccept equals: 777.
	self assert: (b selectorNames includes: 'uiBrowserAccept').
	b sourcePane contents: '6 * 7'.
	self assert: b doIt equals: 42!
testBrowserRendersAFramedFivePaneLayout
	"structural render check: the browser paints NO outer frame (the host
	 window's edges delimit it), a horizontal rule above the source pane,
	 3 vertical column rules — and scroll bars in the overflowing list
	 panes (category and class here; protocol/selector are empty)."
	| b f c bot |
	b := ClassBrowser bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 40 y: 30)).
	b selectCategoryNamed: 'Collections'.
	f := Form width: 40 height: 30.
	c := Canvas on: f.
	b displayOn: c.
	"no outer border: row 0 is white in the text areas of the populated
	 panes and across the empty protocol/selector panes"
	self assert: (f pixelValueAtX: 1 y: 0) equals: 0.
	self assert: (f pixelValueAtX: 11 y: 0) equals: 0.
	self assert: (f pixelValueAtX: 25 y: 0) equals: 0.
	self assert: (f pixelValueAtX: 35 y: 0) equals: 0.
	"the category and class panes overflow their 6-px-wide strips: the
	 scroll bar frame's top edge is black inside each strip"
	self assert: (f pixelValueAtX: 5 y: 0) equals: 1.
	self assert: (f pixelValueAtX: 15 y: 0) equals: 1.
	bot := true.
	0 to: 39 do: [:x | (f pixelValueAtX: x y: 29) = 0 ifFalse: [bot := false]].
	self assert: bot description: 'bottom row must be fully clear'.
	"the list/source divider is at 2/5 of height = row 12"
	self assert: (f pixelValueAtX: 20 y: 12) equals: 1.
	"a column rule exists at x = width//4 = 10 within the list area"
	self assert: (f pixelValueAtX: 10 y: 5) equals: 1!
testBrowserOpensAndScreenshotsTheFullStack
	"the agent/dev workflow (UI.md §4A): navigate the browser to a real
	 method and save the whole rendered screen to a PNG — exercising every
	 layer L0..L5 headlessly (reflection -> widgets -> BitBlt/font -> PNG
	 seam). The PNG is read back via Platform and its signature checked."
	| b f png sig |
	b := ClassBrowser bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 320 y: 200)).
	b selectCategoryNamed: 'Kernel'.
	b selectClassNamed: 'Object'.
	b selectProtocolNamed: 'accessing'.
	b selectSelectorNamed: 'yourself'.
	f := Form width: 320 height: 200.
	b displayOn: (Canvas on: f).
	self deny: TestShotPath isNil.
	f saveTo: TestShotPath.
	self assert: b currentSelector asString equals: 'yourself'.
	png := Platform readFile: TestShotPath.
	self assert: png size > 8.
	sig := Array with: 137 with: 80 with: 78 with: 71.
	1 to: 4 do: [:i |
		self assert: (png at: i) equals: (sig at: i)].
	self assert: (png at: 5) equals: 13.
	self assert: (png at: 6) equals: 10.
	self assert: (png at: 7) equals: 26.
	self assert: (png at: 8) equals: 10!
testBrowserClickNavigates
	"a real mouse-down posted through primitive 334, routed through the
	 UISupervisor -> the browser -> the class list pane under the pointer
	 selects a class and navigation fires. The class-list pane occupies
	 x[60,120), y[0,48); a click at (65,4) hits its first row = Array
	 (class lists are alphabetical)."
	| d b s |
	d := Display width: 240 height: 120.
	b := ClassBrowser bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 240 y: 120)).
	b selectCategoryNamed: 'Collections'.
	s := UISupervisor on: d.
	s rootView: b.
	Event postType: 2 a: 65 b: 4 c: 1 d: 1.
	s pumpEvents.
	self deny: b currentClass isNil.
	self assert: b currentClass name asString equals: 'Array'.
	self deny: b protocolNames isEmpty!
testBrowserThumbDragIsCapturedAcrossPanes
	"a real drag through the event queue: press on the category pane's
	 scroll thumb (x[54,60) of the 240-wide browser), drag down-right until
	 the pointer is over the protocol pane, release. The browser captures
	 the mouse on the down, so the moves keep scrolling the category list —
	 selecting nothing — until the up releases capture."
	| d b s pane |
	d := Display width: 240 height: 120.
	b := ClassBrowser bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 240 y: 120)).
	s := UISupervisor on: d.
	s rootView: b.
	pane := b categoryPane.
	self assert: pane needsScrollBar.
	self assert: pane topIndex equals: 1.
	Event postType: 2 a: 56 b: 3 c: 1 d: 1.
	Event postType: 1 a: 150 b: 40 c: 0 d: 0.
	s pumpEvents.
	self assert: pane isDraggingThumb.
	self assert: pane topIndex > 1.
	self assert: pane selectionIndex equals: 0.
	Event postType: 2 a: 150 b: 40 c: 1 d: 0.
	s pumpEvents.
	self deny: pane isDraggingThumb!
testBrowserShowsPlaceholderForUnretainedSource
	"selecting a selector whose method kept no source (kernel methods built
	 by the no-source materializers answer nil from sourceCodeAt:) must show
	 a note in the source pane, not crash the pane on nil."
	| b sel m info |
	b := ClassBrowser bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 240 y: 120)).
	b selectCategoryNamed: 'Collections'.
	b selectClassNamed: 'Array'.
	b selectProtocolNamed: (b protocolNames at: 1).
	sel := b selectorNames at: 1.
	m := b currentClass compiledMethodAt: sel asSymbol.
	info := m instVarAt: 7.
	m instVarAt: 7 put: nil.
	b selectSelectorNamed: sel.
	self assert: b sourcePane contents equals: '"no source retained for ' , sel , '"'.
	"restore: the slot also carries the source-declared protocol, and the
	 suite shares one image"
	m instVarAt: 7 put: info! !
