"The in-image profiler face of the JIT (JIT.md §17): reports over the
 VM's exact counters and per-site inline-cache statistics. Samples are
 taken by the VM at safepoints (both tiers report bytecode pcs); the
 flat report flows through the existing Profiler primitives, and this
 class adds the JIT-specific views: tier residency (the JIT's own
 effectiveness meter) and the per-site cache-hit tables (the direct
 input for deciding v2 PICs — the megamorphic-site finder)."

Object subclass: #StJITProfiler
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-JIT'!

!StJITProfiler class methodsFor: 'counters'!
counterNamed: aName
	| rows row |
	rows := Profiler primCounters.
	1 to: rows size do: [:i |
		row := rows at: i.
		(row at: 1) = aName ifTrue: [^row at: 2]].
	^0!

tierResidency
	"Array with: nativeSamples with: interpreterSamples"
	^Array with: (self counterNamed: 'jit.samples_native')
		with: (self counterNamed: 'jit.samples_interp')!

nativeSampleFraction
	"percent of samples that landed in compiled code (0 when unsampled)"
	| t |
	t := self tierResidency.
	((t at: 1) + (t at: 2)) = 0 ifTrue: [^0].
	^(t at: 1) * 100 // ((t at: 1) + (t at: 2))! !

!StJITProfiler class methodsFor: 'cache statistics'!
siteStatsOf: aMethod
	"OrderedCollection of (Array with: selector with: hits with: misses)
	 per send site, from the site table's counters word (JIT.md §16)"
	| sites out nsites base sel counters hits misses |
	out := OrderedCollection new.
	sites := aMethod instVarAt: Treaty methodSendSites + 1.
	sites isNil ifTrue: [^out].
	nsites := sites size // Treaty siteStride.
	1 to: nsites do: [:i |
		base := (i - 1) * Treaty siteStride.
		sel := sites at: base + Treaty siteSelector + 1.
		counters := sites at: base + Treaty siteCounters + 1.
		counters isNil ifTrue: [counters := 0].
		hits := counters bitAnd: 16r7FFFFFFF.
		misses := (counters bitShift: -31) bitAnd: 16r7FFFFFFF.
		out add: (Array with: sel with: hits with: misses)].
	^out!

cacheStatsOf: aClass
	"every send site of every method of the class"
	| md vals out |
	out := OrderedCollection new.
	md := aClass instVarAt: Treaty behaviorMethodDictionary + 1.
	md isNil ifTrue: [^out].
	vals := md instVarAt: Treaty mdictValues + 1.
	1 to: vals size do: [:i |
		(self siteStatsOf: (vals at: i)) do: [:row | out add: row]].
	^out!

megamorphicSitesOf: aClass threshold: missCount
	"sites whose misses exceed the threshold — v2 PIC candidates"
	| out |
	out := OrderedCollection new.
	(self cacheStatsOf: aClass) do: [:row |
		(row at: 3) > missCount ifTrue: [out add: row]].
	^out! !

!StJITProfiler class methodsFor: 'reports'!
printTierResidency
	| t |
	t := self tierResidency.
	Transcript showCr: 'tier residency: native ' , (t at: 1) printString
		, ' / interpreted ' , (t at: 2) printString
		, ' (' , self nativeSampleFraction printString , '% native)'!

printCacheStatsOf: aClass
	(self cacheStatsOf: aClass) do: [:row |
		Transcript showCr: (row at: 1) printString
			, ' hits ' , (row at: 2) printString
			, ' misses ' , (row at: 3) printString]! !
