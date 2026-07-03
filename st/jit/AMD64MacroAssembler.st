"AMD64 macro-assembler (JIT.md §13): coarse operations — whole semantic
 steps, one per bytecode fast path — over StAMD64Assembler. This class is
 the portability boundary; an ARM64MacroAssembler implements the same
 protocol idiomatically (M7).

 Frozen runtime contract (Annex J.2, matching the Rust trampoline/stubs
 and the M0 blob conventions):
 - FP=rbx, LNK=r13, GBL=r14; scratch rax rcx rdx rsi rdi r8-r11.
 - FP -> frame slot 0; bytecode slot k at [FP + 32 + 8k].
 - all values live in frame slots at every call and safepoint (J2);
   scratch never carries tagged values across a stub call.
 - deriveFP re-derives FP from GBL after any allocating/exiting stub and
   at every entry/re-entry point; it leaves the untagged frame offset in
   rcx and the Process address in rax.
 - slow paths that leave the template world exit to the interpreter at a
   bytecode pc (re-execution fallback): all state is already in the frame,
   so re-executing the current instruction interpreted is always correct.

 Cold code (slow paths, thunks) is collected as blocks and flushed after
 the hot line, keeping the hot line dense (JIT.md §8)."

Object subclass: #StAMD64MacroAssembler
	instanceVariableNames: 'asm cold epilogueLabel'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-JIT'!

!StAMD64MacroAssembler class methodsFor: 'instance creation'!
new
	^super new initMacro! !

!StAMD64MacroAssembler methodsFor: 'initialization'!
initMacro
	asm := StAMD64Assembler new.
	cold := OrderedCollection new.
	epilogueLabel := asm newLabel! !

!StAMD64MacroAssembler methodsFor: 'registers'!
rax
	^0!
rcx
	^1!
rdx
	^2!
fp
	^3!
rsi
	^6!
rdi
	^7!
r8
	^8!
r9
	^9!
r10
	^10!
r11
	^11!
lnk
	^13!
gbl
	^14! !

!StAMD64MacroAssembler methodsFor: 'accessing'!
asm
	^asm!

here
	^asm here!

newLabel
	^asm newLabel!

bind: label
	asm bind: label!

epilogueLabel
	^epilogueLabel!

finish
	^asm finish! !

!StAMD64MacroAssembler methodsFor: 'pieces'!
slotDisp: k
	"bytecode slot k relative to FP"
	^32 + (8 * k)!

tagged: n
	^n * 2 + 1!

loadSlot: reg from: k
	asm movR: reg fromM: self fp disp: (self slotDisp: k)!

storeSlot: k from: reg
	asm movM: self fp disp: (self slotDisp: k) fromR: reg!

deriveFP
	"FP := stackBody + frameOffset*8; leaves rax=Process, rcx=offset"
	asm movR: self rax fromM: self gbl disp: Treaty gblActiveProcess.
	asm movR: self fp fromM: self rax disp: 8 + (8 * Treaty processStack).
	asm movR: self rcx fromM: self rax disp: 8 + (8 * Treaty processFrameOffset).
	asm sarR: self rcx by: 1.
	asm leaR: self fp base: self fp index: self rcx scale: 3 disp: 8!

coldBlock: aBlock
	cold add: aBlock!

flushCold
	"emit collected slow paths; blocks may not add further cold blocks"
	cold do: [:b | b value].
	cold := OrderedCollection new!

exitToInterpreterAt: pc
	"Process.pc := pc; DISP_EXIT — the re-execution fallback. Also the
	 M2 form of every not-yet-templated opcode (J4: the interpreter is
	 always a correct executor of any frame)."
	asm movR: self rsi fromM: self gbl disp: Treaty gblActiveProcess.
	asm movM: self rsi disp: 8 + (8 * Treaty processPc) imm32: (self tagged: pc).
	asm movR: self rax imm32: Treaty dispExit.
	asm jmpM: self lnk disp: 8 * Treaty lnkExit!

coldExitAt: pc
	"answer a label that, when jumped to, exits to the interpreter at pc"
	| l |
	l := asm newLabel.
	self coldBlock: [asm bind: l. self exitToInterpreterAt: pc].
	^l!

coldSendSlow: specsel dest: d slots: slotArray pcAfter: pcAfter
	"answer a label for a specialized send's slow path: SEND_SLOW stages
	 the corresponding real send inside the trampoline (JIT.md §7) and
	 answers where to continue"
	| l packed |
	l := asm newLabel.
	packed := d.
	1 to: slotArray size do: [:i |
		packed := packed + ((slotArray at: i) bitShift: 8 * i)].
	self coldBlock: [
		asm bind: l.
		self genVmArg.
		asm movR: self rsi imm32: specsel.
		asm movR: self rdx imm32: packed.
		asm movR: self rcx imm32: pcAfter.
		asm callM: self lnk disp: 8 * Treaty lnkSendSlow.
		self genStubTail].
	^l!

genVmArg
	"rdi := the VM pointer (stub argument 0)"
	asm movR: self rdi fromM: self gbl disp: Treaty gblVm!

