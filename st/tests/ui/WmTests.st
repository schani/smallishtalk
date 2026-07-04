"M2 — the event loop + Oberon-style tiling window manager (UI.md §6, §7),
 tested on the VM with the in-image harness. Proves: viewers tile to fill a
 track; resize moves the split; events (posted through primitive 334, the
 same pipeline real devices feed) are drained every tick independent of
 redraw (UI.md §3.2); and drawing has a deterministic work-counter budget
 (UI.md §12 item 5, §13)."

TestCase subclass: #WmTests
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Testing'!

!WmTests methodsFor: 'tests'!
testTwoViewersTileToFillTheTrack
	"two viewers, each 20 tall, stacked; each: top border, name glyph in the
	 14px-font menu strip, a menu-strip rule at viewer-relative y=16, a
	 bottom border. Cursor parked off-screen so this asserts pure tiling."
	| d t |
	d := Display width: 16 height: 40.
	t := d addTrackLeft: 0 width: 16.
	t open: (Viewer named: 'A').
	t open: (Viewer named: 'B').
	d cursorX: 100 y: 100.
	d draw.
	self assert: d asAsciiString equals: '################
#..............#
#..............#
#.....#........#
#....#.#.......#
#....#.#.......#
#...#...#......#
#...#...#......#
#...#####......#
#..#.....#.....#
#..#.....#.....#
#..#.....#.....#
#..............#
#..............#
#..............#
#..............#
################
#..............#
#..............#
################
################
#..............#
#..............#
#..#####.......#
#..#....#......#
#..#....#......#
#..#....#......#
#..#####.......#
#..#....#......#
#..#....#......#
#..#....#......#
#..#####.......#
#..............#
#..............#
#..............#
#..............#
################
#..............#
#..............#
################
'!
testResizeMovesTheSplitBetweenViewers
	| d t a b |
	d := Display width: 16 height: 24.
	t := d addTrackLeft: 0 width: 16.
	a := Viewer named: 'A'.
	b := Viewer named: 'B'.
	t open: a.
	t open: b.
	"move the split up: upper neighbor (a) grows by 4, b shrinks by 4"
	t resize: b by: 4.
	d draw.
	self assert: a height equals: 16.
	self assert: b height equals: 8.
	self assert: a bounds printString equals: '0@0 corner: 16@16'.
	self assert: b bounds printString equals: '0@16 corner: 16@24'!
testEventsDrainEveryTickIndependentOfRedraw
	"a mouse-move then a close event are posted. pumpEvents must drain BOTH
	 with no blit in between (UI.md §3.2): the cursor moves and running goes
	 false. Nothing is drawn here at all."
	| d s |
	d := Display width: 40 height: 40.
	s := UISupervisor on: d.
	Event postType: 1 a: 12 b: 7 c: 0 d: 0.
	Event postType: 7 a: 0 b: 0 c: 0 d: 0.
	s pumpEvents.
	self assert: d cursorX equals: 12.
	self assert: d cursorY equals: 7.
	self assert: s running equals: false!
testDrawHasADeterministicWorkBudget
	"one runStep drawing two named viewers. Everything blits: 1 whole-form
	 clear + per viewer (4 frame edges + 1 separator line + 1 name glyph)
	 x 2 + 1 XOR cursor = 14 BitBlt calls, and 1 frame presented. These are
	 the deterministic WORK metrics CI asserts (timing is only reported).
	 40 tall so both viewers fit the 14px-font menu strip."
	| d t s |
	d := Display width: 32 height: 40.
	t := d addTrackLeft: 0 width: 32.
	t open: (Viewer named: 'A').
	t open: (Viewer named: 'B').
	s := UISupervisor on: d.
	Profiler resetCounters.
	s runStep.
	self assert: (s probe counterNamed: 'ui.bitblt_calls') equals: 14.
	self assert: (s probe counterNamed: 'ui.frames_presented') equals: 1!
testXorCursorIsVisibleAndReversible
	"the cursor is a small Form blitted with XOR (rule 6, UI.md §7.4):
	 drawing it changes the screen; blitting it a second time restores the
	 screen exactly, with no saved background."
	| d blank withCursor erased |
	d := Display width: 12 height: 6.
	d cursorX: 100 y: 100.
	d draw.
	blank := d asAsciiString.
	d cursorX: 2 y: 1.
	d draw.
	withCursor := d asAsciiString.
	d blitCursorXor.
	erased := d asAsciiString.
	self deny: withCursor = blank.
	self assert: erased = blank! !
