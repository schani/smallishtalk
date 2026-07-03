"UI-Graphics L2 (UI.md §5.3): the BitBlt setup object. The instance variable
 order is a HARD contract — primitive 332 (primBitBlt) reads these 14 slots by
 index (see the M0 seam conventions). `copyBits` runs the fast Rust primitive;
 its Smalltalk fallback `copyBitsFallback` is the correctness oracle (UI.md §12
 item 2) and the path used in any build without the primitive."

Object subclass: #BitBlt
	instanceVariableNames: 'destForm sourceForm halftoneForm combinationRule destX destY width height sourceX sourceY clipX clipY clipWidth clipHeight'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Graphics'!

!BitBlt methodsFor: 'setup'!
destForm: f
	destForm := f!
sourceForm: f
	sourceForm := f!
halftoneForm: f
	halftoneForm := f!
combinationRule: r
	combinationRule := r!
destX: n
	destX := n!
destY: n
	destY := n!
width: n
	width := n!
height: n
	height := n!
sourceX: n
	sourceX := n!
sourceY: n
	sourceY := n!
clipX: n
	clipX := n!
clipY: n
	clipY := n!
clipWidth: n
	clipWidth := n!
clipHeight: n
	clipHeight := n! !

!BitBlt methodsFor: 'blitting'!
copyBits
	"the workhorse: primitive 332, falling back to the pure-Smalltalk loop."
	<primitive: 332>
	^self copyBitsFallback!
copyBitsFallback
	"the correctness oracle for the primitive: identical semantics in pure
	 Smalltalk. Rules 0=clear, 3=store, 6=xor, 7=or. Source may be nil."
	| cx0 cy0 cx1 cy1 py px sbit dbit out |
	cx0 := clipX max: 0.
	cy0 := clipY max: 0.
	cx1 := (clipX + clipWidth) min: destForm width.
	cy1 := (clipY + clipHeight) min: destForm height.
	0 to: height - 1 do: [:j |
		py := destY + j.
		(py >= cy0 and: [py < cy1]) ifTrue: [
			0 to: width - 1 do: [:i |
				px := destX + i.
				(px >= cx0 and: [px < cx1]) ifTrue: [
					sbit := sourceForm isNil
						ifTrue: [0]
						ifFalse: [sourceForm pixelValueAtX: sourceX + i y: sourceY + j].
					dbit := destForm pixelValueAtX: px y: py.
					out := combinationRule = 0
						ifTrue: [0]
						ifFalse: [combinationRule = 3
							ifTrue: [sbit]
							ifFalse: [combinationRule = 6
								ifTrue: [sbit bitXor: dbit]
								ifFalse: [sbit bitOr: dbit]]].
					destForm pixelAtX: px y: py put: out]]]].
	^self! !