genPoll: pc
	"safepoint poll (§13/§9): one flag load + byte compare on the hot
	 line; the service call, exit-on-switch, and FP re-derive are cold"
	| slow back |
	slow := asm newLabel.
	back := asm newLabel.
	asm movR: self r8 fromM: self gbl disp: Treaty gblSafepointPtr.
	asm cmpM8: self r8 disp: 0 imm8: 0.
	asm j: #ne to: slow.
	asm bind: back.
	self coldBlock: [
		| cont |
		asm bind: slow.
		self genVmArg.
		asm movR: self rsi imm32: pc.
		asm callM: self lnk disp: 8 * Treaty lnkSafepoint.
		asm testR: self rax fromR: self rax.
		"nonzero disposition: relinquish native execution"
		cont := asm newLabel.
		asm j: #e to: cont.
		asm jmpM: self lnk disp: 8 * Treaty lnkExit.
		asm bind: cont.
		self deriveFP.
		asm jmpTo: back]!

genAllocClass: classIndex format: fmt slots: n
	"call ALLOC; result (object address) in rax; on failure the error
	 disposition leaves through the exit glue. Clobbers all scratch;
	 caller must deriveFP afterwards (allocating stub, §6)."
	| ok |
	ok := asm newLabel.
	self genVmArg.
	asm movR: self rsi imm32: classIndex.
	asm movR: self rdx imm32: fmt.
	asm movR: self rcx imm32: n.
	asm callM: self lnk disp: 8 * Treaty lnkAlloc.
	asm testR: self rax fromR: self rax.
	asm j: #ne to: ok.
	asm movR: self rax imm32: Treaty dispError.
	asm jmpM: self lnk disp: 8 * Treaty lnkExit.
	asm bind: ok!

genBarrierObj: objReg value: valReg
	"the filtered write barrier (§14): call BARRIER_REMEMBER only for an
	 old object storing a young pointer. objReg must be rsi (the stub's
	 argument register); valReg must not be rsi/rdi."
	| skip callIt |
	skip := asm newLabel.
	callIt := asm newLabel.
	asm testR: valReg imm32: 7.
	asm j: #ne to: skip. "not a pointer"
	asm cmpR: valReg fromM: self gbl disp: Treaty gblYoungBase.
	asm j: #b to: skip. "value not young"
	asm cmpR: valReg fromM: self gbl disp: Treaty gblYoungLimit.
	asm j: #ae to: skip.
	asm cmpR: objReg fromM: self gbl disp: Treaty gblYoungBase.
	asm j: #b to: callIt. "object below young space: old"
	asm cmpR: objReg fromM: self gbl disp: Treaty gblYoungLimit.
	asm j: #b to: skip. "object itself young: exempt"
	asm bind: callIt.
	self genVmArg.
	asm callM: self lnk disp: 8 * Treaty lnkBarrierRemember.
	asm bind: skip! !

!StAMD64MacroAssembler methodsFor: 'prologue and epilogue'!
genPrologueFrameSlots: fs argc: argc
	"entry: derive FP, frame-limit check (STACK_GROW slow path), the
	 invocation counter (saturating; profile data per §16), nil the
	 temps, and the send-side safepoint poll at pc 0"
	| ok grow k skip mask |
	ok := asm newLabel.
	grow := asm newLabel.
	self deriveFP.
	"needed = frameOffset + 4 + fs vs the stack's slot count (stacks are
	 always >= 512 slots, so the count is always in the overflow word)"
	asm movR: self rdx fromM: self rax disp: 8 + (8 * Treaty processStack).
	asm leaR: self rsi base: self rcx disp: Treaty frameReceiver + fs.
	asm cmpR: self rsi fromM: self rdx disp: -8.
	asm j: #le to: ok.
	asm jmpTo: grow.
	self coldBlock: [
		| err |
		err := asm newLabel.
		asm bind: grow.
		self genVmArg.
		"rsi already holds the needed slot count"
		asm callM: self lnk disp: 8 * Treaty lnkStackGrow.
		asm testR: self rax fromR: self rax.
		asm j: #e to: err.
		self deriveFP.
		asm jmpTo: ok.
		asm bind: err.
		asm movR: self rax imm32: Treaty dispError.
		asm jmpM: self lnk disp: 8 * Treaty lnkExit].
	asm bind: ok.
	"invocation counter (vmState): saturate at the counter-field ceiling"
	skip := asm newLabel.
	mask := (1 bitShift: Treaty vmstateCounterBits) - 1.
	asm movR: self rdx fromM: self fp disp: 8 * Treaty frameMethod.
	asm movR: self rsi fromM: self rdx disp: 8 + (8 * Treaty methodVmstate).
	asm movR: self rdi fromR: self rsi.
	asm sarR: self rdi by: 1.
	asm andR: self rdi imm32: mask.
	asm cmpR: self rdi imm32: mask.
	asm j: #e to: skip.
	asm addR: self rsi imm8: 2.
	asm movM: self rdx disp: 8 + (8 * Treaty methodVmstate) fromR: self rsi.
	asm bind: skip.
	"nil temps and scratch (fresh native frames from native callers, M3;
	 idempotent under interpreter activation)"
	fs > (1 + argc) ifTrue: [
		asm movR: self rdx fromM: self gbl disp: Treaty gblNil.
		k := 1 + argc.
		[k < fs] whileTrue: [
			self storeSlot: k from: self rdx.
			k := k + 1]].
	self genPoll: 0!

genReturnSlot: k
	"the RET/RETSELF template: result to rdx, one shared epilogue"
	self loadSlot: self rdx from: k.
	asm jmpTo: epilogueLabel!

