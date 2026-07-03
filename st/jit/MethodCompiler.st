"The method compiler (JIT.md §13): two passes over the u32 instructions,
 arch-independent — it speaks only the macro-assembler's coarse ops.

 Pass 1 collects jump targets and send continuations. Pass 2 emits one
 template per instruction, binding a label at every bytecode pc, then the
 shared epilogue, the cold section, and the re-entry thunks.

 M2 scope: straight-line templates, specialized sends, jumps, returns.
 Everything else (SEND, MKCLOSURE, PRIM, NLR, DIV, MOD ...) compiles to
 an exit-to-interpreter at its own pc — always correct (J4: pcs are
 bytecode offsets and all values live in frame slots), so the compiler is
 total from M2 on and native coverage widens per milestone.

 Metadata (Annex J.4): re-entry map entries at every backward-jump target
 (thunks that re-derive FP and join the loop head); returnPoints entries
 at every send continuation (so a method whose sends run interpreted
 still resumes natively after each one); no patch sites yet (M3).

 Determinism (§14): compilation reads only the method and Treaty
 constants; same method -> same bytes."

Object subclass: #StJITMethodCompiler
	instanceVariableNames: 'method masm words nInsns pcLabels backTargets sendConts specConts frameSlots argc hasPrim primNumber patches lastClosureDest profilingLevel'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-JIT'!

!StJITMethodCompiler class methodsFor: 'compiling'!
compile: aMethod
	"answer (Array with: codeBytes with: mapsBytes), or nil when the
	 method is not compilable"
	^self new compileMethod: aMethod level: 0!

compile: aMethod level: aLevel
	"aLevel = the profiling level compiled in (JIT.md §16): >= 1 adds
	 inline-cache hit counting to send templates"
	^self new compileMethod: aMethod level: aLevel! !

!StJITMethodCompiler methodsFor: 'access'!
methodHeaderBits
	^method instVarAt: Treaty methodHeader + 1!

bytecodeWords
	"the u32 instruction stream, little-endian"
	| bc n out w |
	bc := method instVarAt: Treaty methodBytecodes + 1.
	n := bc size // 4.
	out := Array new: n.
	1 to: n do: [:i |
		w := (bc at: 4 * i - 3)
			+ ((bc at: 4 * i - 2) bitShift: 8)
			+ ((bc at: 4 * i - 1) bitShift: 16)
			+ ((bc at: 4 * i) bitShift: 24).
		out at: i put: w].
	^out!

siteCount
	| sites |
	sites := method instVarAt: Treaty methodSendSites + 1.
	sites isNil ifTrue: [^0].
	^sites size // Treaty siteStride! !

