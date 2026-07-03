"The in-image JIT differential selftest (JIT.md §18.2, M2 exit criterion):
 every case runs interpreted, then all of JitTestSubject's methods (and
 their blocks) are compiled and the case runs again — results must match.
 The VM counters prove native code actually executed.

 Filed into the JIT test image after the kernel and the JIT compiler
 sources; driven by 'JitSelftest run' as the program."

Object subclass: #JitTestSubject
	instanceVariableNames: 'alpha beta'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-JIT-Tests'!

!JitTestSubject methodsFor: 'setup'!
prime
	alpha := 3.
	beta := 4.
	^self! !

!JitTestSubject methodsFor: 'cases'!
sumIvars
	^alpha + beta!

setIvars
	alpha := alpha + 1.
	beta := beta + alpha.
	^beta!

add: x to: y
	^x + y!

mixedArith: n
	^n * 3 - (n // 2) + (n \\ 7)!

overflowAdd
	"the ADD fast path must fall back identically in both tiers (the
	 kernel signals ArithmeticOverflow; what matters is that compiled
	 and interpreted agree)"
	^[16r3FFFFFFFFFFFFFFF + 1] on: ArithmeticOverflow do: [:e | -999]!

compares: n
	| c |
	c := 0.
	n < 10 ifTrue: [c := c + 1].
	n > 10 ifTrue: [c := c + 2].
	n <= 10 ifTrue: [c := c + 4].
	n >= 10 ifTrue: [c := c + 8].
	n = 10 ifTrue: [c := c + 16].
	n == 10 ifTrue: [c := c + 32].
	^c!

loopSum: n
	| s |
	s := 0.
	1 to: n do: [:i | s := s + i].
	^s!

whileSum: n
	| s i |
	s := 0.
	i := 0.
	[i < n] whileTrue: [i := i + 1. s := s + i].
	^s!

arrayOps: n
	| a s |
	a := Array new: n.
	1 to: n do: [:i | a at: i put: i * i].
	s := 0.
	1 to: a size do: [:i | s := s + (a at: i)].
	^s!

byteOps
	| b s |
	b := ByteArray new: 8.
	1 to: 8 do: [:i | b at: i put: i * 7].
	s := 0.
	1 to: b size do: [:i | s := s + (b at: i)].
	^s!

classOfCase: x
	x class == SmallInteger ifTrue: [^1].
	x class == Array ifTrue: [^2].
	^0!

notCase: b
	^b not!

idEqCase: x
	^(x == nil) not!

boxCase
	| x blk |
	x := 1.
	blk := [:y | x := x + y. x].
	blk value: 5.
	blk value: 7.
	^x!

sendCase: n
	^(self add: n to: n) + (self mixedArith: n)!

fib: n
	n < 2 ifTrue: [^n].
	^(self fib: n - 1) + (self fib: n - 2)!

helperWith: aBlock
	1 to: 10 do: [:i | aBlock value: i].
	^0!

nlrCase: n
	self helperWith: [:x | x * x > n ifTrue: [^x * 10]].
	^-1!

ensureCase
	| acc |
	acc := 0.
	[acc := acc + 1] ensure: [acc := acc + 10].
	[[self error: 'boom'] ensure: [acc := acc + 100]]
		on: Error do: [:e | acc := acc + 1000].
	^acc!

exceptionCase: n
	^[n < 0
		ifTrue: [Error new signal: 'neg']
		ifFalse: [n * 2]]
		on: Error do: [:e | -7]!

blockChain: n
	| mk c1 c2 |
	mk := [:start | | acc | acc := start. [:inc | acc := acc + inc. acc]].
	c1 := mk value: n.
	c2 := mk value: n * 2.
	c1 value: 1.
	c2 value: 2.
	^(c1 value: 3) + (c2 value: 4)!

pingPong
	"semaphore ping-pong across processes (both sides run this class's
	 compiled code in pass 2 — the scheduler battery's cross-tier case)"
	| semA semB log i |
	semA := Semaphore new.
	semB := Semaphore new.
	log := 0.
	[1 to: 3 do: [:k |
		semA wait.
		log := log * 10 + k.
		semB signal]] fork.
	i := 0.
	[i < 3] whileTrue: [
		i := i + 1.
		log := log * 10 + 7.
		semA signal.
		semB wait].
	^log! !

Object subclass: #JitSelftest
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-JIT-Tests'!

!JitSelftest class methodsFor: 'support'!
methodsOf: aClass
	"every CompiledMethod in the class's dictionary"
	| md vals out |
	md := aClass instVarAt: Treaty behaviorMethodDictionary + 1.
	out := OrderedCollection new.
	md isNil ifTrue: [^out].
	vals := md instVarAt: Treaty mdictValues + 1.
	1 to: vals size do: [:i | out add: (vals at: i)].
	^out!

compileAllOf: aClass
	"compile every method and every block literal, counting successes"
	| n |
	n := 0.
	(self methodsOf: aClass) do: [:m |
		(StJITCompiler compileNow: m) notNil ifTrue: [n := n + 1].
		n := n + (self compileBlocksOf: m)].
	^n!

compileBlocksOf: aMethod
	| lits n v |
	lits := aMethod instVarAt: Treaty methodLiterals + 1.
	n := 0.
	lits isNil ifTrue: [^0].
	1 to: lits size do: [:i |
		v := lits at: i.
		(v notNil and: [v class == CompiledBlock]) ifTrue: [
			(StJITCompiler compileNow: v) notNil ifTrue: [n := n + 1].
			n := n + (self compileBlocksOf: v)]].
	^n!