genEpilogue
	"the return sequence (Annex J.5): deliver rdx to the caller per
	 returnInfo; jump to the caller's compiled return point, or exit
	 (interpreted/unlinked/no-site caller), or halt (base frame)"
	| notBase exitPath |
	notBase := asm newLabel.
	exitPath := asm newLabel.
	asm bind: epilogueLabel.
	asm movR: self rax fromM: self fp disp: 8 * Treaty frameRetinfo.
	asm sarR: self rax by: 1.
	asm movR: self rcx fromM: self fp disp: 8 * Treaty frameCaller.
	asm sarR: self rcx by: 1.
	asm testR: self rcx fromR: self rcx.
	asm j: #ne to: notBase.
	asm movM: self gbl disp: Treaty gblHaltValue fromR: self rdx.
	asm movR: self rax imm32: Treaty dispHalt.
	asm jmpM: self lnk disp: 8 * Treaty lnkExit.
	asm bind: notBase.
	asm movR: self rsi fromM: self gbl disp: Treaty gblActiveProcess.
	asm movR: self rdi fromM: self rsi disp: 8 + (8 * Treaty processStack).
	asm leaR: self rdi base: self rdi index: self rcx scale: 3 disp: 8.
	asm movR: self r8 fromR: self rax.
	asm shrR: self r8 by: Treaty retinfoDestShift.
	asm andR: self r8 imm32: 255.
	asm movM: self rdi index: self r8 scale: 3 disp: 32 fromR: self rdx.
	asm leaR: self r9 base: self rcx index: self rcx scale: 0 disp: 1.
	asm movM: self rsi disp: 8 + (8 * Treaty processFrameOffset) fromR: self r9.
	asm movR: self r10 fromM: self rdi disp: 8 * Treaty frameMethod.
	asm movR: self r10 fromM: self r10 disp: 8 + (8 * Treaty methodVmstate).
	asm sarR: self r10 by: 1.
	asm shrR: self r10 by: Treaty vmstateHandleShift.
	asm testR: self r10 fromR: self r10.
	asm j: #e to: exitPath.
	asm addR: self r10 imm8: -1.
	asm shlR: self r10 by: 4.
	asm movR: self r11 fromM: self gbl disp: Treaty gblHandles.
	asm addR: self r10 fromR: self r11.
	asm movR: self r11 fromM: self r10 disp: 0.
	asm testR: self r11 fromR: self r11.
	asm j: #e to: exitPath. "unlinked"
	asm movR: self r10 fromM: self r10 disp: 8.
	asm movR: self r9 fromR: self rax.
	asm andR: self r9 imm32: 255.
	asm cmpR: self r9 imm32: Treaty retinfoNoSite.
	asm j: #e to: exitPath.
	asm mov32R: self r9 fromM: self r10 index: self r9 scale: 2 disp: 0.
	asm movR: self rax imm32: Treaty jitRetpointNone.
	asm cmp32R: self r9 fromR: self rax.
	asm j: #e to: exitPath.
	asm addR: self r11 fromR: self r9.
	asm jmpR: self r11.
	asm bind: exitPath.
	"caller has no compiled return point: store its resume pc and exit"
	asm movR: self rax fromM: self fp disp: 8 * Treaty frameRetinfo.
	asm sarR: self rax by: 1.
	asm shrR: self rax by: Treaty retinfoPcShift.
	asm leaR: self r11 base: self rax index: self rax scale: 0 disp: 1.
	asm movM: self rsi disp: 8 + (8 * Treaty processPc) fromR: self r11.
	asm movR: self rax imm32: Treaty dispExit.
	asm jmpM: self lnk disp: 8 * Treaty lnkExit! !

!StAMD64MacroAssembler methodsFor: 'templates - data'!
genMove: d from: a
	self loadSlot: self rdx from: a.
	self storeSlot: d from: self rdx!

genLoadNil: d
	asm movR: self rdx fromM: self gbl disp: Treaty gblNil.
	self storeSlot: d from: self rdx!

genLoadTrue: d
	asm movR: self rdx fromM: self gbl disp: Treaty gblTrue.
	self storeSlot: d from: self rdx!

genLoadFalse: d
	asm movR: self rdx fromM: self gbl disp: Treaty gblFalse.
	self storeSlot: d from: self rdx!

genLoadSelf: d
	self genMove: d from: 0!

genLoadInt: d value: v
	asm movM: self fp disp: (self slotDisp: d) imm32: (self tagged: v)!

genLoadK: d index: k
	asm movR: self rdx fromM: self fp disp: 8 * Treaty frameMethod.
	asm movR: self rdx fromM: self rdx disp: 8 + (8 * Treaty methodLiterals).
	asm movR: self rdx fromM: self rdx disp: 8 + (8 * k).
	self storeSlot: d from: self rdx!

genGetIvar: d index: i
	"receiver is self (slot 0); the bytecode compiler only emits ivar
	 access on classes with ivars, so no pointer guard (§19)"
	self loadSlot: self rdx from: 0.
	asm movR: self rdx fromM: self rdx disp: 8 + (8 * i).
	self storeSlot: d from: self rdx!

genSetIvar: i from: b
	self loadSlot: self rdx from: b.
	self loadSlot: self rsi from: 0.
	asm movM: self rsi disp: 8 + (8 * i) fromR: self rdx.
	self genBarrierObj: self rsi value: self rdx!

genGetBox: d from: a
	self loadSlot: self rdx from: a.
	asm movR: self rdx fromM: self rdx disp: 8.
	self storeSlot: d from: self rdx!

