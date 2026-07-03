"UI-Graphics L2 (UI.md §5.5): a Smalltalk-80 style strike font. All glyphs live
 side-by-side in one Form (`glyphs`); glyph g spans strike columns
 (xTable at: g+1) up to (xTable at: g+2), so proportional and fixed fonts share
 one code path (a fixed font just has a uniform xTable). The default font is
 the proportional Adobe Helvetica 12 in DefaultFont.st (gen_font_bdf.py); the
 original fixed 8x8 lives on as `classic` in ClassicFont.st (gen_font.py) so
 pixel-golden tests stay font-independent."

Object subclass: #StrikeFont
	instanceVariableNames: 'glyphs width height first count xTable'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Graphics'!

!StrikeFont class methodsFor: 'instances'!
default
	"the baked-in proportional font, built once and cached in a global."
	DefaultStrikeFont isNil ifTrue: [DefaultStrikeFont := self buildDefault].
	^DefaultStrikeFont!
buildDefault
	| f strike bits |
	bits := self defaultGlyphBits.
	strike := Form width: self defaultStrikeWidth height: self defaultHeight.
	1 to: bits size do: [:i | strike bits at: i put: (bits at: i)].
	f := self new.
	f setGlyphs: strike height: self defaultHeight
		first: self defaultFirst count: self defaultCount xTable: self defaultXTable.
	^f!
classic
	"the original fixed 8x8 all-caps font, for font-independent golden tests."
	ClassicStrikeFont isNil ifTrue: [ClassicStrikeFont := self buildClassic].
	^ClassicStrikeFont!
buildClassic
	| f strike bits |
	bits := self classicGlyphBits.
	strike := Form width: self classicWidth * self classicCount height: self classicHeight.
	1 to: bits size do: [:i | strike bits at: i put: (bits at: i)].
	f := self new.
	f setGlyphs: strike width: self classicWidth height: self classicHeight
		first: self classicFirst count: self classicCount.
	^f!
useClassic
	"pin the classic fixed font as the default — golden tests call this first
	 so their pixel and coordinate assertions do not depend on the default
	 font's metrics."
	DefaultStrikeFont := self classic! !

!StrikeFont methodsFor: 'accessing'!
setGlyphs: aForm width: w height: h first: fst count: c
	"a fixed-width strike: glyphs w apart — expressed as a uniform xTable."
	| xt |
	xt := Array new: c + 1.
	1 to: c + 1 do: [:i | xt at: i put: (i - 1) * w].
	self setGlyphs: aForm height: h first: fst count: c xTable: xt.
	width := w!
setGlyphs: aForm height: h first: fst count: c xTable: anArray
	glyphs := aForm.
	height := h.
	first := fst.
	count := c.
	xTable := anArray.
	"widest advance; fixed-width setup overwrites this with the cell width."
	width := 0.
	1 to: c do: [:i |
		width := width max: (anArray at: i + 1) - (anArray at: i)]!
glyphs
	^glyphs!
width
	^width!
height
	^height!
widthOfCharacter: code
	"the advance of a character code; unknown codes take the space advance."
	| idx |
	idx := code - first.
	(idx < 0 or: [idx >= count]) ifTrue: [idx := 0].
	^(xTable at: idx + 2) - (xTable at: idx + 1)!
widthOfString: aString
	| w |
	w := 0.
	1 to: aString size do: [:i | w := w + (self widthOfCharacter: (aString at: i))].
	^w! !

!StrikeFont methodsFor: 'displaying'!
displayString: aString on: aForm at: aPoint
	"paint text over the background (rule 7 = or)."
	^self displayString: aString on: aForm at: aPoint rule: 7!
displayString: aString on: aForm at: aPoint rule: aRule
	"blit each glyph left-to-right with aRule; chars outside the font's range
	 leave a space-sized gap. `aString at: i` answers the byte/code at the VM
	 level. Rule 6 (xor) over a black bar gives reverse-video (white glyphs),
	 used for selection."
	| x code idx gl gw bb |
	x := aPoint x.
	1 to: aString size do: [:i |
		code := aString at: i.
		idx := code - first.
		(idx >= 0 and: [idx < count])
			ifTrue: [
				gl := xTable at: idx + 1.
				gw := (xTable at: idx + 2) - gl.
				bb := BitBlt new.
				bb destForm: aForm.
				bb sourceForm: glyphs.
				bb combinationRule: aRule.
				bb destX: x.
				bb destY: aPoint y.
				bb width: gw.
				bb height: height.
				bb sourceX: gl.
				bb sourceY: 0.
				bb clipX: 0.
				bb clipY: 0.
				bb clipWidth: aForm width.
				bb clipHeight: aForm height.
				bb copyBits.
				x := x + gw]
			ifFalse: [x := x + (self widthOfCharacter: code)]].
	^self! !
