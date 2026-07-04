"Code generator (SPEC §18): AST → mnemonic instruction sequences over
 frame slots, with the slot allocator (expression depth counter + temp
 map), capture analysis (§10 copy-vs-box), macro expansion of
 ifTrue:/ifFalse:/and:/or:/whileTrue:/whileFalse: on literal argless
 blocks, and specialized-send selection (§12).

 Conventions (must match the Rust VM):
 - bytecode slot 0 = receiver, 1..argc = args, then temps, then scratch.
 - a send staged at slot r clobbers slots r-4..r-1 (callee control words),
   so sends stage at r = sp+4 where sp is the current scratch top.
 - primitive methods: slot 1+argc receives the failure code; if the method
   declares temps, the first temp aliases it, otherwise the slot is
   reserved invisibly.
 - <handlerFrame> / <ensureFrame> pragmas: handlerSlotBase = 1+argc and
   the first 3 / 4 declared temps are the VM-reserved slots.
 - blocks: slot 0 = the closure; captured values are read with
   GETIVAR d, 4+i; self is captured at index 0 when needed; instance
   variables inside blocks go through instVarAt:(put:) sends.

 Determinism (§18): only insertion-ordered collections anywhere; literal
 dedup uses an OrderedCollection scan, symbols/sites in emission order."

Object subclass: #StGlobalRef
	instanceVariableNames: 'name'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Compiler'!

!StGlobalRef class methodsFor: 'instance creation'!
named: aString
	| g |
	g := self new.
	g setName: aString.
	^g! !

!StGlobalRef methodsFor: 'accessing'!
setName: aString name := aString!
name ^name!
= other
	^(other isKindOf: StGlobalRef) and: [name = other name]!
hash
	^name hash!
printOn: aStream
	aStream nextPutAll: '{global ' , name , '}'! !

Object subclass: #StSiteSpec
	instanceVariableNames: 'selector argc isSuper'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Compiler'!

!StSiteSpec methodsFor: 'accessing'!
selector ^selector!
selector: s selector := s!
argc ^argc!
argc: n argc := n!
isSuper ^isSuper!
isSuper: b isSuper := b!
printOn: aStream
	aStream nextPutAll: '{site ' , selector.
	isSuper ifTrue: [aStream nextPutAll: ' super'].
	aStream nextPut: $}! !

Object subclass: #StCodeSpec
	instanceVariableNames: 'selector argc frameSlots primitive handlerSlotBase mhFlags insns literals sites'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Compiler'!

"Common shape of compiled methods and compiled blocks."

!StCodeSpec methodsFor: 'accessing'!
selector ^selector!
selector: s selector := s!
argc ^argc!
argc: n argc := n!
frameSlots ^frameSlots!
frameSlots: n frameSlots := n!
primitive ^primitive!
primitive: n primitive := n!
handlerSlotBase ^handlerSlotBase!
handlerSlotBase: n handlerSlotBase := n!
mhFlags ^mhFlags!
mhFlags: n mhFlags := n!
insns ^insns!
insns: c insns := c!
literals ^literals!
literals: c literals := c!
sites ^sites!
sites: c sites := c! !

!StCodeSpec methodsFor: 'printing'!
mnemonics
	"one instruction per line, formatted exactly like the Rust
	 disassembler's Display output — eyeball-comparable across suites"
	| ws |
	ws := WriteStream on: String new.
	insns do: [:i | ws nextPutAll: (self printInsn: i)]
		separatedBy: [ws nextPut: (Character value: 10)].
	^ws contents!

printInsn: insn
	| ws |
	ws := WriteStream on: String new.
	ws nextPutAll: (insn at: 1).
	2 to: insn size do: [:k |
		ws nextPutAll: (k = 2 ifTrue: [' '] ifFalse: [', ']).
		ws nextPutAll: (insn at: k) printString].
	^ws contents! !

StCodeSpec subclass: #StMethodSpec
	instanceVariableNames: 'methodClassName'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Compiler'!

!StMethodSpec methodsFor: 'accessing'!
methodClassName ^methodClassName!
methodClassName: n methodClassName := n! !

StCodeSpec subclass: #StBlockSpec
	instanceVariableNames: 'numCaptured hasNlr'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Compiler'!

!StBlockSpec methodsFor: 'accessing'!
numCaptured ^numCaptured!
numCaptured: n numCaptured := n!
hasNlr ^hasNlr!
hasNlr: b hasNlr := b! !

Object subclass: #StClassScope
	instanceVariableNames: 'name instVarNames'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Compiler'!

"What the code generator needs to know about the class a method is
 compiled for: the full (inherited-first) instance-variable name list."

!StClassScope class methodsFor: 'instance creation'!
name: aString instVarNames: aCollection
	| s |
	s := self new.
	s setName: aString instVarNames: aCollection.
	^s!

empty
	^self name: 'UndefinedObject' instVarNames: OrderedCollection new! !

!StClassScope methodsFor: 'accessing'!
setName: aString instVarNames: aCollection
	name := aString.
	instVarNames := aCollection!
name ^name!
instVarNames ^instVarNames!
indexOfInstVar: aString
	"0-based ivar index, or nil"
	| i |
	i := 0.
	instVarNames do: [:n |
		n = aString ifTrue: [^i].
		i := i + 1].
	^nil! !

Object subclass: #StCodeGen
	instanceVariableNames: 'classScope node parent captureNames captureSelf
argNames tempNames boxedNames insns literals sites sp maxSp scratchBase
isBlock reservedFailureSlot loopBindings'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Compiler'!

!StCodeGen class methodsFor: 'compiling'!
compileMethod: methodNode inClass: aClassScope
	| gen |
	gen := self new.
	gen setMethod: methodNode class: aClassScope.
	^gen compile!

compileMethodSource: aString inClass: aClassScope
	^self compileMethod: (StParser parseMethod: aString) inClass: aClassScope!

compileDoItSource: aString inClass: aClassScope
	^self compileMethod: (StParser parseDoIt: aString) inClass: aClassScope! !

!StCodeGen methodsFor: 'setup'!
setMethod: methodNode class: aClassScope
	classScope := aClassScope.
	node := methodNode.
	parent := nil.
	isBlock := false.
	captureNames := OrderedCollection new.
	captureSelf := false!

setBlock: blockNode parent: aGen captures: names capturesSelf: selfFlag
	classScope := aGen classScope.
	node := blockNode.
	parent := aGen.
	isBlock := true.
	captureNames := names.
	captureSelf := selfFlag!

