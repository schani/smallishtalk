"The in-image face of the JIT (JIT.md §3, Annex J.6): the primitive
 wrappers and the compilation driver. StJITCompiler compileNow: is the
 test path; the background JIT process (M4) drains the queue through the
 same entry point."

Object subclass: #StJIT
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-JIT'!

!StJIT class methodsFor: 'primitives'!
nextRequest
	"the oldest queued compilation request, or nil"
	<primitive: 430>
	^self primitiveFailed: #nextRequest!

queueSize
	<primitive: 431>
	^self primitiveFailed: #queueSize!

install: aMethod code: codeBytes maps: mapBytes
	"install machine code for the method; answers the handle index"
	<primitive: 432>
	^self primitiveFailed: #install:code:maps:!

control: op with: arg
	<primitive: 433>
	^self primitiveFailed: #control:with:!

codeInfo: aMethod
	"{handle. codeLen. entryOff. reentryCount. retptsCount. patchCount}"
	<primitive: 435>
	^self primitiveFailed: #codeInfo:!

threshold
	^self control: Treaty jitctlGetThreshold with: 0!

threshold: n
	^self control: Treaty jitctlSetThreshold with: n!

enable: aBoolean
	^self control: Treaty jitctlSetEnabled with: (aBoolean ifTrue: [1] ifFalse: [0])!

flushAll
	^self control: Treaty jitctlFlushAll with: 0!

semaphore
	"the Treaty jitSemaphore the counter-trip path signals"
	^self control: Treaty jitctlGetSemaphore with: 0!

markDoNotCompile: aMethod
	^self control: Treaty jitctlMarkDnc with: aMethod! !

!StJIT class methodsFor: 'background compilation'!
startBackgroundCompiler
	"JIT.md §3: the JIT process — an ordinary green thread at high
	 priority, blocked on jitSemaphore, draining the VM's request queue
	 through the in-image compiler. Answers the process."
	| sem |
	sem := self semaphore.
	^[[true] whileTrue: [
		sem wait.
		StJITCompiler drain]] forkAt: 7! !

Object subclass: #StJITCompiler
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-JIT'!

!StJITCompiler class methodsFor: 'compiling'!
compile: aMethod
	"translate and install; answers the handle index, or nil after
	 marking an uncompilable method do-not-compile (so it never trips
	 again). Compilation is pure (§14); installation is the primitive."
	| pair |
	pair := StJITMethodCompiler compile: aMethod
		level: (StJIT control: Treaty jitctlGetProfilingLevel with: 0).
	pair isNil ifTrue: [
		StJIT markDoNotCompile: aMethod.
		^nil].
	^StJIT install: aMethod code: (pair at: 1) maps: (pair at: 2)!

compileNow: aMethod
	"the synchronous test path (§3 warm-up semantics: tests that must
	 assert tier use this + drain)"
	^self compile: aMethod!

drain
	"the image-side barrier: compile the methods queued as of now. It
	 processes a bounded snapshot (the queue's current size) rather than
	 chasing its tail — compiling a method activates compiler methods
	 that themselves trip and re-enqueue, so an unbounded drain would
	 never terminate on the self-compile workload. Newly-queued methods
	 are picked up by the next drain / semaphore signal."
	| n m |
	n := StJIT queueSize.
	[n > 0] whileTrue: [
		m := StJIT nextRequest.
		m isNil ifTrue: [^self].
		self compile: m.
		n := n - 1]! !