genSetBox: a from: b
	self loadSlot: self rdx from: b.
	self loadSlot: self rsi from: a.
	asm movM: self rsi disp: 8 fromR: self rdx.
	self genBarrierObj: self rsi value: self rdx!

genMkBox: d from: b
	"allocate the Box, then store the (re-loaded) value; a fresh young
	 box needs no barrier"
	self genAllocClass: Treaty classBox format: Treaty fmtFixed slots: 1.
	asm movR: self rdx fromR: self rax.
	self deriveFP.
	self loadSlot: self rsi from: b.
	asm movM: self rdx disp: 8 fromR: self rsi.
	self storeSlot: d from: self rdx! !

!StAMD64MacroAssembler methodsFor: 'templates - arithmetic'!
genBothIntsA: a b: b else: coldLabel
	"rdx := slot a, rsi := slot b; branch out unless both SmallIntegers"
	self loadSlot: self rdx from: a.
	self loadSlot: self rsi from: b.
	asm movR: self rdi fromR: self rdx.
	asm andR: self rdi fromR: self rsi.
	asm testR: self rdi imm32: 1.
	asm j: #e to: coldLabel!

genAdd: d a: a b: b pc: pc
	| slow |
	slow := self coldSendSlow: Treaty specselPlus dest: d
		slots: (Array with: a with: b) pcAfter: pc + 1.
	self genBothIntsA: a b: b else: slow.
	asm addR: self rdx fromR: self rsi.
	asm j: #o to: slow.
	asm addR: self rdx imm8: -1.
	self storeSlot: d from: self rdx!

genSub: d a: a b: b pc: pc
	| slow |
	slow := self coldSendSlow: Treaty specselMinus dest: d
		slots: (Array with: a with: b) pcAfter: pc + 1.
	self genBothIntsA: a b: b else: slow.
	asm subR: self rdx fromR: self rsi.
	asm j: #o to: slow.
	asm addR: self rdx imm8: 1.
	self storeSlot: d from: self rdx!

genMul: d a: a b: b pc: pc
	| slow |
	slow := self coldSendSlow: Treaty specselTimes dest: d
		slots: (Array with: a with: b) pcAfter: pc + 1.
	self genBothIntsA: a b: b else: slow.
	asm sarR: self rdx by: 1.
	asm addR: self rsi imm8: -1.
	asm imulR: self rdx fromR: self rsi.
	asm j: #o to: slow.
	asm addR: self rdx imm8: 1.
	self storeSlot: d from: self rdx!

genDivMod: d a: a b: b pc: pc isDiv: isDiv
	"floored // and \\\\ (§12): idiv plus the sign-adjust; zero divisor
	 and non-ints exit. 63-bit operands cannot overflow idiv."
	| slow done |
	slow := self coldSendSlow: (isDiv
			ifTrue: [Treaty specselIntDiv] ifFalse: [Treaty specselMod])
		dest: d slots: (Array with: a with: b) pcAfter: pc + 1.
	done := asm newLabel.
	self genBothIntsA: a b: b else: slow.
	asm movR: self r8 fromR: self rdx.
	asm movR: self r9 fromR: self rsi.
	asm cmpR: self r9 imm32: 1.
	asm j: #e to: slow. "tagged zero divisor"
	asm sarR: self r8 by: 1.
	asm sarR: self r9 by: 1.
	asm movR: self rax fromR: self r8.
	asm cqo.
	asm idivR: self r9.
	"rax = trunc quotient, rdx = trunc remainder; floored adjust when the
	 remainder is nonzero and signs differ"
	asm testR: self rdx fromR: self rdx.
	asm j: #e to: done.
	asm movR: self rdi fromR: self rdx.
	asm xorR: self rdi fromR: self r9.
	asm j: #ns to: done.
	asm addR: self rax imm8: -1.
	asm addR: self rdx fromR: self r9.
	asm bind: done.
	isDiv
		ifTrue: [asm leaR: self rdx base: self rax index: self rax scale: 0 disp: 1]
		ifFalse: [asm leaR: self rdx base: self rdx index: self rdx scale: 0 disp: 1].
	self storeSlot: d from: self rdx!

genCmp: cond d: d a: a b: b pc: pc
	"cond is the signed condition for 'a cond b' (tagging preserves
	 order and equality)"
	| slow t e specsel |
	cond = #l ifTrue: [specsel := Treaty specselLt].
	cond = #g ifTrue: [specsel := Treaty specselGt].
	cond = #le ifTrue: [specsel := Treaty specselLe].
	cond = #ge ifTrue: [specsel := Treaty specselGe].
	cond = #e ifTrue: [specsel := Treaty specselEq].
	slow := self coldSendSlow: specsel dest: d
		slots: (Array with: a with: b) pcAfter: pc + 1.
	t := asm newLabel.
	e := asm newLabel.
	self genBothIntsA: a b: b else: slow.
	asm cmpR: self rdx fromR: self rsi.
	asm j: cond to: t.
	asm movR: self rdx fromM: self gbl disp: Treaty gblFalse.
	asm jmpTo: e.
	asm bind: t.
	asm movR: self rdx fromM: self gbl disp: Treaty gblTrue.
	asm bind: e.
	self storeSlot: d from: self rdx!

genIdEq: d a: a b: b
	| t e |
	t := asm newLabel.
	e := asm newLabel.
	self loadSlot: self rdx from: a.
	self loadSlot: self rsi from: b.
	asm cmpR: self rdx fromR: self rsi.
	asm j: #e to: t.
	asm movR: self rdx fromM: self gbl disp: Treaty gblFalse.
	asm jmpTo: e.
	asm bind: t.
	asm movR: self rdx fromM: self gbl disp: Treaty gblTrue.
	asm bind: e.
	self storeSlot: d from: self rdx!

