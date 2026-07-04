"M3 — the widget toolkit (UI.md §8), tested on the VM with the in-image
 harness. Model behavior (editing, selection, list navigation, layout) is
 asserted directly on the widgets; one reverse-video render is an ASCII
 golden."

TestCase subclass: #WidgetTests
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Testing'!

!WidgetTests methodsFor: 'tests'!
testTextPaneEditingSelectionAndClipboard
	| tp |
	tp := TextPane bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 100 y: 40)).
	tp contents: 'hello'.
	tp caretAt: 5.
	tp insertString: ' world'.
	self assert: tp contents equals: 'hello world'.
	tp selectFrom: 0 to: 5.
	self assert: tp selectedText equals: 'hello'.
	tp cut.
	self assert: tp contents equals: ' world'.
	tp caretAt: 0.
	tp paste.
	self assert: tp contents equals: 'hello world'.
	tp selectFrom: 6 to: 11.
	tp backspace.
	"deleting the selected 'world' from 'hello world' correctly leaves the
	 trailing space"
	self assert: tp contents equals: 'hello '.
	self assert: tp selectionStart equals: 6.
	tp contents: 'abc'.
	tp caretAt: 3.
	tp insertCharacter: 100.
	self assert: tp contents equals: 'abcd'.
	tp backspace.
	self assert: tp contents equals: 'abc'!
testTextPaneSelectRangeLandsOnAToken
	"the compile-error interaction (UI.md §10.2): set the selection to the
	 offending token's range so the caret lands on it."
	| tp |
	tp := TextPane bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 100 y: 40)).
	tp contents: 'foo := bar baz'.
	tp selectFrom: 7 to: 10.
	self assert: tp selectedText equals: 'bar'.
	self assert: tp selectionStart equals: 7.
	self assert: tp selectionStop equals: 10!
testListPaneSelectionNavigationAndClick
	| lp log |
	lp := ListPane bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 60 y: 40)).
	log := OrderedCollection new.
	lp items: (Array with: 'Object' with: 'Array' with: 'String').
	lp onSelect: [:i | log add: i].
	lp select: 2.
	self assert: lp selectedItem equals: 'Array'.
	lp selectNext.
	self assert: lp selectedItem equals: 'String'.
	lp selectPrevious.
	lp selectPrevious.
	self assert: lp selectedItem equals: 'Object'.
	"rows are 15px apart below the 2px topInset, so y=20 is on row 2"
	self assert: (lp selectAtPoint: (Point x: 3 y: 20)).
	self assert: lp selectedItem equals: 'Array'.
	self assert: log size equals: 5!
testPanedViewSplitsBoundsEqually
	| pv a b |
	pv := PanedView bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 30 y: 10)).
	pv horizontal.
	a := View bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 1 y: 1)).
	b := View bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 1 y: 1)).
	pv addPane: a.
	pv addPane: b.
	pv layout.
	self assert: a bounds printString equals: '0@0 corner: 15@10'.
	self assert: b bounds printString equals: '15@0 corner: 30@10'!
testMenuPaneResolvesCommandAtPoint
	| m |
	m := MenuPane bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 40 y: 32)).
	m commands: (Array with: #Accept with: #Cancel with: #Format).
	"menu rows are 15px apart below the 1px frame: y=2 is row 1, y=17 row 2"
	self assert: (m commandAtPoint: (Point x: 5 y: 2)) equals: #Accept.
	self assert: (m commandAtPoint: (Point x: 5 y: 17)) equals: #Cancel.
	self assert: (m commandAtPoint: (Point x: 5 y: 100)) equals: nil!
testListPaneScrollBarScrollsWithoutSelecting
	"a ListPane whose items overflow shows a 6-px scroll-bar strip on its
	 right: a click in the strip jump-scrolls (topIndex from the click's
	 fraction of the bar) without touching the selection; row clicks then
	 select relative to the new topIndex. A pane whose items fit shows no
	 strip and keeps the full row width."
	| lp items f |
	items := OrderedCollection new.
	1 to: 10 do: [:k | items add: 'item' , k printString].
	lp := ListPane bounds: (Rectangle left: 0 top: 0 right: 40 bottom: 32).
	lp items: items.
	"visibleRows = (32-2)//15 = 2, 10 items -> the strip is x[34,40)"
	self assert: lp needsScrollBar.
	self assert: lp contentRight equals: 34.
	lp select: 1.
	"click at the bar's bottom: t = 31*10//32+1 = 10, clamped to maxTop 9"
	self assert: (lp selectAtPoint: (Point x: 36 y: 31)).
	self assert: lp topIndex equals: 9.
	self assert: lp selectionIndex equals: 1.
	"a row click now selects relative to the scrolled top: row 2 = item 10"
	self assert: (lp selectAtPoint: (Point x: 3 y: 17)).
	self assert: lp selectedItem equals: 'item10'.
	"click at the bar's top scrolls back"
	self assert: (lp selectAtPoint: (Point x: 36 y: 0)).
	self assert: lp topIndex equals: 1.
	"the rendered strip: the bar frame's top-left corner pixel is black"
	f := Form width: 40 height: 32.
	lp displayOn: (Canvas on: f).
	self assert: (f pixelValueAtX: 34 y: 0) equals: 1.
	"no strip when the items fit"
	lp items: (Array with: 'a' with: 'b').
	self deny: lp needsScrollBar.
	self assert: lp contentRight equals: 40!
testScrollBarThumbTracksPosition
	| sb |
	sb := ScrollBar bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 5 y: 100)).
	sb total: 200 visible: 50 position: 0.
	self assert: sb thumbRect printString equals: '0@0 corner: 5@25'.
	sb total: 200 visible: 50 position: 100.
	self assert: sb thumbRect printString equals: '0@50 corner: 5@75'!
testListPaneSelectedRowIsReverseVideo
	"rows start at the 2-px topInset, 15 px apart (font height 14 + 1
	 leading). 'AB' and 'EF' draw black-on-white; the selected 'CD' is a
	 14-row black bar with the glyphs XORed to white — reverse video."
	| f lp c |
	f := Form width: 24 height: 47.
	lp := ListPane bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 24 y: 47)).
	lp items: (Array with: 'AB' with: 'CD' with: 'EF').
	lp select: 2.
	c := Canvas on: f.
	lp displayOn: c.
	self assert: f asAsciiString equals: '........................
........................
........................
........................
.....#.....#####........
....#.#....#....#.......
....#.#....#....#.......
...#...#...#....#.......
...#...#...#####........
...#####...#....#.......
..#.....#..#....#.......
..#.....#..#....#.......
..#.....#..#####........
........................
........................
........................
........................
########################
########################
####....###.....########
###.####.##.####.#######
##.########.#####.######
##.########.#####.######
##.########.#####.######
##.########.#####.######
##.########.#####.######
###.####.##.####.#######
####....###.....########
########################
########################
########################
........................
........................
........................
..######..######........
..#.......#.............
..#.......#.............
..#.......#.............
..######..#####.........
..#.......#.............
..#.......#.............
..#.......#.............
..######..#.............
........................
........................
........................
........................
'! !
