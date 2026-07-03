"AMD64 assembler (JIT.md §13, milestone M1): one method per instruction
 form, emitting into a growing byte collection, with label objects and
 rel32 fixups. No clever encoders — exactly the subset of the ISA the
 templates use, and nothing more.

 Conventions:
 - registers are integers 0-15 (rax..r15); Treaty jitAmd64Fp etc. name the
   dedicated ones.
 - memory operands are always mod=10 ([base + disp32]) or mod=10 with SIB
   ([base + index*2^scale + disp32]) — one frozen shape per form, so the
   VM's patch routines (Annex J.5) and the disassembler stay trivial.
 - offsets (`here`, labels, patch sites) are 0-based byte offsets into the
   emitted code, matching the Annex J.4 metadata."

Object subclass: #StAMD64Assembler
	instanceVariableNames: 'bytes labels fixups'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-JIT'!

!StAMD64Assembler class methodsFor: 'instance creation'!
new
	^super new init! !

!StAMD64Assembler methodsFor: 'initialization'!
init
	bytes := OrderedCollection new.
	labels := OrderedCollection new.
	fixups := OrderedCollection new! !

!StAMD64Assembler methodsFor: 'emitting'!
here
	"0-based offset of the next byte"
	^bytes size!

emitByte: b
	bytes add: (b bitAnd: 255)!

emitU32: v
	| w |
	w := v bitAnd: 16rFFFFFFFF.
	self emitByte: w.
	self emitByte: (w bitShift: -8).
	self emitByte: (w bitShift: -16).
	self emitByte: (w bitShift: -24)!

emitU64: v
	self emitU32: v.
	self emitU32: (v bitShift: -32)!

rexW: w reg: r index: x base: b
	"emit a REX prefix when needed (always for 64-bit operations)"
	| v |
	v := 64 + (w * 8) + ((r bitShift: -3) * 4) + ((x bitShift: -3) * 2)
		+ (b bitShift: -3).
	(w = 1 or: [v > 64]) ifTrue: [self emitByte: v]!

modrm: mode reg: r rm: m
	self emitByte: (mode * 64) + ((r bitAnd: 7) * 8) + (m bitAnd: 7)!

mem: reg base: base disp: disp
	"mod=10 memory operand [base + disp32]"
	(base bitAnd: 7) = 4
		ifTrue: [
			self modrm: 2 reg: reg rm: 4.
			self emitByte: 16r24]
		ifFalse: [self modrm: 2 reg: reg rm: base].
	self emitU32: disp!

mem: reg base: base index: idx scale: scale disp: disp
	"mod=10 SIB operand [base + index*2^scale + disp32]"
	(idx bitAnd: 7) = 4 ifTrue: [self error: 'rsp cannot index'].
	self modrm: 2 reg: reg rm: 4.
	self emitByte: (scale * 64) + ((idx bitAnd: 7) * 8) + (base bitAnd: 7).
	self emitU32: disp! !

!StAMD64Assembler methodsFor: 'labels'!
newLabel
	labels add: nil.
	^labels size!

bind: label
	(labels at: label) isNil ifFalse: [self error: 'label bound twice'].
	labels at: label put: self here!

emitRel32To: label
	fixups add: (Array with: self here with: label).
	self emitU32: 0!

finish
	"resolve fixups; answer the bytes (an OrderedCollection of 0-255)"
	fixups do: [:f |
		| pos target rel |
		pos := f at: 1.
		target := labels at: (f at: 2).
		target isNil ifTrue: [self error: 'unbound label'].
		rel := (target - (pos + 4)) bitAnd: 16rFFFFFFFF.
		bytes at: pos + 1 put: (rel bitAnd: 255).
		bytes at: pos + 2 put: ((rel bitShift: -8) bitAnd: 255).
		bytes at: pos + 3 put: ((rel bitShift: -16) bitAnd: 255).
		bytes at: pos + 4 put: ((rel bitShift: -24) bitAnd: 255)].
	fixups := OrderedCollection new.
	^bytes! !

!StAMD64Assembler methodsFor: 'moves'!
movR: dst fromR: src
	self rexW: 1 reg: src index: 0 base: dst.
	self emitByte: 16r89.
	self modrm: 3 reg: src rm: dst!

movR: dst fromM: base disp: disp
	self rexW: 1 reg: dst index: 0 base: base.
	self emitByte: 16r8B.
	self mem: dst base: base disp: disp!