genNot: d from: b pc: pc
	| slow putF e |
	slow := self coldSendSlow: Treaty specselNot dest: d
		slots: (Array with: b) pcAfter: pc + 1.
	putF := asm newLabel.
	e := asm newLabel.
	self loadSlot: self rdx from: b.
	asm cmpR: self rdx fromM: self gbl disp: Treaty gblTrue.
	asm j: #e to: putF.
	asm cmpR: self rdx fromM: self gbl disp: Treaty gblFalse.
	asm j: #ne to: slow.
	asm movR: self rdx fromM: self gbl disp: Treaty gblTrue.
	asm jmpTo: e.
	asm bind: putF.
	asm movR: self rdx fromM: self gbl disp: Treaty gblFalse.
	asm bind: e.
	self storeSlot: d from: self rdx!

genClassOf: d from: b
	"the header-load CLASSOF (§12); tag 1 -> SmallInteger"
	| isInt got |
	isInt := asm newLabel.
	got := asm newLabel.
	self loadSlot: self rdx from: b.
	asm testR: self rdx imm32: 1.
	asm j: #ne to: isInt.
	asm movR: self rsi fromM: self rdx disp: 0.
	asm shrR: self rsi by: Treaty hdrClassShift.
	asm jmpTo: got.
	asm bind: isInt.
	asm movR: self rsi imm32: Treaty classSmallinteger.
	asm bind: got.
	asm movR: self rdi fromM: self gbl disp: Treaty gblClassTable.
	asm movR: self rdx fromM: self rdi index: self rsi scale: 3 disp: 0.
	self storeSlot: d from: self rdx! !

!StAMD64MacroAssembler methodsFor: 'templates - indexed'!
genHeaderInfoOf: objReg else: coldLabel
	"rsi := header; rdi := slot count (overflow-aware); r8 := format.
	 objReg must not be rsi/rdi/r8."
	| haveN |
	haveN := asm newLabel.
	asm movR: self rsi fromM: objReg disp: 0.
	asm movR: self rdi fromR: self rsi.
	asm shrR: self rdi by: 12.
	asm andR: self rdi imm32: 255.
	asm cmpR: self rdi imm32: 255.
	asm j: #ne to: haveN.
	asm movR: self rdi fromM: objReg disp: -8.
	asm bind: haveN.
	asm movR: self r8 fromR: self rsi.
	asm shrR: self r8 by: 8.
	asm andR: self r8 imm32: 15!

genSize: d from: b pc: pc
	| slow bytes store |
	slow := self coldSendSlow: Treaty specselSize dest: d
		slots: (Array with: b) pcAfter: pc + 1.
	bytes := asm newLabel.
	store := asm newLabel.
	self loadSlot: self rdx from: b.
	asm testR: self rdx imm32: 7.
	asm j: #ne to: slow.
	self genHeaderInfoOf: self rdx else: slow.
	asm cmpR: self r8 imm32: Treaty fmtPtrs.
	asm j: #ne to: bytes.
	asm leaR: self rdx base: self rdi index: self rdi scale: 0 disp: 1.
	asm jmpTo: store.
	asm bind: bytes.
	asm cmpR: self r8 imm32: Treaty fmtBytesBase.
	asm j: #b to: slow. "fixed format: a real send"
	asm shlR: self rdi by: 3.
	asm subR: self rdi fromR: self r8.
	asm addR: self rdi imm8: 8.
	asm leaR: self rdx base: self rdi index: self rdi scale: 0 disp: 1.
	asm bind: store.
	self storeSlot: d from: self rdx!

genAt: d obj: b idx: c pc: pc
	| slow bytes store |
	slow := self coldSendSlow: Treaty specselAt dest: d
		slots: (Array with: b with: c) pcAfter: pc + 1.
	bytes := asm newLabel.
	store := asm newLabel.
	self loadSlot: self rdx from: b.
	asm testR: self rdx imm32: 7.
	asm j: #ne to: slow.
	self loadSlot: self r9 from: c.
	asm testR: self r9 imm32: 1.
	asm j: #e to: slow.
	asm sarR: self r9 by: 1. "1-based index"
	self genHeaderInfoOf: self rdx else: slow.
	asm cmpR: self r9 imm32: 1.
	asm j: #l to: slow.
	asm cmpR: self r8 imm32: Treaty fmtPtrs.
	asm j: #ne to: bytes.
	asm cmpR: self r9 fromR: self rdi.
	asm j: #g to: slow.
	asm movR: self rdx fromM: self rdx index: self r9 scale: 3 disp: 0.
	asm jmpTo: store.
	asm bind: bytes.
	asm cmpR: self r8 imm32: Treaty fmtBytesBase.
	asm j: #b to: slow.
	"byte size = n*8 - pad"
	asm shlR: self rdi by: 3.
	asm subR: self rdi fromR: self r8.
	asm addR: self rdi imm8: 8.
	asm cmpR: self r9 fromR: self rdi.
	asm j: #g to: slow.
	asm movzxR: self rdx fromM8: self rdx index: self r9 disp: 7.
	asm leaR: self rdx base: self rdx index: self rdx scale: 0 disp: 1.
	asm bind: store.
	self storeSlot: d from: self rdx!

