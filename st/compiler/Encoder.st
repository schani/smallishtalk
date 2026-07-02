"Encoder (SPEC §6, A.2): mnemonic instruction tuples → 32-bit words.
 ABC: [opcode:8 | A:8 | B:8 | C:8]; AD: [opcode:8 | A:8 | D:16], D
 unsigned or signed (jumps, LOADINT). Little-endian byte emission."

Object subclass: #StEncoder
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

!StEncoder class methodsFor: 'encoding'!
opcodeFor: mnemonic
	mnemonic = 'NOP' ifTrue: [^Treaty opNop].
	mnemonic = 'BREAK' ifTrue: [^Treaty opBreak].
	mnemonic = 'MOVE' ifTrue: [^Treaty opMove].
	mnemonic = 'LOADK' ifTrue: [^Treaty opLoadk].
	mnemonic = 'LOADINT' ifTrue: [^Treaty opLoadint].
	mnemonic = 'LOADNIL' ifTrue: [^Treaty opLoadnil].
	mnemonic = 'LOADTRUE' ifTrue: [^Treaty opLoadtrue].
	mnemonic = 'LOADFALSE' ifTrue: [^Treaty opLoadfalse].
	mnemonic = 'LOADSELF' ifTrue: [^Treaty opLoadself].
	mnemonic = 'GETIVAR' ifTrue: [^Treaty opGetivar].
	mnemonic = 'SETIVAR' ifTrue: [^Treaty opSetivar].
	mnemonic = 'GETBOX' ifTrue: [^Treaty opGetbox].
	mnemonic = 'SETBOX' ifTrue: [^Treaty opSetbox].
	mnemonic = 'MKBOX' ifTrue: [^Treaty opMkbox].
	mnemonic = 'SEND' ifTrue: [^Treaty opSend].
	mnemonic = 'SENDSUPER' ifTrue: [^Treaty opSendsuper].
	mnemonic = 'RET' ifTrue: [^Treaty opRet].
	mnemonic = 'RETSELF' ifTrue: [^Treaty opRetself].
	mnemonic = 'NLR' ifTrue: [^Treaty opNlr].
	mnemonic = 'PRIM' ifTrue: [^Treaty opPrim].
	mnemonic = 'MKCLOSURE' ifTrue: [^Treaty opMkclosure].
	mnemonic = 'CAPTURE' ifTrue: [^Treaty opCapture].
	mnemonic = 'JUMP' ifTrue: [^Treaty opJump].
	mnemonic = 'JUMPTRUE' ifTrue: [^Treaty opJumptrue].
	mnemonic = 'JUMPFALSE' ifTrue: [^Treaty opJumpfalse].
	mnemonic = 'ADD' ifTrue: [^Treaty opAdd].
	mnemonic = 'SUB' ifTrue: [^Treaty opSub].
	mnemonic = 'MUL' ifTrue: [^Treaty opMul].
	mnemonic = 'DIV' ifTrue: [^Treaty opDiv].
	mnemonic = 'MOD' ifTrue: [^Treaty opMod].
	mnemonic = 'LT' ifTrue: [^Treaty opLt].
	mnemonic = 'GT' ifTrue: [^Treaty opGt].
	mnemonic = 'LE' ifTrue: [^Treaty opLe].
	mnemonic = 'GE' ifTrue: [^Treaty opGe].
	mnemonic = 'EQNUM' ifTrue: [^Treaty opEqnum].
	mnemonic = 'AT' ifTrue: [^Treaty opAt].
	mnemonic = 'ATPUT' ifTrue: [^Treaty opAtput].
	mnemonic = 'SIZE' ifTrue: [^Treaty opSize].
	mnemonic = 'CLASSOF' ifTrue: [^Treaty opClassof].
	mnemonic = 'NOT' ifTrue: [^Treaty opNot].
	mnemonic = 'IDEQ' ifTrue: [^Treaty opIdeq].
	self error: 'unknown mnemonic ' , mnemonic!

isAdFormat: mnemonic
	"instructions whose 16-bit D field holds the second operand"
	^#('LOADK' 'LOADINT' 'PRIM' 'MKCLOSURE' 'JUMP' 'JUMPTRUE' 'JUMPFALSE')
		includes: mnemonic!

isNoOperandAd: mnemonic
	^#('RETSELF' 'NOP' 'BREAK') includes: mnemonic!

isSingleA: mnemonic
	^#('LOADNIL' 'LOADTRUE' 'LOADFALSE' 'LOADSELF' 'RET' 'NLR') includes: mnemonic!

encode: insn
	"insn: Array of mnemonic string + integer operands → u32"
	| m op |
	m := insn at: 1.
	op := self opcodeFor: m.
	(self isNoOperandAd: m) ifTrue: [^op].
	(self isSingleA: m) ifTrue: [^op + ((insn at: 2) * 256)].
	(self isAdFormat: m) ifTrue: [
		m = 'PRIM' ifTrue: [^op + (((insn at: 2) bitAnd: 16rFFFF) * 65536)].
		m = 'JUMP' ifTrue: [^op + (((insn at: 2) bitAnd: 16rFFFF) * 65536)].
		"LOADK d,k / LOADINT d,imm / MKCLOSURE d,b / JUMPTRUE a,off / JUMPFALSE a,off"
		^op + ((insn at: 2) * 256)
			+ (((insn at: 3) bitAnd: 16rFFFF) * 65536)].
	"ABC: two or three operand bytes"
	insn size = 3 ifTrue: [
		^op + ((insn at: 2) * 256) + ((insn at: 3) * 65536)].
	^op + ((insn at: 2) * 256) + ((insn at: 3) * 65536)
		+ ((insn at: 4) * 16777216)!

encodeAll: insns
	| out |
	out := OrderedCollection new.
	insns do: [:i | out add: (self encode: i)].
	^out!

bytesForAll: insns
	"little-endian u32 stream, as an OrderedCollection of byte values"
	| out |
	out := OrderedCollection new.
	insns do: [:i |
		| w |
		w := self encode: i.
		out add: (w bitAnd: 255).
		out add: ((w bitShift: -8) bitAnd: 255).
		out add: ((w bitShift: -16) bitAnd: 255).
		out add: ((w bitShift: -24) bitAnd: 255)].
	^out! !
