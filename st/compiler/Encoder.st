"Encoder (SPEC §6, A.2): mnemonic instruction tuples → 32-bit words.
 ABC: [opcode:8 | A:8 | B:8 | C:8]; AD: [opcode:8 | A:8 | D:16], D
 unsigned or signed (jumps, LOADINT). Little-endian byte emission."

Object subclass: #StEncoder
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

!StEncoder class methodsFor: 'encoding'!
mnemonicKey: mnemonic
	"(size, first byte, last byte) uniquely identifies every mnemonic -
	 an integer key, so dispatch is EQNUM fast-path compares instead of
	 up-to-41 String>>= sends per instruction"
	| n |
	n := mnemonic size.
	^(n * 65536) + ((mnemonic byteAt: 1) * 256) + (mnemonic byteAt: n)!

opcodeForKey: key
	key = 281925 ifTrue: [^Treaty opMove]. "MOVE"
	key = 283460 ifTrue: [^Treaty opSend]. "SEND"
	key = 217684 ifTrue: [^Treaty opRet]. "RET"
	key = 608837 ifTrue: [^Treaty opJumpfalse]. "JUMPFALSE"
	key = 477010 ifTrue: [^Treaty opGetivar]. "GETIVAR"
	key = 543814 ifTrue: [^Treaty opLoadself]. "LOADSELF"
	key = 478284 ifTrue: [^Treaty opLoadnil]. "LOADNIL"
	key = 281168 ifTrue: [^Treaty opJump]. "JUMP"
	key = 283461 ifTrue: [^Treaty opSize]. "SIZE"
	key = 478292 ifTrue: [^Treaty opLoadint]. "LOADINT"
	key = 280913 ifTrue: [^Treaty opIdeq]. "IDEQ"
	key = 147796 ifTrue: [^Treaty opAt]. "AT"
	key = 213316 ifTrue: [^Treaty opAdd]. "ADD"
	key = 150597 ifTrue: [^Treaty opLe]. "LE"
	key = 345421 ifTrue: [^Treaty opEqnum]. "EQNUM"
	key = 347211 ifTrue: [^Treaty opLoadk]. "LOADK"
	key = 543301 ifTrue: [^Treaty opJumptrue]. "JUMPTRUE"
	key = 609349 ifTrue: [^Treaty opLoadfalse]. "LOADFALSE"
	key = 543813 ifTrue: [^Treaty opLoadtrue]. "LOADTRUE"
	key = 475974 ifTrue: [^Treaty opClassof]. "CLASSOF"
	key = 344404 ifTrue: [^Treaty opAtput]. "ATPUT"
	key = 411480 ifTrue: [^Treaty opGetbox]. "GETBOX"
	key = 414552 ifTrue: [^Treaty opSetbox]. "SETBOX"
	key = 480082 ifTrue: [^Treaty opSetivar]. "SETIVAR"
	key = 216660 ifTrue: [^Treaty opNot]. "NOT"
	key = 479814 ifTrue: [^Treaty opRetself]. "RETSELF"
	key = 149317 ifTrue: [^Treaty opGe]. "GE"
	key = 149332 ifTrue: [^Treaty opGt]. "GT"
	key = 150612 ifTrue: [^Treaty opLt]. "LT"
	key = 217922 ifTrue: [^Treaty opSub]. "SUB"
	key = 216396 ifTrue: [^Treaty opMul]. "MUL"
	key = 214102 ifTrue: [^Treaty opDiv]. "DIV"
	key = 216388 ifTrue: [^Treaty opMod]. "MOD"
	key = 347480 ifTrue: [^Treaty opMkbox]. "MKBOX"
	key = 475973 ifTrue: [^Treaty opCapture]. "CAPTURE"
	key = 609605 ifTrue: [^Treaty opMkclosure]. "MKCLOSURE"
	key = 611154 ifTrue: [^Treaty opSendsuper]. "SENDSUPER"
	key = 216658 ifTrue: [^Treaty opNlr]. "NLR"
	key = 282701 ifTrue: [^Treaty opPrim]. "PRIM"
	key = 216656 ifTrue: [^Treaty opNop]. "NOP"
	key = 344651 ifTrue: [^Treaty opBreak]. "BREAK"
	^nil!

opcodeFor: mnemonic
	| op |
	op := self opcodeForKey: (self mnemonicKey: mnemonic).
	op isNil ifTrue: [self error: 'unknown mnemonic ' , mnemonic].
	^op!

isAdFormat: mnemonic
	"instructions whose 16-bit D field holds the second operand"
	^#('LOADK' 'LOADINT' 'PRIM' 'MKCLOSURE' 'JUMP' 'JUMPTRUE' 'JUMPFALSE')
		includes: mnemonic!

isNoOperandAd: mnemonic
	^#('RETSELF' 'NOP' 'BREAK') includes: mnemonic!

isSingleA: mnemonic
	^#('LOADNIL' 'LOADTRUE' 'LOADFALSE' 'LOADSELF' 'RET' 'NLR') includes: mnemonic!

encode: insn
	"insn: Array of mnemonic string + integer operands → u32. Format
	 classification is by mnemonic key (integer compares)."
	| key op |
	key := self mnemonicKey: (insn at: 1).
	op := self opcodeForKey: key.
	op isNil ifTrue: [self error: 'unknown mnemonic ' , (insn at: 1)].
	(key = 479814 or: [key = 216656 or: [key = 344651]]) "RETSELF NOP BREAK"
		ifTrue: [^op].
	(key = 478284 or: [key = 543813 or: [key = 609349
		or: [key = 543814 or: [key = 217684 or: [key = 216658]]]]])
		"LOADNIL LOADTRUE LOADFALSE LOADSELF RET NLR"
		ifTrue: [^op + ((insn at: 2) * 256)].
	(key = 282701 or: [key = 281168]) "PRIM JUMP"
		ifTrue: [^op + (((insn at: 2) bitAnd: 16rFFFF) * 65536)].
	(key = 347211 or: [key = 478292 or: [key = 609605
		or: [key = 543301 or: [key = 608837]]]])
		"LOADK LOADINT MKCLOSURE JUMPTRUE JUMPFALSE"
		ifTrue: [^op + ((insn at: 2) * 256)
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
