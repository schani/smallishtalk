| methods md vals t0 t1 n reps |
"compile-speed metric (JIT.md M6): compile a representative method set
 repeatedly with the compiler itself hot (background tiering on), and
 report microseconds per method"
StJIT threshold: 2.
StJIT startBackgroundCompiler.
methods := OrderedCollection new.
(Array with: SmallInteger with: BlockClosure with: OrderedCollection) do: [:cls |
	md := cls instVarAt: Treaty behaviorMethodDictionary + 1.
	md notNil ifTrue: [
		vals := md instVarAt: Treaty mdictValues + 1.
		1 to: vals size do: [:i | methods add: (vals at: i)]]].
"warm: compile everything twice (tiers the compiler up along the way)"
methods do: [:m | StJITCompiler compileNow: m].
methods do: [:m | StJITCompiler compileNow: m].
StJITCompiler drain.
"timed: five rounds"
reps := 5.
t0 := Profiler primClockMs.
1 to: reps do: [:r |
	methods do: [:m | StJITCompiler compileNow: m]].
t1 := Profiler primClockMs.
n := methods size * reps.
Transcript showCr: 'MS ' , (t1 - t0) printString.
Transcript showCr: 'METHODS ' , n printString.
Transcript showCr: 'USPERMETHOD ' , ((t1 - t0) * 1000 // n) printString.
Transcript showCr: 'CHECK ' , n printString
