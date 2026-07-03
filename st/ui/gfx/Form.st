"UI-Graphics L2 (UI.md §5.2): a monochrome bitmap. The instance variable
 order `width height depth bits` is a HARD contract — the Rust seam primitives
 (331 primPixelBlit, 333 primSaveForm) read these slots by index (see the
 M0 seam conventions). `bits` is a ByteArray, MSB-first packed, stride bytes
 per row."

Object subclass: #Form
	instanceVariableNames: 'width height depth bits'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Graphics'!

!Form class methodsFor: 'creation'!
width: w height: h
	| f |
	f := self new.
	f setWidth: w height: h.
	^f!
extent: aPoint
	^self width: aPoint x height: aPoint y! !

!Form methodsFor: 'initializing'!
setWidth: w height: h
	width := w.
	height := h.
	depth := 1.
	bits := ByteArray new: ((w + 7) // 8) * h! !

!Form methodsFor: 'accessing'!
width
	^width!
height
	^height!
depth
	^depth!
bits
	^bits!
stride
	^(width + 7) // 8!
extent
	^Point x: width y: height!
boundingBox
	^Rectangle origin: (Point x: 0 y: 0) corner: (Point x: width y: height)! !

!Form methodsFor: 'pixels'!
pixelValueAtX: px y: py
	"0 or 1; out-of-bounds reads as 0 (transparent)."
	| byte |
	(px < 0 or: [py < 0 or: [px >= width or: [py >= height]]]) ifTrue: [^0].
	byte := bits at: py * self stride + (px // 8) + 1.
	^(byte bitShift: (px \\ 8) - 7) bitAnd: 1!
pixelAtX: px y: py put: v
	"set (v ~= 0) or clear (v = 0) one pixel; out-of-bounds is a no-op."
	| idx byte mask |
	(px < 0 or: [py < 0 or: [px >= width or: [py >= height]]]) ifTrue: [^self].
	idx := py * self stride + (px // 8) + 1.
	byte := bits at: idx.
	mask := 1 bitShift: 7 - (px \\ 8).
	v = 0
		ifTrue: [byte := byte bitAnd: (255 bitXor: mask)]
		ifFalse: [byte := byte bitOr: mask].
	bits at: idx put: byte.
	^self! !

!Form methodsFor: 'drawing'!
fill: aValue
	"value 0 clears through the BitBlt primitive (a whole-screen clear runs on
	 every repaint); nonzero keeps the byte loop — it is rare, and it is what
	 builds the solid source form the value-1 blits read from."
	| bb |
	aValue = 0 ifTrue: [
		bb := BitBlt new.
		bb destForm: self.
		bb sourceForm: nil.
		bb combinationRule: 0.
		bb destX: 0.
		bb destY: 0.
		bb width: width.
		bb height: height.
		bb sourceX: 0.
		bb sourceY: 0.
		bb clipX: 0.
		bb clipY: 0.
		bb clipWidth: width.
		bb clipHeight: height.
		bb copyBits.
		^self].
	1 to: bits size do: [:i | bits at: i put: 255].
	^self!
copyFrom: srcRect in: srcForm to: dstPoint rule: r
	"convenience: build a BitBlt of the whole srcRect and run it."
	| bb |
	bb := BitBlt new.
	bb destForm: self.
	bb sourceForm: srcForm.
	bb combinationRule: r.
	bb destX: dstPoint x.
	bb destY: dstPoint y.
	bb width: srcRect width.
	bb height: srcRect height.
	bb sourceX: srcRect origin x.
	bb sourceY: srcRect origin y.
	bb clipX: 0.
	bb clipY: 0.
	bb clipWidth: width.
	bb clipHeight: height.
	bb copyBits.
	^self! !

!Form methodsFor: 'host seam'!
flush
	"present this Form on the host Display (primPixelBlit, UI.md §4.1)."
	<primitive: 331>
	^self primitiveFailed: #flush!
saveTo: aFileName
	"write a PNG screenshot (primSaveForm:toFile:, UI.md §4.4)."
	^self primSaveForm: self toFile: aFileName!
primSaveForm: aForm toFile: aFileName
	<primitive: 333>
	^self primitiveFailed: #primSaveForm:toFile:! !

!Form methodsFor: 'printing'!
asAsciiString
	"a human-readable rendering: '#' for set, '.' for clear, one row per line.
	 This is the golden-test substrate for pixel content (UI.md §12)."
	| ws |
	ws := WriteStream on: (String new: (width + 1) * height).
	0 to: height - 1 do: [:py |
		0 to: width - 1 do: [:px |
			ws nextPut: ((self pixelValueAtX: px y: py) = 1 ifTrue: [35] ifFalse: [46])].
		ws nextPut: 10].
	^ws contents! !