movM: base disp: disp fromR: src
	self rexW: 1 reg: src index: 0 base: base.
	self emitByte: 16r89.
	self mem: src base: base disp: disp!

movR: dst fromM: base index: idx scale: scale disp: disp
	self rexW: 1 reg: dst index: idx base: base.
	self emitByte: 16r8B.
	self mem: dst base: base index: idx scale: scale disp: disp!

movM: base index: idx scale: scale disp: disp fromR: src
	self rexW: 1 reg: src index: idx base: base.
	self emitByte: 16r89.
	self mem: src base: base index: idx scale: scale disp: disp!

mov32R: dst fromM: base index: idx scale: scale disp: disp
	"32-bit load, zero-extending (returnPoints entries)"
	self rexW: 0 reg: dst index: idx base: base.
	self emitByte: 16r8B.
	self mem: dst base: base index: idx scale: scale disp: disp!

movR: r imm32: imm
	"32-bit immediate, zero-extended"
	r >= 8 ifTrue: [self emitByte: 16r41].
	self emitByte: 16rB8 + (r bitAnd: 7).
	self emitU32: imm!

movR: r imm64: imm
	self rexW: 1 reg: 0 index: 0 base: r.
	self emitByte: 16rB8 + (r bitAnd: 7).
	self emitU64: imm!

movM: base disp: disp imm32: imm
	"store a sign-extended 32-bit immediate to a 64-bit slot"
	self rexW: 1 reg: 0 index: 0 base: base.
	self emitByte: 16rC7.
	self mem: 0 base: base disp: disp.
	self emitU32: imm!

leaR: dst base: base index: idx scale: scale disp: disp
	self rexW: 1 reg: dst index: idx base: base.
	self emitByte: 16r8D.
	self mem: dst base: base index: idx scale: scale disp: disp!

leaR: dst base: base disp: disp
	self rexW: 1 reg: dst index: 0 base: base.
	self emitByte: 16r8D.
	self mem: dst base: base disp: disp! !

!StAMD64Assembler methodsFor: 'arithmetic'!
aluOp: opByte r: dst fromR: src
	self rexW: 1 reg: src index: 0 base: dst.
	self emitByte: opByte.
	self modrm: 3 reg: src rm: dst!

addR: dst fromR: src
	self aluOp: 16r01 r: dst fromR: src!

subR: dst fromR: src
	self aluOp: 16r29 r: dst fromR: src!

orR: dst fromR: src
	self aluOp: 16r09 r: dst fromR: src!

xorR: dst fromR: src
	self aluOp: 16r31 r: dst fromR: src!

andR: dst fromR: src
	self aluOp: 16r21 r: dst fromR: src!

cmpR: a fromR: b
	self aluOp: 16r39 r: a fromR: b!

cmpR: r fromM: base disp: disp
	"cmp r, [base+disp]"
	self rexW: 1 reg: r index: 0 base: base.
	self emitByte: 16r3B.
	self mem: r base: base disp: disp!

cmp32R: a fromR: b
	self rexW: 0 reg: b index: 0 base: a.
	self emitByte: 16r39.
	self modrm: 3 reg: b rm: a!

imulR: dst fromR: src
	"imul dst, src (0F AF: reg field is the destination)"
	self rexW: 1 reg: dst index: 0 base: src.
	self emitByte: 16r0F.
	self emitByte: 16rAF.
	self modrm: 3 reg: dst rm: src!

group1: ext r: r imm32: imm
	self rexW: 1 reg: 0 index: 0 base: r.
	self emitByte: 16r81.
	self modrm: 3 reg: ext rm: r.
	self emitU32: imm!

addR: r imm32: imm
	self group1: 0 r: r imm32: imm!

andR: r imm32: imm
	self group1: 4 r: r imm32: imm!

subR: r imm32: imm
	self group1: 5 r: r imm32: imm!

cmpR: r imm32: imm
	self group1: 7 r: r imm32: imm!

addR: r imm8: imm
	"sign-extended 8-bit immediate"
	self rexW: 1 reg: 0 index: 0 base: r.
	self emitByte: 16r83.
	self modrm: 3 reg: 0 rm: r.
	self emitByte: imm!

cmp32R: r imm32: imm
	"32-bit compare with imm32 — THE patchable class-compare shape
	 (Annex J.5). Answers the 0-based offset of the imm32 field."
	| off |
	r >= 8 ifTrue: [self emitByte: 16r41].
	self emitByte: 16r81.
	self modrm: 3 reg: 7 rm: r.
	off := self here.
	self emitU32: imm.
	^off!