!StJITMethodCompiler methodsFor: 'compiling'!
compileMethod: aMethod level: aLevel
	| header primN |
	method := aMethod.
	profilingLevel := aLevel.
	header := self methodHeaderBits.
	hasPrim := ((header bitShift: Treaty mhHasPrimitiveShift negated) bitAnd: 1) = 1.
	hasPrim ifTrue: [
		"primitives that cannot run in the framed convention are
		 do-not-compile (JIT.md §3; see jit_patch_send_site's twin list)"
		primN := (header bitShift: Treaty mhPrimitiveShift negated)
			bitAnd: (1 bitShift: Treaty mhPrimitiveBits) - 1.
		primNumber := primN.
		(primN = Treaty primPerformWithArgs or: [primN = Treaty primSnapshot])
			ifTrue: [^nil]].
	frameSlots := header bitAnd: 255.
	argc := (header bitShift: Treaty mhArgcShift negated) bitAnd: 15.
	words := self bytecodeWords.
	nInsns := words size.
	nInsns = 0 ifTrue: [^nil].
	nInsns > 65535 ifTrue: [^nil].
	masm := StAMD64MacroAssembler new.
	patches := OrderedCollection new.
	lastClosureDest := nil.
	self scanTargets.
	self emitBody.
	masm genEpilogue.
	masm flushCold.
	^self finishWithMaps!

scanTargets
	"pass 1: backward-jump targets (re-entry points) and send
	 continuations (site -> pc after the send)"
	| pc w op off target |
	backTargets := OrderedCollection new.
	sendConts := OrderedCollection new.
	specConts := OrderedCollection new.
	pc := 0.
	[pc < nInsns] whileTrue: [
		w := words at: pc + 1.
		op := w bitAnd: 255.
		((op = Treaty opJump or: [op = Treaty opJumptrue])
			or: [op = Treaty opJumpfalse]) ifTrue: [
			off := (w bitShift: -16) bitAnd: 16rFFFF.
			off >= 32768 ifTrue: [off := off - 65536].
			target := pc + 1 + off.
			(off < 0 and: [(backTargets includes: target) not])
				ifTrue: [backTargets add: target]].
		(op = Treaty opSend or: [op = Treaty opSendsuper]) ifTrue: [
			sendConts add: (Array with: ((w bitShift: -24) bitAnd: 255)
				with: pc + 1)].
		"specialized sends stage siteless sends on their slow paths; map
		 their continuations so the return climbs back into native code"
		(self isSpecializedOp: op) ifTrue: [
			(pc + 1 < nInsns and: [(specConts includes: pc + 1) not])
				ifTrue: [specConts add: pc + 1]].
		pc := pc + 1].
	backTargets := self sortedInts: backTargets!

isSpecializedOp: op
	((op >= Treaty opAdd and: [op <= Treaty opEqnum])
		or: [op = Treaty opAt or: [op = Treaty opAtput
		or: [op = Treaty opSize or: [op = Treaty opNot]]]])
		ifTrue: [^true].
	^false!

sortedInts: aCollection
	"ascending insertion sort (the kernel has no SortedCollection; the
	 re-entry map must be strictly sorted by pc)"
	| out tmp inserted |
	out := OrderedCollection new.
	aCollection do: [:v |
		tmp := OrderedCollection new.
		inserted := false.
		out do: [:x |
			(inserted not and: [v < x]) ifTrue: [tmp add: v. inserted := true].
			tmp add: x].
		inserted ifFalse: [tmp add: v].
		out := tmp].
	^out!

emitBody
	| pc w op |
	pcLabels := Array new: nInsns.
	1 to: nInsns do: [:i | pcLabels at: i put: masm newLabel].
	masm genPrologueFrameSlots: frameSlots argc: argc.
	pc := 0.
	[pc < nInsns] whileTrue: [
		masm bind: (pcLabels at: pc + 1).
		w := words at: pc + 1.
		op := w bitAnd: 255.
		self emitInsn: w pc: pc.
		"mirror the interpreter's closure_reg discipline"
		(op = Treaty opMkclosure or: [op = Treaty opCapture])
			ifFalse: [lastClosureDest := nil].
		pc := pc + 1]!