genAtPut: d obj: b idx: c pc: pc
	"value at slot c+1 (§12 ATPUT encoding)"
	| slow bytes done |
	slow := self coldSendSlow: Treaty specselAtPut dest: d
		slots: (Array with: b with: c with: c + 1) pcAfter: pc + 1.
	bytes := asm newLabel.
	done := asm newLabel.
	self loadSlot: self r10 from: b.
	asm testR: self r10 imm32: 7.
	asm j: #ne to: slow.
	self loadSlot: self r9 from: c.
	asm testR: self r9 imm32: 1.
	asm j: #e to: slow.
	asm sarR: self r9 by: 1.
	self genHeaderInfoOf: self r10 else: slow.
	"immutable (gcBits bit 7)?"
	asm testR: self rsi imm32: 128.
	asm j: #ne to: slow.
	asm cmpR: self r9 imm32: 1.
	asm j: #l to: slow.
	self loadSlot: self rdx from: c + 1.
	asm cmpR: self r8 imm32: Treaty fmtPtrs.
	asm j: #ne to: bytes.
	asm cmpR: self r9 fromR: self rdi.
	asm j: #g to: slow.
	asm movM: self r10 index: self r9 scale: 3 disp: 0 fromR: self rdx.
	asm movR: self rsi fromR: self r10.
	self genBarrierObj: self rsi value: self rdx.
	asm jmpTo: done.
	asm bind: bytes.
	asm cmpR: self r8 imm32: Treaty fmtBytesBase.
	asm j: #b to: slow.
	asm shlR: self rdi by: 3.
	asm subR: self rdi fromR: self r8.
	asm addR: self rdi imm8: 8.
	asm cmpR: self r9 fromR: self rdi.
	asm j: #g to: slow.
	"value must be a SmallInteger 0..255"
	asm testR: self rdx imm32: 1.
	asm j: #e to: slow.
	asm sarR: self rdx by: 1.
	asm cmpR: self rdx imm32: 255.
	asm j: #g to: slow.
	asm testR: self rdx fromR: self rdx.
	asm j: #s to: slow.
	asm movM8: self r10 index: self r9 disp: 7 fromR: self rdx.
	asm bind: done.
	"ATPUT's result is the value; reload it un-clobbered"
	self loadSlot: self rdx from: c + 1.
	self storeSlot: d from: self rdx! !

!StAMD64MacroAssembler methodsFor: 'templates - sends'!
genStubTail
	"after an exiting stub that answers displacement-or-address (M3
	 stubs): > DISP_ERROR means a native address to jump to; otherwise
	 the disposition leaves through the exit glue"
	| jj |
	jj := asm newLabel.
	asm cmpR: self rax imm32: Treaty dispError.
	asm j: #a to: jj.
	asm jmpM: self lnk disp: 8 * Treaty lnkExit.
	asm bind: jj.
	asm jmpR: self rax!

genSend: d recv: r site: s pcAfter: pcAfter
	^self genSend: d recv: r site: s pcAfter: pcAfter countHits: false!

genSend: d recv: r site: s pcAfter: pcAfter countHits: countHits
	"the monomorphic inline-cache send (JIT.md §8, Annex J.5). Answers
	 (Array with: classImmOffset with: targetRel32Offset) for the patch
	 records. The hit path writes the callee's four control words, bumps
	 the serial, sets Process.frameOffset, and jumps to the patched
	 target (a compiled entry or the CALL_INTERP glue — always inside
	 the cache). The miss path hands the whole send to SEND_MISS."
	| isInt haveCls miss classOff targetOff retinfo |
	isInt := asm newLabel.
	haveCls := asm newLabel.
	miss := asm newLabel.
	self loadSlot: self rdx from: r.
	asm testR: self rdx imm32: 1.
	asm j: #ne to: isInt.
	asm movR: self r8 fromM: self rdx disp: 0.
	asm shrR: self r8 by: Treaty hdrClassShift.
	asm jmpTo: haveCls.
	asm bind: isInt.
	asm movR: self r8 imm32: Treaty classSmallinteger.
	asm bind: haveCls.
	classOff := asm cmp32R: self r8 imm32: 0.
	asm j: #ne to: miss.
	countHits ifTrue: [
		"profiling level >= 1 (JIT.md §16): count the cache hit"
		asm movR: self r10 fromM: self fp disp: 8 * Treaty frameMethod.
		asm movR: self r10 fromM: self r10 disp: 8 + (8 * Treaty methodSendSites).
		asm movR: self r11 fromM: self r10
			disp: 8 + (8 * (s * Treaty siteStride + Treaty siteCounters)).
		asm addR: self r11 imm8: 2.
		asm movM: self r10
			disp: 8 + (8 * (s * Treaty siteStride + Treaty siteCounters))
			fromR: self r11].
	"hit: control words at bytecode slots r-4..r-1 (FP + 8*r ..)"
	asm movR: self rsi fromM: self gbl disp: Treaty gblActiveProcess.
	asm movR: self rcx fromM: self rsi disp: 8 + (8 * Treaty processFrameOffset).
	asm sarR: self rcx by: 1.
	asm leaR: self r9 base: self rcx index: self rcx scale: 0 disp: 1.
	asm movM: self fp disp: 8 * r fromR: self r9.
	retinfo := (s + (d bitShift: Treaty retinfoDestShift)
		+ (pcAfter bitShift: Treaty retinfoPcShift)) * 2 + 1.
	asm movR: self r9 imm64: retinfo.
	asm movM: self fp disp: 8 * r + 8 fromR: self r9.
	asm movR: self r10 fromM: self fp disp: 8 * Treaty frameMethod.
	asm movR: self r10 fromM: self r10 disp: 8 + (8 * Treaty methodSendSites).
	asm movR: self r10 fromM: self r10
		disp: 8 + (8 * (s * Treaty siteStride + Treaty siteCacheMethod)).
	asm movM: self fp disp: 8 * r + 16 fromR: self r10.
	"serial++ and the callee's handler/ensure frame bits"
	asm movR: self r11 fromM: self rsi disp: 8 + (8 * Treaty processSerialCounter).
	asm sarR: self r11 by: 1.
	asm addR: self r11 imm8: 1.
	asm leaR: self rdi base: self r11 index: self r11 scale: 0 disp: 1.
	asm movM: self rsi disp: 8 + (8 * Treaty processSerialCounter) fromR: self rdi.
	asm movR: self rdi fromM: self r10 disp: 8 + (8 * Treaty methodHeader).
	asm sarR: self rdi by: 1.
	asm shrR: self rdi by: Treaty mhFlagsShift.
	asm andR: self rdi imm32: 3.
	asm shlR: self r11 by: Treaty serialShift.
	asm orR: self r11 fromR: self rdi.
	asm leaR: self r11 base: self r11 index: self r11 scale: 0 disp: 1.
	asm movM: self fp disp: 8 * r + 24 fromR: self r11.
	"Process.frameOffset := tagged(myOffset + r); enter the callee"
	asm leaR: self rdi base: self rcx disp: r.
	asm leaR: self rdi base: self rdi index: self rdi scale: 0 disp: 1.
	asm movM: self rsi disp: 8 + (8 * Treaty processFrameOffset) fromR: self rdi.
	targetOff := asm jmpPatchableTo: miss.
	self coldBlock: [
		asm bind: miss.
		self genVmArg.
		asm movR: self rsi imm32: s.
		asm movR: self rdx imm32: r + (d bitShift: 8).
		asm movR: self rcx imm32: pcAfter.
		asm callM: self lnk disp: 8 * Treaty lnkSendMiss.
		self genStubTail].
	^Array with: classOff with: targetOff!