classScope ^classScope!
node ^node!
argNames ^argNames!
tempNames ^tempNames!
boxedNames ^boxedNames!
captureNames ^captureNames!
captureSelf ^captureSelf!
isBlock ^isBlock! !

!StCodeGen methodsFor: 'compiling'!
compile
	| spec prim handlerBase flags stmts |
	insns := OrderedCollection new.
	literals := OrderedCollection new.
	sites := OrderedCollection new.
	loopBindings := OrderedCollection new.
	argNames := node arguments asOrderedCollection.
	tempNames := (node isKindOf: StBlockNode)
		ifTrue: [node temporaries asOrderedCollection]
		ifFalse: [node temporaries asOrderedCollection].
	argNames size > 15 ifTrue: [self error: 'too many arguments'].
	prim := nil.
	handlerBase := 0.
	flags := 0.
	reservedFailureSlot := false.
	isBlock ifFalse: [
		node pragmas do: [:p |
			p selector = #primitive: ifTrue: [prim := p arguments first].
			p selector = #handlerFrame ifTrue: [
				flags := flags bitOr: Treaty mhFlagIsHandler.
				handlerBase := 1 + argNames size.
				tempNames size < 3 ifTrue: [self error: 'handlerFrame needs 3 temps']].
			p selector = #ensureFrame ifTrue: [
				flags := flags bitOr: Treaty mhFlagIsEnsure.
				handlerBase := 1 + argNames size.
				tempNames size < 4 ifTrue: [self error: 'ensureFrame needs 4 temps']]].
		(prim notNil and: [tempNames isEmpty]) ifTrue: [reservedFailureSlot := true]].

	scratchBase := 1 + argNames size + tempNames size.
	reservedFailureSlot ifTrue: [scratchBase := scratchBase + 1].
	sp := scratchBase.
	maxSp := sp.

	boxedNames := self classifyLocals.

	prim notNil ifTrue: [self emit: (Array with: 'PRIM' with: prim)].

	"box captured-and-mutated locals at entry (frame slots start nil /
	 hold raw argument values)"
	argNames do: [:n |
		(boxedNames includes: n) ifTrue: [
			| s |
			s := self slotOfLocal: n.
			self emit: (Array with: 'MKBOX' with: s with: s)]].
	tempNames do: [:n |
		(boxedNames includes: n) ifTrue: [
			| s |
			s := self slotOfLocal: n.
			self emit: (Array with: 'MKBOX' with: s with: s)]].

	stmts := node statements.
	self compileBody: stmts.

	spec := isBlock
		ifTrue: [StBlockSpec new]
		ifFalse: [StMethodSpec new].
	spec selector: (isBlock ifTrue: [nil] ifFalse: [node selector]).
	spec argc: argNames size.
	spec frameSlots: (maxSp min: 255).
	maxSp > 255 ifTrue: [self error: 'method needs too many frame slots'].
	spec primitive: prim.
	spec handlerSlotBase: handlerBase.
	spec mhFlags: flags.
	spec insns: insns.
	spec literals: literals.
	spec sites: sites.
	isBlock ifTrue: [
		spec numCaptured: captureNames size + (captureSelf ifTrue: [1] ifFalse: [0]).
		spec hasNlr: (self subtreeHasReturn: node)].
	^spec!

compileBody: stmts
	| last |
	stmts isEmpty ifTrue: [
		isBlock
			ifTrue: [
				self emit: (Array with: 'LOADNIL' with: sp).
				self emit: (Array with: 'RET' with: sp)]
			ifFalse: [self emit: (Array with: 'RETSELF')].
		^self].
	1 to: stmts size do: [:i |
		| stmt isLast |
		stmt := stmts at: i.
		isLast := i = stmts size.
		(stmt isKindOf: StReturnNode)
			ifTrue: [
				self compileReturn: stmt.
				^self]
			ifFalse: [
				isLast
					ifTrue: [
						isBlock
							ifTrue: [
								"a block answers its last statement"
								self compileExpr: stmt to: sp.
								self emit: (Array with: 'RET' with: sp)]
							ifFalse: [
								self compileExpr: stmt to: sp.
								self emit: (Array with: 'RETSELF')]]
					ifFalse: [self compileExpr: stmt to: sp]]]!

compileReturn: stmt
	| e |
	e := stmt expression.
	isBlock
		ifTrue: [
			self compileExpr: e to: sp.
			self emit: (Array with: 'NLR' with: sp)]
		ifFalse: [
			((e isKindOf: StVariableNode) and: [e name = 'self'])
				ifTrue: [^self emit: (Array with: 'RETSELF')].
			self compileExpr: e to: sp.
			self emit: (Array with: 'RET' with: sp)]! !

!StCodeGen methodsFor: 'emitting'!
emit: insnArray
	insns add: insnArray!

emitPlaceholder
	insns add: nil.
	^insns size!

patch: index with: insnArray
	insns at: index put: insnArray!

noteSp: n
	n + 1 > maxSp ifTrue: [maxSp := n + 1]!

literalIndex: value
	"insertion-ordered dedup by same-class equality ('foo' = #foo is true
	 in Smalltalk, but they are different literals!)"
	| i |
	i := 1.
	literals do: [:lit |
		(lit class == value class and: [lit = value]) ifTrue: [^i - 1].
		i := i + 1].
	literals add: value.
	^literals size - 1!

blockLiteralIndex: spec
	"block specs are never deduped"
	literals add: spec.
	^literals size - 1!

siteIndex: selector argc: n super: superFlag
	"each call site gets its own entry — inline caches are per-site (§8)"
	| s |
	s := StSiteSpec new.
	s selector: selector.
	s argc: n.
	s isSuper: superFlag.
	sites add: s.
	sites size > 255 ifTrue: [self error: 'too many send sites'].
	^sites size - 1! !

!StCodeGen methodsFor: 'scopes'!
slotOfLocal: name
	"bytecode slot of an argument or temp of this scope, or nil"
	| i |
	i := 1.
	argNames do: [:n |
		n = name ifTrue: [^i].
		i := i + 1].
	reservedFailureSlot ifTrue: [i := i + 1].
	tempNames do: [:n |
		n = name ifTrue: [^i].
		i := i + 1].
	^nil!

capturedIndexOf: name
	"index into this block's captured values (0-based, self-adjusted)"
	| base i |
	base := captureSelf ifTrue: [1] ifFalse: [0].
	i := 0.
	captureNames do: [:n |
		n = name ifTrue: [^base + i].
		i := i + 1].
	^nil!