emitInsn: w pc: pc
	| op a b c d16 |
	op := w bitAnd: 255.
	a := (w bitShift: -8) bitAnd: 255.
	b := (w bitShift: -16) bitAnd: 255.
	c := (w bitShift: -24) bitAnd: 255.
	d16 := (w bitShift: -16) bitAnd: 16rFFFF.

	op = Treaty opNop ifTrue: [^self].
	op = Treaty opMove ifTrue: [^masm genMove: a from: b].
	op = Treaty opLoadk ifTrue: [^masm genLoadK: a index: d16].
	op = Treaty opLoadint ifTrue: [
		| v |
		v := d16.
		v >= 32768 ifTrue: [v := v - 65536].
		^masm genLoadInt: a value: v].
	op = Treaty opLoadnil ifTrue: [^masm genLoadNil: a].
	op = Treaty opLoadtrue ifTrue: [^masm genLoadTrue: a].
	op = Treaty opLoadfalse ifTrue: [^masm genLoadFalse: a].
	op = Treaty opLoadself ifTrue: [^masm genLoadSelf: a].
	op = Treaty opGetivar ifTrue: [^masm genGetIvar: a index: b].
	op = Treaty opSetivar ifTrue: [^masm genSetIvar: a from: b].
	op = Treaty opGetbox ifTrue: [^masm genGetBox: a from: b].
	op = Treaty opSetbox ifTrue: [^masm genSetBox: a from: b].
	op = Treaty opMkbox ifTrue: [^masm genMkBox: a from: b].
	op = Treaty opRet ifTrue: [^masm genReturnSlot: a].
	op = Treaty opRetself ifTrue: [^masm genReturnSlot: 0].
	op = Treaty opAdd ifTrue: [^masm genAdd: a a: b b: c pc: pc].
	op = Treaty opSub ifTrue: [^masm genSub: a a: b b: c pc: pc].
	op = Treaty opMul ifTrue: [^masm genMul: a a: b b: c pc: pc].
	op = Treaty opLt ifTrue: [^masm genCmp: #l d: a a: b b: c pc: pc].
	op = Treaty opGt ifTrue: [^masm genCmp: #g d: a a: b b: c pc: pc].
	op = Treaty opLe ifTrue: [^masm genCmp: #le d: a a: b b: c pc: pc].
	op = Treaty opGe ifTrue: [^masm genCmp: #ge d: a a: b b: c pc: pc].
	op = Treaty opEqnum ifTrue: [^masm genCmp: #e d: a a: b b: c pc: pc].
	op = Treaty opDiv ifTrue: [^masm genDivMod: a a: b b: c pc: pc isDiv: true].
	op = Treaty opMod ifTrue: [^masm genDivMod: a a: b b: c pc: pc isDiv: false].
	op = Treaty opIdeq ifTrue: [^masm genIdEq: a a: b b: c].
	op = Treaty opNot ifTrue: [^masm genNot: a from: b pc: pc].
	op = Treaty opClassof ifTrue: [^masm genClassOf: a from: b].
	op = Treaty opSize ifTrue: [^masm genSize: a from: b pc: pc].
	op = Treaty opAt ifTrue: [^masm genAt: a obj: b idx: c pc: pc].
	op = Treaty opAtput ifTrue: [^masm genAtPut: a obj: b idx: c pc: pc].
	op = Treaty opJump ifTrue: [
		| off target |
		off := d16.
		off >= 32768 ifTrue: [off := off - 65536].
		target := pc + 1 + off.
		^masm genJumpTo: (pcLabels at: target + 1)
			backward: off < 0 targetPc: target].
	(op = Treaty opJumptrue or: [op = Treaty opJumpfalse]) ifTrue: [
		| off target |
		off := d16.
		off >= 32768 ifTrue: [off := off - 65536].
		target := pc + 1 + off.
		^masm genJumpTrue: a to: (pcLabels at: target + 1)
			backward: off < 0 targetPc: target pc: pc
			ifTrue: op = Treaty opJumptrue].
	(op = Treaty opSend or: [op = Treaty opSendsuper]) ifTrue: [
		| offs |
		offs := masm genSend: a recv: b site: c pcAfter: pc + 1
			countHits: profilingLevel >= 1.
		patches add: (Array with: (offs at: 1) with: Treaty jitPatchClass with: c).
		patches add: (Array with: (offs at: 2) with: Treaty jitPatchTarget with: c).
		^self].
	(op = Treaty opPrim and: [pc = 0 and: [hasPrim]]) ifTrue: [
		^masm genPrimCall: primNumber].
	op = Treaty opNlr ifTrue: [^masm genNlr: a pc: pc].
	op = Treaty opMkclosure ifTrue: [
		| lits blk info ncap nlrFlag |
		lits := method instVarAt: Treaty methodLiterals + 1.
		blk := (lits isNil or: [d16 + 1 > lits size])
			ifTrue: [nil] ifFalse: [lits at: d16 + 1].
		blk isNil ifTrue: [
			lastClosureDest := nil.
			^masm exitToInterpreterAt: pc].
		info := blk instVarAt: Treaty blockInfo + 1.
		ncap := info bitAnd: 255.
		nlrFlag := ((info bitShift: Treaty biHasNlrShift negated) bitAnd: 1) = 1.
		masm genMkClosure: a litIndex: d16 ncap: ncap hasNlr: nlrFlag.
		lastClosureDest := a.
		^self].
	op = Treaty opCapture ifTrue: [
		lastClosureDest isNil ifTrue: [^masm exitToInterpreterAt: pc].
		^masm genCapture: a from: b closureSlot: lastClosureDest].
	"everything else (BREAK, misplaced PRIM, unknown) runs interpreted
	 from here (re-execution fallback)"
	masm exitToInterpreterAt: pc!

finishWithMaps
	"emit one shared re-entry thunk per resumable pc — backward-jump
	 targets, send continuations, and the post-PRIM fallback — then
	 serialize Annex J.4. returnPoints share the continuation thunks."
	| reentryPcs thunkOffs retpts bytes maps nsites idx |
	reentryPcs := OrderedCollection new.
	backTargets do: [:p |
		(reentryPcs includes: p) ifFalse: [reentryPcs add: p]].
	sendConts do: [:pair |
		| p |
		p := pair at: 2.
		(p < nInsns and: [(reentryPcs includes: p) not])
			ifTrue: [reentryPcs add: p]].
	specConts do: [:p |
		(reentryPcs includes: p) ifFalse: [reentryPcs add: p]].
	(hasPrim and: [nInsns > 1 and: [(reentryPcs includes: 1) not]])
		ifTrue: [reentryPcs add: 1].
	reentryPcs := self sortedInts: reentryPcs.
	thunkOffs := OrderedCollection new.
	reentryPcs do: [:p |
		thunkOffs add: masm here.
		masm deriveFP.
		masm asm jmpTo: (pcLabels at: p + 1)].
	nsites := self siteCount.
	retpts := Array new: nsites.
	1 to: nsites do: [:i | retpts at: i put: Treaty jitRetpointNone].
	sendConts do: [:pair |
		| site p |
		site := pair at: 1.
		p := pair at: 2.
		p < nInsns ifTrue: [
			idx := 1.
			[(reentryPcs at: idx) = p] whileFalse: [idx := idx + 1].
			retpts at: site + 1 put: (thunkOffs at: idx)]].
	bytes := masm finish.
	maps := OrderedCollection new.
	self emitU32: 0 into: maps. "entry offset"
	self emitU32: reentryPcs size into: maps.
	self emitU32: nsites into: maps.
	self emitU32: patches size into: maps.
	1 to: reentryPcs size do: [:i |
		self emitU32: (reentryPcs at: i) into: maps.
		self emitU32: (thunkOffs at: i) into: maps].
	retpts do: [:rp | self emitU32: rp into: maps].
	patches do: [:p |
		self emitU32: (p at: 1) into: maps.
		self emitU32: (p at: 2) + ((p at: 3) bitShift: 8) into: maps].
	^Array with: (self byteArrayFrom: bytes)
		with: (self byteArrayFrom: maps)!

emitU32: v into: out
	out add: (v bitAnd: 255).
	out add: ((v bitShift: -8) bitAnd: 255).
	out add: ((v bitShift: -16) bitAnd: 255).
	out add: ((v bitShift: -24) bitAnd: 255)!

byteArrayFrom: aCollection
	| ba i |
	ba := ByteArray new: aCollection size.
	i := 1.
	aCollection do: [:b |
		ba at: i put: b.
		i := i + 1].
	^ba! !
