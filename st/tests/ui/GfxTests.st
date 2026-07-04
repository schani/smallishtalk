"M1 — the Smalltalk graphics kernel (UI.md §5), tested on the VM with the
 in-image harness: Points, Rectangles, Forms, the BitBlt primitive-vs-
 fallback oracle, and deterministic 1-bit rendering (text via the baked-in
 strike font, a Bresenham diagonal, a Canvas rectangle frame) asserted as
 ASCII-art goldens."

TestCase subclass: #GfxTests
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Testing'!

!GfxTests methodsFor: 'tests'!
testPointAndRectangle
	| p q r |
	p := Point x: 3 y: 4.
	q := Point x: 10 y: 1.
	self assert: (p + q) printString equals: '13@5'.
	self assert: (p max: q) printString equals: '10@4'.
	r := Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 8 y: 4).
	self assert: r area equals: 32.
	self assert: (r containsPoint: (Point x: 7 y: 3)).
	self deny: (r containsPoint: (Point x: 8 y: 0))!
testFormCopyStoreAndXor
	| dest src |
	dest := Form width: 8 height: 2.
	src := Form width: 8 height: 2.
	src fill: 1.
	dest copyFrom: src boundingBox in: src to: (Point x: 0 y: 0) rule: 3.
	self assert: dest asAsciiString equals: '########
########
'.
	dest copyFrom: src boundingBox in: src to: (Point x: 0 y: 0) rule: 6.
	self assert: dest asAsciiString equals: '........
........
'!
testBitBltPrimitiveMatchesFallback
	"the primitive-vs-fallback oracle (UI.md §12 item 2): the same OR blit
	 through primitive 332 and through the pure-Smalltalk fallback must
	 leave identical bits."
	| destA destB src bb bbf |
	destA := Form width: 8 height: 1.
	destB := Form width: 8 height: 1.
	destA bits at: 1 put: 85.
	destB bits at: 1 put: 85.
	src := Form width: 8 height: 1.
	src bits at: 1 put: 178.
	bb := BitBlt new.
	bb destForm: destA. bb sourceForm: src. bb combinationRule: 7.
	bb destX: 0. bb destY: 0. bb width: 8. bb height: 1.
	bb sourceX: 0. bb sourceY: 0. bb clipX: 0. bb clipY: 0. bb clipWidth: 8. bb clipHeight: 1.
	bb copyBits.
	bbf := BitBlt new.
	bbf destForm: destB. bbf sourceForm: src. bbf combinationRule: 7.
	bbf destX: 0. bbf destY: 0. bbf width: 8. bbf height: 1.
	bbf sourceX: 0. bbf sourceY: 0. bbf clipX: 0. bbf clipY: 0. bbf clipWidth: 8. bbf clipHeight: 1.
	bbf copyBitsFallback.
	self assert: destA bits = destB bits description: 'primitive and fallback bits differ'.
	self assert: destA asAsciiString equals: '####.###
'!
testTextRendersTheStrikeFont
	"'A' has a 9px advance, 'B' 8px; caps sit on the baseline at row 10
	 (ascent 11), with the 3 descent rows blank."
	| f |
	f := Form width: 17 height: 14.
	StrikeFont default displayString: 'AB' on: f at: (Point x: 0 y: 0).
	self assert: f asAsciiString equals: '.................
.................
....#.....#####..
...#.#....#....#.
...#.#....#....#.
..#...#...#....#.
..#...#...#####..
..#####...#....#.
.#.....#..#....#.
.#.....#..#....#.
.#.....#..#####..
.................
.................
.................
'!
testPenDrawsABresenhamDiagonal
	| f pen |
	f := Form width: 5 height: 5.
	pen := Pen on: f.
	pen place: (Point x: 0 y: 0).
	pen goto: (Point x: 4 y: 4).
	self assert: f asAsciiString equals: '#....
.#...
..#..
...#.
....#
'!
testCanvasFramesARectangle
	| f c |
	f := Form width: 6 height: 5.
	c := Canvas on: f.
	c frameRectangle: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 6 y: 5)) value: 1.
	self assert: f asAsciiString equals: '######
#....#
#....#
#....#
######
'! !