resolve: name
	"→ #(#slot s boxed) | #(#captured i boxed) | #(#self) | #(#ivar i)
	   | #(#global) | #(#pseudo #true/#false/#nil)"
	| s idx k |
	name = 'self' ifTrue: [^Array with: #self].
	name = 'true' ifTrue: [^Array with: #pseudo with: #true].
	name = 'false' ifTrue: [^Array with: #pseudo with: #false].
	name = 'nil' ifTrue: [^Array with: #pseudo with: #nil].
	name = 'super' ifTrue: [self error: 'super outside receiver position'].
	name = 'thisContext' ifTrue: [self error: 'thisContext is not supported'].
	k := loopBindings size.
	[k >= 1] whileTrue: [
		((loopBindings at: k) at: 1) = name ifTrue: [
			^Array with: #slot with: ((loopBindings at: k) at: 2) with: false].
		k := k - 1].
	s := self slotOfLocal: name.
	s notNil ifTrue: [
		^Array with: #slot with: s with: (boxedNames includes: name)].
	isBlock ifTrue: [
		idx := self capturedIndexOf: name.
		idx notNil ifTrue: [
			^Array with: #captured with: idx with: (self isBoxedInAncestors: name)]].
	idx := classScope indexOfInstVar: name.
	idx notNil ifTrue: [^Array with: #ivar with: idx].
	^Array with: #global!

isBoxedInAncestors: name
	| gen s |
	gen := parent.
	[gen notNil] whileTrue: [
		s := gen slotOfLocal: name.
		s notNil ifTrue: [^gen boxedNames includes: name].
		gen := gen parentGen].
	self error: 'unresolved captured variable ' , name!

parentGen ^parent! !

!StCodeGen methodsFor: 'analysis'!
macroKindOf: n
	"non-nil iff this message compiles as an inline control macro; the
	 analysis walkers and compileMessage must agree exactly"
	| sel args |
	(n isKindOf: StMessageNode) ifFalse: [^nil].
	sel := n selector.
	args := n arguments.
	(sel = #ifTrue:ifFalse: and: [args size = 2
		and: [(self isInlinableBlock: (args at: 1))
		and: [self isInlinableBlock: (args at: 2)]]]) ifTrue: [^#ifTrueIfFalse].
	(sel = #ifFalse:ifTrue: and: [args size = 2
		and: [(self isInlinableBlock: (args at: 1))
		and: [self isInlinableBlock: (args at: 2)]]]) ifTrue: [^#ifFalseIfTrue].
	(sel = #ifTrue: and: [args size = 1
		and: [self isInlinableBlock: (args at: 1)]]) ifTrue: [^#ifTrue].
	(sel = #ifFalse: and: [args size = 1
		and: [self isInlinableBlock: (args at: 1)]]) ifTrue: [^#ifFalse].
	(sel = #and: and: [args size = 1
		and: [self isInlinableBlock: (args at: 1)]]) ifTrue: [^#and].
	(sel = #or: and: [args size = 1
		and: [self isInlinableBlock: (args at: 1)]]) ifTrue: [^#or].
	((sel = #whileTrue: or: [sel = #whileFalse:]) and: [args size = 1
		and: [(self isInlinableBlock: n receiver)
		and: [self isInlinableBlock: (args at: 1)]]]) ifTrue: [^#while].
	((sel = #whileTrue or: [sel = #whileFalse]) and: [args isEmpty
		and: [self isInlinableBlock: n receiver]]) ifTrue: [^#whileNoBody].
	(sel = #to:do: and: [args size = 2 and: [self toDoInlinable: n]])
		ifTrue: [^#toDo].
	^nil!

toDoShape: n
	"syntactic shape: X to: Y do: [:v | ...] with a literal 1-arg, 0-temp
	 block argument"
	| body |
	((n isKindOf: StMessageNode) and: [n selector = #to:do:
		and: [n arguments size = 2]]) ifFalse: [^false].
	body := n arguments at: 2.
	^(body isKindOf: StBlockNode)
		and: [body arguments size = 1 and: [body temporaries isEmpty]]!

toDoInlinable: n
	"pure function of the method AST + class scope, so every analysis
	 walker and every nested codegen agrees (docs/codegen-plan.md).
	 Inline iff the loop variable is not an ivar name, occurs nowhere in
	 the method outside to:do: bodies that bind it, and inside this body
	 is never assigned and never referenced from a nested real
	 (non-inlined) block."
	| body v |
	(self toDoShape: n) ifFalse: [^false].
	body := n arguments at: 2.
	v := body arguments first.
	(classScope indexOfInstVar: v) notNil ifTrue: [^false].
	(self rootMethodBinds: v) ifTrue: [^false].
	^self loopVar: v safeInInline: body statements!

rootMethodBinds: v
	"v declared or mentioned anywhere in the whole method outside
	 to:do:-shaped bodies binding v"
	| g root |
	g := self.
	[g parentGen notNil] whileTrue: [g := g parentGen].
	root := g node.
	(root arguments includes: v) ifTrue: [^true].
	(root temporaries includes: v) ifTrue: [^true].
	root statements do: [:s | (self name: v occursIn: s) ifTrue: [^true]].
	^false!

name: v occursIn: n
	"any appearance of v in the subtree - reference, assignment target,
	 or declaration - skipping to:do:-shaped bodies whose own loop
	 variable is v (they shadow it, whether inlined or not)"
	(n isKindOf: StVariableNode) ifTrue: [^n name = v].
	(n isKindOf: StAssignmentNode) ifTrue: [
		n variable = v ifTrue: [^true].
		^self name: v occursIn: n expression].
	(n isKindOf: StReturnNode) ifTrue: [^self name: v occursIn: n expression].
	(n isKindOf: StCascadeNode) ifTrue: [
		(self name: v occursIn: n receiver) ifTrue: [^true].
		n messages do: [:m |
			m arguments do: [:a | (self name: v occursIn: a) ifTrue: [^true]]].
		^false].
	(n isKindOf: StMessageNode) ifTrue: [
		(self name: v occursIn: n receiver) ifTrue: [^true].
		((self toDoShape: n) and: [((n arguments at: 2) arguments first) = v])
			ifTrue: [^self name: v occursIn: (n arguments at: 1)].
		n arguments do: [:a | (self name: v occursIn: a) ifTrue: [^true]].
		^false].
	(n isKindOf: StBlockNode) ifTrue: [
		(n arguments includes: v) ifTrue: [^true].
		(n temporaries includes: v) ifTrue: [^true].
		n statements do: [:s | (self name: v occursIn: s) ifTrue: [^true]].
		^false].
	^false!

loopVar: v safeInInline: stmts
	stmts do: [:s | (self loopVar: v safeIn: s) ifFalse: [^false]].
	^true!

loopVar: v safeIn: n
	"walking code that compiles inline in the loop's scope: assignment to
	 v is out; any occurrence of v inside a block that compiles as a real
	 closure is out. Macro-flattened blocks are transparent; a nested
	 to:do: is transparent iff it is itself inlinable (an inner to:do:
	 that falls back turns its body into a real closure)."
	| kind |
	(n isKindOf: StAssignmentNode) ifTrue: [
		n variable = v ifTrue: [^false].
		^self loopVar: v safeIn: n expression].
	(n isKindOf: StReturnNode) ifTrue: [^self loopVar: v safeIn: n expression].
	(n isKindOf: StCascadeNode) ifTrue: [
		(self loopVar: v safeIn: n receiver) ifFalse: [^false].
		n messages do: [:m |
			m arguments do: [:a |
				(a isKindOf: StBlockNode)
					ifTrue: [(self name: v occursIn: a) ifTrue: [^false]]
					ifFalse: [(self loopVar: v safeIn: a) ifFalse: [^false]]]].
		^true].
	(n isKindOf: StBlockNode) ifTrue: [
		"a block in expression position compiles as a real closure"
		^(self name: v occursIn: n) not].
	(n isKindOf: StMessageNode) ifTrue: [
		kind := self macroKindOf: n.
		kind isNil ifTrue: [
			(n receiver isKindOf: StBlockNode)
				ifTrue: [(self name: v occursIn: n receiver) ifTrue: [^false]]
				ifFalse: [(self loopVar: v safeIn: n receiver) ifFalse: [^false]].
			n arguments do: [:a |
				(a isKindOf: StBlockNode)
					ifTrue: [(self name: v occursIn: a) ifTrue: [^false]]
					ifFalse: [(self loopVar: v safeIn: a) ifFalse: [^false]]].
			^true].
		kind = #toDo ifTrue: [
			(self loopVar: v safeIn: n receiver) ifFalse: [^false].
			(self loopVar: v safeIn: (n arguments at: 1)) ifFalse: [^false].
			((n arguments at: 2) arguments first) = v ifTrue: [^true].
			^self loopVar: v safeInInline: (n arguments at: 2) statements].
		(n receiver isKindOf: StBlockNode)
			ifTrue: [(self loopVar: v safeInInline: n receiver statements)
				ifFalse: [^false]]
			ifFalse: [(self loopVar: v safeIn: n receiver) ifFalse: [^false]].
		n arguments do: [:a |
			(a isKindOf: StBlockNode)
				ifTrue: [(self loopVar: v safeInInline: a statements) ifFalse: [^false]]
				ifFalse: [(self loopVar: v safeIn: a) ifFalse: [^false]]].
		^true].
	^true!

walkMessage: n with: walker
	"walk a message's parts; macro-inlined literal blocks are traversed
	 as inline code, not treated as closures"
	(self macroKindOf: n) isNil
		ifTrue: [
			walker value: n receiver.
			n arguments do: [:a | walker value: a]]
		ifFalse: [
			(n receiver isKindOf: StBlockNode)
				ifTrue: [n receiver statements do: [:st | walker value: st]]
				ifFalse: [walker value: n receiver].
			n arguments do: [:a |
				(a isKindOf: StBlockNode)
					ifTrue: [a statements do: [:st | walker value: st]]
					ifFalse: [walker value: a]]]!

classifyLocals
	"§10: a local is boxed iff it is captured by some block and mutated
	 after capture (assignment inside any block, or in this scope after
	 the first capturing block). Walk in execution order with a counter."
	| boxed captureAt walker |
	boxed := OrderedCollection new.
	captureAt := OrderedCollection new. "names already captured"
	walker := nil.
	walker := [:n |
		(n isKindOf: StBlockNode) ifTrue: [
			| free |
			free := self freeLocalNamesOf: n.
			free do: [:fn |
				(captureAt includes: fn) ifFalse: [captureAt add: fn]].
			(self namesAssignedInside: n) do: [:an |
				((self isLocalName: an) and: [(boxed includes: an) not])
					ifTrue: [boxed add: an]]].
		(n isKindOf: StAssignmentNode) ifTrue: [
			((self isLocalName: n variable)
			 and: [(captureAt includes: n variable)
			 and: [(boxed includes: n variable) not]])
				ifTrue: [boxed add: n variable].
			walker value: n expression].
		(n isKindOf: StReturnNode) ifTrue: [walker value: n expression].
		(n isKindOf: StMessageNode) ifTrue: [self walkMessage: n with: walker].
		(n isKindOf: StCascadeNode) ifTrue: [
			walker value: n receiver.
			n messages do: [:m | m arguments do: [:a | walker value: a]]]].
	node statements do: [:s | walker value: s].
	^boxed!

isLocalName: name
	^(argNames includes: name) or: [tempNames includes: name]!

namesAssignedInside: blockNode
	"all names assigned anywhere in the block subtree (its own locals
	 shadow and are excluded)"
	| out ownLocals walker |
	out := OrderedCollection new.
	ownLocals := OrderedCollection new.
	ownLocals addAll: blockNode arguments; addAll: blockNode temporaries.
	walker := nil.
	walker := [:n |
		(n isKindOf: StAssignmentNode) ifTrue: [
			((ownLocals includes: n variable) not and: [(out includes: n variable) not])
				ifTrue: [out add: n variable].
			walker value: n expression].
		(n isKindOf: StReturnNode) ifTrue: [walker value: n expression].
		(n isKindOf: StMessageNode) ifTrue: [self walkMessage: n with: walker].
		(n isKindOf: StCascadeNode) ifTrue: [
			walker value: n receiver.
			n messages do: [:m | m arguments do: [:a | walker value: a]]].
		(n isKindOf: StBlockNode) ifTrue: [
			"deeper block: names its own locals shadow are excluded there"
			(self namesAssignedInside: n) do: [:an |
				((ownLocals includes: an) not and: [(out includes: an) not])
					ifTrue: [out add: an]]]].
	blockNode statements do: [:s | walker value: s].
	^out!

freeLocalNamesOf: blockNode
	"names referenced in the block subtree that resolve in THIS scope
	 (its locals, or — when this scope is itself a block — its captures),
	 in first-reference order"
	| out ownLocals walker |
	out := OrderedCollection new.
	ownLocals := OrderedCollection new.
	ownLocals addAll: blockNode arguments; addAll: blockNode temporaries.
	walker := nil.
	walker := [:n |
		(n isKindOf: StVariableNode) ifTrue: [
			((ownLocals includes: n name) not
			 and: [(self isCapturableName: n name)
			 and: [(out includes: n name) not]])
				ifTrue: [out add: n name]].
		(n isKindOf: StAssignmentNode) ifTrue: [
			((ownLocals includes: n variable) not
			 and: [(self isCapturableName: n variable)
			 and: [(out includes: n variable) not]])
				ifTrue: [out add: n variable].
			walker value: n expression].
		(n isKindOf: StReturnNode) ifTrue: [walker value: n expression].
		(n isKindOf: StMessageNode) ifTrue: [self walkMessage: n with: walker].
		(n isKindOf: StCascadeNode) ifTrue: [
			walker value: n receiver.
			n messages do: [:m | m arguments do: [:a | walker value: a]]].
		(n isKindOf: StBlockNode) ifTrue: [
			| inner |
			inner := self freeNamesOfNested: n shadowed: ownLocals.
			inner do: [:fn |
				((self isCapturableName: fn) and: [(out includes: fn) not])
					ifTrue: [out add: fn]]]].
	blockNode statements do: [:s | walker value: s].
	^out!

isCapturableName: name
	^(self isLocalName: name)
		or: [isBlock and: [(self capturedIndexOf: name) notNil]]!

freeNamesOfNested: blockNode shadowed: outerShadow
	"free names of a nested block, excluding both its own locals and the
	 given shadow set"
	| out shadow |
	shadow := OrderedCollection new.
	shadow addAll: outerShadow; addAll: blockNode arguments; addAll: blockNode temporaries.
	out := OrderedCollection new.
	(self rawFreeNamesOf: blockNode) do: [:n |
		((shadow includes: n) not and: [(out includes: n) not]) ifTrue: [out add: n]].
	^out!

rawFreeNamesOf: blockNode
	"every variable name referenced in the subtree minus locals declared
	 at each level"
	| out ownLocals walker |
	out := OrderedCollection new.
	ownLocals := OrderedCollection new.
	ownLocals addAll: blockNode arguments; addAll: blockNode temporaries.
	walker := nil.
	walker := [:n |
		(n isKindOf: StVariableNode) ifTrue: [
			((ownLocals includes: n name) not and: [(out includes: n name) not])
				ifTrue: [out add: n name]].
		(n isKindOf: StAssignmentNode) ifTrue: [
			((ownLocals includes: n variable) not and: [(out includes: n variable) not])
				ifTrue: [out add: n variable].
			walker value: n expression].
		(n isKindOf: StReturnNode) ifTrue: [walker value: n expression].
		(n isKindOf: StMessageNode) ifTrue: [self walkMessage: n with: walker].
		(n isKindOf: StCascadeNode) ifTrue: [
			walker value: n receiver.
			n messages do: [:m | m arguments do: [:a | walker value: a]]].
		(n isKindOf: StBlockNode) ifTrue: [
			(self rawFreeNamesOf: n) do: [:fn |
				((ownLocals includes: fn) not and: [(out includes: fn) not])
					ifTrue: [out add: fn]]]].
	blockNode statements do: [:s | walker value: s].
	^out!

subtreeHasReturn: blockNode
	| found walker |
	found := false.
	walker := nil.
	walker := [:n |
		(n isKindOf: StReturnNode) ifTrue: [found := true].
		(n isKindOf: StAssignmentNode) ifTrue: [walker value: n expression].
		(n isKindOf: StMessageNode) ifTrue: [
			walker value: n receiver.
			n arguments do: [:a | walker value: a]].
		(n isKindOf: StCascadeNode) ifTrue: [
			walker value: n receiver.
			n messages do: [:m | m arguments do: [:a | walker value: a]]].
		(n isKindOf: StBlockNode) ifTrue: [
			n statements do: [:s | walker value: s]]].
	blockNode statements do: [:s | walker value: s].
	^found!

blockUsesSelfOrIvars: blockNode
	"self, super, or any instance variable reference in the subtree"
	| found names |
	names := self rawFreeNamesOf: blockNode.
	found := false.
	names do: [:n |
		(n = 'self' or: [n = 'super']) ifTrue: [found := true].
		((self isLocalName: n) not
		 and: [(classScope indexOfInstVar: n) notNil]) ifTrue: [found := true]].
	"NLR needs the home; the VM also reaches self through captured 0 only
	 when referenced — returns alone don't force a self capture."
	^found! !

!StCodeGen methodsFor: 'expressions'!
compileExpr: n to: dest
	(n isKindOf: StLiteralNode) ifTrue: [^self compileLiteral: n to: dest].
	(n isKindOf: StVariableNode) ifTrue: [^self compileVariable: n name to: dest].
	(n isKindOf: StAssignmentNode) ifTrue: [^self compileAssignment: n to: dest].
	(n isKindOf: StMessageNode) ifTrue: [^self compileMessage: n to: dest].
	(n isKindOf: StCascadeNode) ifTrue: [^self compileCascade: n to: dest].
	(n isKindOf: StBlockNode) ifTrue: [^self compileBlock: n to: dest].
	self error: 'cannot compile node ' , n printString!

compileLiteral: n to: dest
	| v |
	self noteSp: dest.
	v := n value.
	n kind = #integer ifTrue: [
		(v >= -32768 and: [v <= 32767]) ifTrue: [
			^self emit: (Array with: 'LOADINT' with: dest with: v)].
		^self emit: (Array with: 'LOADK' with: dest with: (self literalIndex: v))].
	n kind = #byteArray ifTrue: [
		^self emit: (Array with: 'LOADK' with: dest
			with: (self literalIndex: (Array with: #byteArray with: v asArray)))].
	^self emit: (Array with: 'LOADK' with: dest with: (self literalIndex: v))!

compileVariable: name to: dest
	| r |
	self noteSp: dest.
	r := self resolve: name.
	r first = #self ifTrue: [
		isBlock
			ifTrue: [^self emit: (Array with: 'GETIVAR' with: dest with: 4)]
			ifFalse: [^self emit: (Array with: 'LOADSELF' with: dest)]].
	r first = #pseudo ifTrue: [
		(r at: 2) = #true ifTrue: [^self emit: (Array with: 'LOADTRUE' with: dest)].
		(r at: 2) = #false ifTrue: [^self emit: (Array with: 'LOADFALSE' with: dest)].
		^self emit: (Array with: 'LOADNIL' with: dest)].
	r first = #slot ifTrue: [
		(r at: 3)
			ifTrue: [^self emit: (Array with: 'GETBOX' with: dest with: (r at: 2))].
		(r at: 2) = dest ifTrue: [^self].
		^self emit: (Array with: 'MOVE' with: dest with: (r at: 2))].
	r first = #captured ifTrue: [
		self emit: (Array with: 'GETIVAR' with: dest with: 4 + (r at: 2)).
		(r at: 3) ifTrue: [
			self emit: (Array with: 'GETBOX' with: dest with: dest)].
		^self].
	r first = #ivar ifTrue: [
		isBlock ifTrue: [^self compileBlockIvarRead: (r at: 2) to: dest].
		^self emit: (Array with: 'GETIVAR' with: dest with: (r at: 2))].
	r first = #global ifTrue: [
		"global/classvar: literal Association, read via a #value send"
		| assocIdx r2 |
		r2 := sp + 4.
		self noteSp: r2.
		assocIdx := self literalIndex: (StGlobalRef named: name).
		self emit: (Array with: 'LOADK' with: r2 with: assocIdx).
		self emit: (Array with: 'SEND' with: dest with: r2
			with: (self siteIndex: #value argc: 0 super: false)).
		^self].
	self error: 'unresolvable name ' , name!

compileBlockIvarRead: ivarIndex to: dest
	"inside a block: self instVarAt: ivarIndex+1"
	| r |
	r := sp + 4.
	self noteSp: r + 1.
	self emit: (Array with: 'GETIVAR' with: r with: 4). "captured self"
	self emit: (Array with: 'LOADINT' with: r + 1 with: ivarIndex + 1).
	self emit: (Array with: 'SEND' with: dest with: r
		with: (self siteIndex: #instVarAt: argc: 1 super: false))!

compileAssignment: n to: dest
	| r |
	r := self resolve: n variable.
	r first = #slot ifTrue: [
		(r at: 3)
			ifTrue: [
				self compileExpr: n expression to: dest.
				self emit: (Array with: 'SETBOX' with: (r at: 2) with: dest).
				^self]
			ifFalse: [
				self compileExpr: n expression to: (r at: 2).
				(r at: 2) = dest ifFalse: [
					self noteSp: dest.
					self emit: (Array with: 'MOVE' with: dest with: (r at: 2))].
				^self]].
	r first = #captured ifTrue: [
		"captured mutables are boxed by construction"
		| boxSlot |
		(r at: 3) ifFalse: [self error: 'assignment to copy-captured ' , n variable].
		self compileExpr: n expression to: dest.
		boxSlot := sp + 4.
		self noteSp: boxSlot.
		self emit: (Array with: 'GETIVAR' with: boxSlot with: 4 + (r at: 2)).
		self emit: (Array with: 'SETBOX' with: boxSlot with: dest).
		^self].
	r first = #ivar ifTrue: [
		self compileExpr: n expression to: dest.
		isBlock ifTrue: [^self compileBlockIvarWrite: (r at: 2) from: dest].
		self emit: (Array with: 'SETIVAR' with: (r at: 2) with: dest).
		^self].
	r first = #global ifTrue: [
		| assocIdx r2 |
		self compileExpr: n expression to: dest.
		r2 := sp + 4.
		self noteSp: r2 + 1.
		assocIdx := self literalIndex: (StGlobalRef named: n variable).
		self emit: (Array with: 'LOADK' with: r2 with: assocIdx).
		self emit: (Array with: 'MOVE' with: r2 + 1 with: dest).
		self emit: (Array with: 'SEND' with: r2 with: r2
			with: (self siteIndex: #value: argc: 1 super: false)).
		^self].
	self error: 'cannot assign to ' , n variable!

compileBlockIvarWrite: ivarIndex from: valueSlot
	| r |
	r := sp + 4.
	self noteSp: r + 2.
	self emit: (Array with: 'GETIVAR' with: r with: 4).
	self emit: (Array with: 'LOADINT' with: r + 1 with: ivarIndex + 1).
	self emit: (Array with: 'MOVE' with: r + 2 with: valueSlot).
	self emit: (Array with: 'SEND' with: r with: r
		with: (self siteIndex: #instVarAt:put: argc: 2 super: false))! !

!StCodeGen methodsFor: 'messages'!
specializedOpFor: selector argc: n
	n = 1 ifTrue: [
		selector = #+ ifTrue: [^'ADD'].
		selector = #- ifTrue: [^'SUB'].
		selector = #* ifTrue: [^'MUL'].
		selector = #// ifTrue: [^'DIV'].
		selector = #\\ ifTrue: [^'MOD'].
		selector = #< ifTrue: [^'LT'].
		selector = #> ifTrue: [^'GT'].
		selector = #<= ifTrue: [^'LE'].
		selector = #>= ifTrue: [^'GE'].
		selector = #= ifTrue: [^'EQNUM'].
		selector = #== ifTrue: [^'IDEQ'].
		selector = #at: ifTrue: [^'AT']].
	n = 2 ifTrue: [
		selector = #at:put: ifTrue: [^'ATPUT']].
	n = 0 ifTrue: [
		selector = #size ifTrue: [^'SIZE'].
		selector = #class ifTrue: [^'CLASSOF'].
		selector = #not ifTrue: [^'NOT']].
	^nil!

isInlinableBlock: n
	^(n isKindOf: StBlockNode)
		and: [n arguments isEmpty and: [n temporaries isEmpty]]!

compileMessage: n to: dest
	| args sel kind |
	sel := n selector.
	args := n arguments.
	"control-flow macros on literal argless blocks (§12)"
	kind := self macroKindOf: n.
	kind = #ifTrueIfFalse
		ifTrue: [^self compileIf: n receiver then: (args at: 1) else: (args at: 2) to: dest].
	kind = #ifFalseIfTrue
		ifTrue: [^self compileIf: n receiver then: (args at: 2) else: (args at: 1) to: dest].
	kind = #ifTrue
		ifTrue: [^self compileIf: n receiver then: (args at: 1) else: nil to: dest].
	kind = #ifFalse
		ifTrue: [^self compileIf: n receiver then: nil else: (args at: 1) to: dest].
	kind = #and
		ifTrue: [^self compileAnd: n receiver with: (args at: 1) to: dest].
	kind = #or
		ifTrue: [^self compileOr: n receiver with: (args at: 1) to: dest].
	kind = #while
		ifTrue: [^self compileWhile: n receiver body: (args at: 1)
			until: (sel = #whileFalse:) to: dest].
	kind = #whileNoBody
		ifTrue: [^self compileWhile: n receiver body: nil
			until: (sel = #whileFalse) to: dest].
	kind = #toDo
		ifTrue: [^self compileToDo: n to: dest].

	n isSuperSend ifTrue: [^self compileSuperSend: n to: dest].

	"isNil/notNil are sealed selectors of the dialect (docs/codegen-plan.md):
	 nil tests are identity tests, no send"
	(args isEmpty and: [sel = #isNil or: [sel = #notNil]]) ifTrue: [
		^self compileNilTest: n negated: sel = #notNil to: dest].

	"specialized sends (§12): unconditional for the sealed selectors"
	(self specializedOpFor: sel argc: args size) notNil ifTrue: [
		^self compileSpecialized: n to: dest].

	^self compileGeneralSend: n to: dest!

compileSpecialized: n to: dest
	| op base savedSp |
	op := self specializedOpFor: n selector argc: n arguments size.
	savedSp := sp.
	base := sp.
	self noteSp: base + n arguments size.
	self compileExpr: n receiver to: base.
	sp := base + 1.
	1 to: n arguments size do: [:i |
		self compileExpr: (n arguments at: i) to: base + i.
		sp := base + 1 + i].
	sp := savedSp.
	self noteSp: dest.
	(op = 'SIZE' or: [op = 'CLASSOF' or: [op = 'NOT']])
		ifTrue: [^self emit: (Array with: op with: dest with: base)].
	op = 'ATPUT'
		ifTrue: [^self emit: (Array with: op with: dest with: base with: base + 1)].
	self emit: (Array with: op with: dest with: base with: base + 1)!

compileNilTest: n negated: neg to: dest
	| base savedSp |
	savedSp := sp.
	base := sp.
	self noteSp: base + 1.
	self compileExpr: n receiver to: base.
	sp := base + 1.
	self emit: (Array with: 'LOADNIL' with: base + 1).
	sp := savedSp.
	self noteSp: dest.
	self emit: (Array with: 'IDEQ' with: dest with: base with: base + 1).
	neg ifTrue: [self emit: (Array with: 'NOT' with: dest with: dest)]!

compileGeneralSend: n to: dest
	| r savedSp |
	savedSp := sp.
	r := sp + 4.
	self noteSp: r + n arguments size.
	self compileExpr: n receiver to: r.
	sp := r + 1.
	1 to: n arguments size do: [:i |
		self compileExpr: (n arguments at: i) to: r + i.
		sp := r + 1 + i].
	sp := savedSp.
	self noteSp: dest.
	self emit: (Array with: 'SEND' with: dest with: r
		with: (self siteIndex: n selector argc: n arguments size super: false))!

compileSuperSend: n to: dest
	| r savedSp |
	savedSp := sp.
	r := sp + 4.
	self noteSp: r + n arguments size.
	"super's value is self"
	isBlock
		ifTrue: [self emit: (Array with: 'GETIVAR' with: r with: 4)]
		ifFalse: [self emit: (Array with: 'LOADSELF' with: r)].
	sp := r + 1.
	1 to: n arguments size do: [:i |
		self compileExpr: (n arguments at: i) to: r + i.
		sp := r + 1 + i].
	sp := savedSp.
	self noteSp: dest.
	self emit: (Array with: 'SENDSUPER' with: dest with: r
		with: (self siteIndex: n selector argc: n arguments size super: true))!

compileCascade: n to: dest
	| rcv savedSp |
	savedSp := sp.
	rcv := sp.
	self compileExpr: n receiver to: rcv.
	sp := rcv + 1.
	1 to: n messages size do: [:mi |
		| m target |
		m := n messages at: mi.
		target := mi = n messages size ifTrue: [dest] ifFalse: [sp].
		self compileOneCascadeMessage: m receiverSlot: rcv to: target].
	sp := savedSp.
	self noteSp: dest!

compileOneCascadeMessage: m receiverSlot: rcv to: dest
	| r savedSp op |
	savedSp := sp.
	op := self specializedOpFor: m selector argc: m arguments size.
	op notNil
		ifTrue: [
			| base |
			base := sp.
			self noteSp: base + m arguments size.
			self emit: (Array with: 'MOVE' with: base with: rcv).
			sp := base + 1.
			1 to: m arguments size do: [:i |
				self compileExpr: (m arguments at: i) to: base + i.
				sp := base + 1 + i].
			sp := savedSp.
			self noteSp: dest.
			(m arguments size = 0)
				ifTrue: [self emit: (Array with: op with: dest with: base)]
				ifFalse: [self emit: (Array with: op with: dest with: base with: base + 1)].
			^self].
	r := sp + 4.
	self noteSp: r + m arguments size.
	self emit: (Array with: 'MOVE' with: r with: rcv).
	sp := r + 1.
	1 to: m arguments size do: [:i |
		self compileExpr: (m arguments at: i) to: r + i.
		sp := r + 1 + i].
	sp := savedSp.
	self noteSp: dest.
	self emit: (Array with: 'SEND' with: dest with: r
		with: (self siteIndex: m selector argc: m arguments size super: false))! !

!StCodeGen methodsFor: 'control macros'!
compileIf: cond then: thenBlk else: elseBlk to: dest
	| c jumpToElse jumpToEnd |
	c := sp.
	self compileExpr: cond to: c.
	jumpToElse := self emitPlaceholder.
	thenBlk isNil
		ifTrue: [self emit: (Array with: 'LOADNIL' with: dest)]
		ifFalse: [self compileInlineBlock: thenBlk to: dest].
	jumpToEnd := self emitPlaceholder.
	self patch: jumpToElse
		with: (Array with: 'JUMPFALSE' with: c with: insns size - jumpToElse).
	elseBlk isNil
		ifTrue: [self emit: (Array with: 'LOADNIL' with: dest)]
		ifFalse: [self compileInlineBlock: elseBlk to: dest].
	self patch: jumpToEnd
		with: (Array with: 'JUMP' with: insns size - jumpToEnd)!

compileAnd: recv with: blk to: dest
	| jumpFalse jumpEnd |
	self compileExpr: recv to: dest.
	jumpFalse := self emitPlaceholder.
	self compileInlineBlock: blk to: dest.
	jumpEnd := self emitPlaceholder.
	self patch: jumpFalse
		with: (Array with: 'JUMPFALSE' with: dest with: insns size - jumpFalse).
	self emit: (Array with: 'LOADFALSE' with: dest).
	self patch: jumpEnd with: (Array with: 'JUMP' with: insns size - jumpEnd)!

compileOr: recv with: blk to: dest
	| jumpTrue jumpEnd |
	self compileExpr: recv to: dest.
	jumpTrue := self emitPlaceholder.
	self compileInlineBlock: blk to: dest.
	jumpEnd := self emitPlaceholder.
	self patch: jumpTrue
		with: (Array with: 'JUMPTRUE' with: dest with: insns size - jumpTrue).
	self emit: (Array with: 'LOADTRUE' with: dest).
	self patch: jumpEnd with: (Array with: 'JUMP' with: insns size - jumpEnd)!

compileWhile: condBlk body: bodyBlk until: isUntil to: dest
	| loopStart c exitJump |
	loopStart := insns size.
	c := sp.
	self compileInlineBlock: condBlk to: c.
	exitJump := self emitPlaceholder.
	bodyBlk notNil ifTrue: [self compileInlineBlock: bodyBlk to: sp].
	self emit: (Array with: 'JUMP' with: loopStart - insns size - 1).
	self patch: exitJump
		with: (Array
			with: (isUntil ifTrue: ['JUMPTRUE'] ifFalse: ['JUMPFALSE'])
			with: c
			with: insns size - exitJump).
	self noteSp: dest.
	self emit: (Array with: 'LOADNIL' with: dest)!

compileToDo: n to: dest
	"inline X to: Y do: [:v | ...] (docs/codegen-plan.md): the loop
	 variable lives in a frame slot; LE/ADD fall through to real #<=/#+
	 sends for non-SmallInteger values, preserving Number semantics.
	 Answers the receiver, like the kernel fallback."
	| savedSp recvSlot iSlot limitSlot t body loopStart exitJump |
	body := n arguments at: 2.
	savedSp := sp.
	recvSlot := sp.
	iSlot := sp + 1.
	limitSlot := sp + 2.
	t := sp + 3.
	self noteSp: t.
	self compileExpr: n receiver to: recvSlot.
	sp := recvSlot + 1.
	self compileExpr: (n arguments at: 1) to: limitSlot.
	self emit: (Array with: 'MOVE' with: iSlot with: recvSlot).
	sp := t.
	loopStart := insns size.
	self emit: (Array with: 'LE' with: t with: iSlot with: limitSlot).
	exitJump := self emitPlaceholder.
	loopBindings add: (Array with: body arguments first with: iSlot).
	self compileToDoBody: body statements.
	loopBindings removeLast.
	self emit: (Array with: 'LOADINT' with: t with: 1).
	self emit: (Array with: 'ADD' with: iSlot with: iSlot with: t).
	self emit: (Array with: 'JUMP' with: loopStart - insns size - 1).
	self patch: exitJump
		with: (Array with: 'JUMPFALSE' with: t with: insns size - exitJump).
	sp := savedSp.
	self noteSp: dest.
	dest = recvSlot ifFalse: [
		self emit: (Array with: 'MOVE' with: dest with: recvSlot)]!

compileToDoBody: stmts
	"all body statements are effect-position (to:do: discards the block's
	 value); a ^ ends the list (the rest is unreachable)"
	stmts do: [:stmt |
		(stmt isKindOf: StReturnNode)
			ifTrue: [
				self compileReturn: stmt.
				^self].
		self compileExpr: stmt to: sp]!

compileInlineBlock: blk to: dest
	"inline an argless, templess literal block's statements in the
	 current scope; value = last statement (nil when empty)"
	| stmts |
	stmts := blk statements.
	stmts isEmpty ifTrue: [
		self noteSp: dest.
		^self emit: (Array with: 'LOADNIL' with: dest)].
	1 to: stmts size do: [:i |
		| stmt |
		stmt := stmts at: i.
		(stmt isKindOf: StReturnNode)
			ifTrue: [
				self compileReturn: stmt.
				^self]
			ifFalse: [
				self compileExpr: stmt to: (i = stmts size ifTrue: [dest] ifFalse: [sp])]]! !

!StCodeGen methodsFor: 'blocks'!
compileBlock: blockNode to: dest
	"MKCLOSURE must be followed IMMEDIATELY by its CAPTUREs (§6), so any
	 capture source that needs a load (outer-block captures, block self)
	 is staged into scratch slots first"
	| free needsSelf blockGen spec litIdx plan stage |
	free := self freeLocalNamesOf: blockNode.
	needsSelf := (self blockUsesSelfOrIvars: blockNode)
		or: [isBlock and: [self nestedNeedsSelf: blockNode]].
	blockGen := StCodeGen new.
	blockGen setBlock: blockNode parent: self
		captures: free capturesSelf: needsSelf.
	spec := blockGen compile.
	litIdx := self blockLiteralIndex: spec.
	"plan: (captureIndex sourceSlot) pairs, staging loads first"
	plan := OrderedCollection new.
	stage := sp.
	dest = stage ifTrue: [stage := stage + 1].
	needsSelf ifTrue: [
		isBlock
			ifTrue: [
				self noteSp: stage.
				self emit: (Array with: 'GETIVAR' with: stage with: 4).
				plan add: (Array with: 0 with: stage).
				stage := stage + 1.
				dest = stage ifTrue: [stage := stage + 1]]
			ifFalse: [plan add: (Array with: 0 with: 0)]].
	1 to: free size do: [:i |
		| name idx r |
		name := free at: i.
		idx := (needsSelf ifTrue: [1] ifFalse: [0]) + i - 1.
		r := self resolve: name.
		r first = #slot
			ifTrue: [plan add: (Array with: idx with: (r at: 2))]
			ifFalse: [
				r first = #captured
					ifFalse: [self error: 'bad capture source for ' , name].
				self noteSp: stage.
				self emit: (Array with: 'GETIVAR' with: stage with: 4 + (r at: 2)).
				plan add: (Array with: idx with: stage).
				stage := stage + 1.
				dest = stage ifTrue: [stage := stage + 1]]].
	self noteSp: dest.
	self emit: (Array with: 'MKCLOSURE' with: dest with: litIdx).
	plan do: [:pair |
		self emit: (Array with: 'CAPTURE' with: (pair at: 1) with: (pair at: 2))]!

nestedNeedsSelf: blockNode
	"a nested block that itself needs self forces this block to capture it"
	^self blockUsesSelfOrIvars: blockNode! !