counterNamed: aName
	"a row from primVmCounters, or 0"
	| rows row |
	rows := Profiler primCounters.
	1 to: rows size do: [:i |
		row := rows at: i.
		(row at: 1) = aName ifTrue: [^row at: 2]].
	^0!

cases
	"each: selector + arguments"
	| c |
	c := OrderedCollection new.
	c add: (Array with: #sumIvars with: (Array new: 0)).
	c add: (Array with: #setIvars with: (Array new: 0)).
	c add: (Array with: #add:to: with: (Array with: 20 with: 22)).
	c add: (Array with: #mixedArith: with: (Array with: 123)).
	c add: (Array with: #overflowAdd with: (Array new: 0)).
	c add: (Array with: #compares: with: (Array with: 10)).
	c add: (Array with: #compares: with: (Array with: 3)).
	c add: (Array with: #loopSum: with: (Array with: 100)).
	c add: (Array with: #whileSum: with: (Array with: 50)).
	c add: (Array with: #arrayOps: with: (Array with: 12)).
	c add: (Array with: #byteOps with: (Array new: 0)).
	c add: (Array with: #classOfCase: with: (Array with: 5)).
	c add: (Array with: #classOfCase: with: (Array with: (Array new: 1))).
	c add: (Array with: #notCase: with: (Array with: true)).
	c add: (Array with: #notCase: with: (Array with: false)).
	c add: (Array with: #idEqCase: with: (Array with: 7)).
	c add: (Array with: #boxCase with: (Array new: 0)).
	c add: (Array with: #sendCase: with: (Array with: 9)).
	c add: (Array with: #fib: with: (Array with: 12)).
	c add: (Array with: #nlrCase: with: (Array with: 20)).
	c add: (Array with: #nlrCase: with: (Array with: 1000)).
	c add: (Array with: #ensureCase with: (Array new: 0)).
	c add: (Array with: #exceptionCase: with: (Array with: 21)).
	c add: (Array with: #exceptionCase: with: (Array with: -3)).
	c add: (Array with: #blockChain: with: (Array with: 5)).
	c add: (Array with: #pingPong with: (Array new: 0)).
	^c! !

!JitSelftest class methodsFor: 'running'!
run
	| failures interpreted compiled subj entersBefore entersAfter nCompiled stats hits info |
	"no background tiering during the differential run; profiling level
	 1 so compiled sends count inline-cache hits (M5)"
	StJIT threshold: 1000000.
	StJIT control: Treaty jitctlSetProfilingLevel with: 1.
	failures := 0.

	"pass 1: interpreted"
	interpreted := OrderedCollection new.
	self cases do: [:case |
		subj := JitTestSubject new prime.
		interpreted add: (subj perform: (case at: 1) withArguments: (case at: 2))].

	"compile the whole subject class (and its blocks) — plus the block
	 machinery and SmallInteger, so value-primitives (tail-replacing
	 framed activation) and PRIM-with-fallback methods run compiled too"
	nCompiled := self compileAllOf: JitTestSubject.
	nCompiled := nCompiled + (self compileAllOf: BlockClosure).
	nCompiled := nCompiled + (self compileAllOf: SmallInteger).
	Transcript showCr: 'compiled ' , nCompiled printString , ' methods'.
	nCompiled < 10 ifTrue: [
		Transcript showCr: 'FAIL: too few methods compiled'.
		failures := failures + 1].

	"pass 2: compiled; results must be identical"
	entersBefore := self counterNamed: 'jit.enters'.
	compiled := OrderedCollection new.
	self cases do: [:case |
		subj := JitTestSubject new prime.
		compiled add: (subj perform: (case at: 1) withArguments: (case at: 2))].
	entersAfter := self counterNamed: 'jit.enters'.

	1 to: interpreted size do: [:i |
		(interpreted at: i) = (compiled at: i) ifFalse: [
			failures := failures + 1.
			Transcript showCr: 'FAIL case ' , i printString
				, ': interpreted=' , (interpreted at: i) printString
				, ' compiled=' , (compiled at: i) printString]].

	entersAfter > entersBefore ifFalse: [
		failures := failures + 1.
		Transcript showCr: 'FAIL: no native execution happened'].
	Transcript showCr: 'native enters: '
		, (entersAfter - entersBefore) printString.

	"M5: the per-site inline-cache statistics must have counted hits
	 (interpreter hits always; compiled hits at profiling level 1)"
	stats := StJITProfiler cacheStatsOf: JitTestSubject.
	hits := 0.
	stats do: [:row | hits := hits + (row at: 2)].
	hits > 0 ifFalse: [
		failures := failures + 1.
		Transcript showCr: 'FAIL: no inline-cache hits counted'].
	Transcript showCr: 'cache hits counted: ' , hits printString.

	"M5: primJITCodeInfo reports on installed code"
	info := StJIT codeInfo: ((self methodsOf: JitTestSubject) at: 1).
	(info at: 2) > 0 ifFalse: [
		failures := failures + 1.
		Transcript showCr: 'FAIL: codeInfo has no code length'].

	failures = 0
		ifTrue: [Transcript showCr: 'JIT-SELFTEST-PASSED']
		ifFalse: [Transcript showCr: 'JIT-SELFTEST-FAILED ('
			, failures printString , ')']! !