genPrimCall: primNumber
	"the PRIM template (JIT.md §10): the frequent SmallInteger
	 primitives get an inline fast path (both-ints check, the operation,
	 the shared return sequence); everything else — and every inline
	 failure — runs the numbered primitive via the PRIM_CALL stub.
	 Stub answer 0 = clean failure -> fall through to the compiled
	 fallback body (FP re-derived); otherwise continuation address or
	 exit disposition."
	| fail jj slow |
	slow := asm newLabel.
	self genInlinePrim: primNumber slow: slow.
	asm bind: slow.
	fail := asm newLabel.
	jj := asm newLabel.
	self genVmArg.
	asm callM: self lnk disp: 8 * Treaty lnkPrimCall.
	asm testR: self rax fromR: self rax.
	asm j: #e to: fail.
	asm cmpR: self rax imm32: Treaty dispError.
	asm j: #a to: jj.
	asm jmpM: self lnk disp: 8 * Treaty lnkExit.
	asm bind: jj.
	asm jmpR: self rax.
	asm bind: fail.
	self deriveFP!

genInlinePrim: n slow: slow
	"inline fast paths for the hot SmallInteger primitives; receiver at
	 slot 0, argument at slot 1; success returns through the epilogue"
	(n >= Treaty primIntAdd and: [n <= Treaty primIntMul]) ifTrue: [
		self genBothIntsA: 0 b: 1 else: slow.
		n = Treaty primIntAdd ifTrue: [
			asm addR: self rdx fromR: self rsi.
			asm j: #o to: slow.
			asm addR: self rdx imm8: -1].
		n = Treaty primIntSub ifTrue: [
			asm subR: self rdx fromR: self rsi.
			asm j: #o to: slow.
			asm addR: self rdx imm8: 1].
		n = Treaty primIntMul ifTrue: [
			asm sarR: self rdx by: 1.
			asm addR: self rsi imm8: -1.
			asm imulR: self rdx fromR: self rsi.
			asm j: #o to: slow.
			asm addR: self rdx imm8: 1].
		^asm jmpTo: epilogueLabel].
	(n >= Treaty primIntBitAnd and: [n <= Treaty primIntBitXor]) ifTrue: [
		self genBothIntsA: 0 b: 1 else: slow.
		n = Treaty primIntBitAnd ifTrue: [asm andR: self rdx fromR: self rsi].
		n = Treaty primIntBitOr ifTrue: [asm orR: self rdx fromR: self rsi].
		n = Treaty primIntBitXor ifTrue: [
			asm xorR: self rdx fromR: self rsi.
			asm addR: self rdx imm8: 1].
		^asm jmpTo: epilogueLabel].
	(n >= Treaty primIntLt and: [n <= Treaty primIntEq]) ifTrue: [
		| t e cond |
		n = Treaty primIntLt ifTrue: [cond := #l].
		n = Treaty primIntGt ifTrue: [cond := #g].
		n = Treaty primIntLe ifTrue: [cond := #le].
		n = Treaty primIntGe ifTrue: [cond := #ge].
		n = Treaty primIntEq ifTrue: [cond := #e].
		t := asm newLabel.
		e := asm newLabel.
		self genBothIntsA: 0 b: 1 else: slow.
		asm cmpR: self rdx fromR: self rsi.
		asm j: cond to: t.
		asm movR: self rdx fromM: self gbl disp: Treaty gblFalse.
		asm jmpTo: e.
		asm bind: t.
		asm movR: self rdx fromM: self gbl disp: Treaty gblTrue.
		asm bind: e.
		^asm jmpTo: epilogueLabel].
	"no inline form: fall to the stub"
	^self!

genNlr: a pc: pc
	self genVmArg.
	asm movR: self rsi imm32: a.
	asm movR: self rdx imm32: pc + 1.
	asm callM: self lnk disp: 8 * Treaty lnkNlr.
	self genStubTail!

genMkClosure: d litIndex: k ncap: ncap hasNlr: hasNlr
	"allocate and fill the closure; fresh young object, so the stores
	 are barrier-exempt. Home fields per §10: nil for non-NLR blocks;
	 copied from the enclosing closure inside a block activation; the
	 current activation's (process, offset, serial) triple otherwise."
	| blockctx done |
	self genAllocClass: Treaty classBlockclosure
		format: Treaty fmtFixed
		slots: Treaty closureCapturedBase + ncap.
	asm movR: self rdx fromR: self rax.
	self deriveFP.
	asm movR: self rsi fromM: self fp disp: 8 * Treaty frameMethod.
	asm movR: self rsi fromM: self rsi disp: 8 + (8 * Treaty methodLiterals).
	asm movR: self rsi fromM: self rsi disp: 8 + (8 * k).
	asm movM: self rdx disp: 8 + (8 * Treaty closureCompiledBlock) fromR: self rsi.
	hasNlr
		ifFalse: [
			asm movR: self rsi fromM: self gbl disp: Treaty gblNil.
			asm movM: self rdx disp: 8 + (8 * Treaty closureHomeProcess) fromR: self rsi.
			asm movM: self rdx disp: 8 + (8 * Treaty closureHomeOffset) imm32: 1.
			asm movM: self rdx disp: 8 + (8 * Treaty closureHomeSerial) imm32: 1]
		ifTrue: [
			blockctx := asm newLabel.
			done := asm newLabel.
			asm movR: self rsi fromM: self fp disp: 8 * Treaty frameFlags.
			asm sarR: self rsi by: 1.
			asm testR: self rsi imm32: Treaty flagBlockctx.
			asm j: #ne to: blockctx.
			asm movR: self r8 fromM: self gbl disp: Treaty gblActiveProcess.
			asm movM: self rdx disp: 8 + (8 * Treaty closureHomeProcess) fromR: self r8.
			asm movR: self r9 fromM: self r8 disp: 8 + (8 * Treaty processFrameOffset).
			asm movM: self rdx disp: 8 + (8 * Treaty closureHomeOffset) fromR: self r9.
			asm movR: self r9 fromM: self fp disp: 8 * Treaty frameFlags.
			asm sarR: self r9 by: 1.
			asm shrR: self r9 by: Treaty serialShift.
			asm leaR: self r9 base: self r9 index: self r9 scale: 0 disp: 1.
			asm movM: self rdx disp: 8 + (8 * Treaty closureHomeSerial) fromR: self r9.
			asm jmpTo: done.
			asm bind: blockctx.
			asm movR: self rdi fromM: self fp disp: (self slotDisp: 0).
			asm movR: self r9 fromM: self rdi disp: 8 + (8 * Treaty closureHomeProcess).
			asm movM: self rdx disp: 8 + (8 * Treaty closureHomeProcess) fromR: self r9.
			asm movR: self r9 fromM: self rdi disp: 8 + (8 * Treaty closureHomeOffset).
			asm movM: self rdx disp: 8 + (8 * Treaty closureHomeOffset) fromR: self r9.
			asm movR: self r9 fromM: self rdi disp: 8 + (8 * Treaty closureHomeSerial).
			asm movM: self rdx disp: 8 + (8 * Treaty closureHomeSerial) fromR: self r9.
			asm bind: done].
	self storeSlot: d from: self rdx!

genCapture: c from: a closureSlot: closureSlot
	"store into the (young, freshly made) closure's capture slot"
	self loadSlot: self rdx from: closureSlot.
	self loadSlot: self rsi from: a.
	asm movM: self rdx disp: 8 + (8 * (Treaty closureCapturedBase + c))
		fromR: self rsi! !

!StAMD64MacroAssembler methodsFor: 'templates - control'!
genJumpTo: label backward: isBack targetPc: tpc
	isBack ifTrue: [self genPoll: tpc].
	asm jmpTo: label!

genJumpTrue: a to: label backward: isBack targetPc: tpc pc: pc ifTrue: wantTrue
	"JUMPTRUE/JUMPFALSE: non-booleans exit (the interpreter re-executes
	 the jump and performs the mustBeBoolean send)"
	| slow notTaken |
	slow := self coldExitAt: pc.
	notTaken := asm newLabel.
	self loadSlot: self rdx from: a.
	asm cmpR: self rdx
		fromM: self gbl
		disp: (wantTrue ifTrue: [Treaty gblTrue] ifFalse: [Treaty gblFalse]).
	asm j: #ne to: notTaken.
	isBack ifTrue: [self genPoll: tpc].
	asm jmpTo: label.
	asm bind: notTaken.
	asm cmpR: self rdx
		fromM: self gbl
		disp: (wantTrue ifTrue: [Treaty gblFalse] ifFalse: [Treaty gblTrue]).
	asm j: #ne to: slow! !