testR: a fromR: b
	self rexW: 1 reg: b index: 0 base: a.
	self emitByte: 16r85.
	self modrm: 3 reg: b rm: a!

testR: r imm32: imm
	self rexW: 1 reg: 0 index: 0 base: r.
	self emitByte: 16rF7.
	self modrm: 3 reg: 0 rm: r.
	self emitU32: imm!

negR: r
	self rexW: 1 reg: 0 index: 0 base: r.
	self emitByte: 16rF7.
	self modrm: 3 reg: 3 rm: r!

idivR: r
	"signed divide rdx:rax by r"
	self rexW: 1 reg: 0 index: 0 base: r.
	self emitByte: 16rF7.
	self modrm: 3 reg: 7 rm: r!

cqo
	self emitByte: 16r48.
	self emitByte: 16r99!

shift: ext r: r by: n
	self rexW: 1 reg: 0 index: 0 base: r.
	self emitByte: 16rC1.
	self modrm: 3 reg: ext rm: r.
	self emitByte: n!

shlR: r by: n
	self shift: 4 r: r by: n!

shrR: r by: n
	self shift: 5 r: r by: n!

sarR: r by: n
	self shift: 7 r: r by: n! !

!StAMD64Assembler methodsFor: 'memory compares'!
cmpM8: base disp: disp imm8: imm
	"cmp byte [base+disp], imm8 (the safepoint-flag poll)"
	self rexW: 0 reg: 0 index: 0 base: base.
	self emitByte: 16r80.
	self mem: 7 base: base disp: disp.
	self emitByte: imm! !

!StAMD64Assembler methodsFor: 'byte access'!
movzxR: dst fromM8: base index: idx disp: disp
	"movzx dst, byte [base + idx*1 + disp] (AT on byte objects)"
	self rexW: 1 reg: dst index: idx base: base.
	self emitByte: 16r0F.
	self emitByte: 16rB6.
	self mem: dst base: base index: idx scale: 0 disp: disp!

movM8: base index: idx disp: disp fromR: src
	"mov byte [base + idx*1 + disp], src8 (ATPUT on byte objects; use a
	 low source register — dl — so no REX is needed for the reg field)"
	self rexW: 0 reg: src index: idx base: base.
	self emitByte: 16r88.
	self mem: src base: base index: idx scale: 0 disp: disp! !

!StAMD64Assembler methodsFor: 'control'!
condCode: cond
	"condition symbol -> tttn code (objdump mnemonic set)"
	cond = #o ifTrue: [^0].
	cond = #no ifTrue: [^1].
	cond = #b ifTrue: [^2].
	cond = #ae ifTrue: [^3].
	cond = #e ifTrue: [^4].
	cond = #ne ifTrue: [^5].
	cond = #be ifTrue: [^6].
	cond = #a ifTrue: [^7].
	cond = #s ifTrue: [^8].
	cond = #ns ifTrue: [^9].
	cond = #l ifTrue: [^12].
	cond = #ge ifTrue: [^13].
	cond = #le ifTrue: [^14].
	cond = #g ifTrue: [^15].
	self error: 'unknown condition'!

j: cond to: label
	self emitByte: 16r0F.
	self emitByte: 16r80 + (self condCode: cond).
	self emitRel32To: label!

jmpTo: label
	self emitByte: 16rE9.
	self emitRel32To: label!

jmpPatchableTo: label
	"jmp rel32 whose field is a JIT_PATCH_TARGET site (Annex J.5).
	 Answers the 0-based offset of the rel32."
	| off |
	self emitByte: 16rE9.
	off := self here.
	self emitRel32To: label.
	^off!

jmpR: r
	r >= 8 ifTrue: [self emitByte: 16r41].
	self emitByte: 16rFF.
	self modrm: 3 reg: 4 rm: r!

jmpM: base disp: disp
	self rexW: 0 reg: 0 index: 0 base: base.
	self emitByte: 16rFF.
	self mem: 4 base: base disp: disp!

callM: base disp: disp
	self rexW: 0 reg: 0 index: 0 base: base.
	self emitByte: 16rFF.
	self mem: 2 base: base disp: disp!

int3
	self emitByte: 16rCC!

ud2
	self emitByte: 16r0F.
	self emitByte: 16r0B! !
