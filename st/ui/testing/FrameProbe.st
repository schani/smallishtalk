"UI instrumentation (UI.md §13): the per-frame probe wired into the UI loop.
 It keeps a hard split between two kinds of metric:
   - TIMING (per-phase nanoseconds via primClockMonotonicNs): wall-clock,
     non-deterministic, REPORTED for humans, never asserted.
   - WORK (BitBlt calls, pixels, glyphs, events, frames — the VM's always-on UI
     counters): deterministic under the virtual clock, so CI ASSERTS budgets on
     them (perf-budget tests, UI.md §12 item 5).
 Reading the VM counters goes through the existing Profiler primitive (424)."

Object subclass: #FrameProbe
	instanceVariableNames: 'frameCount lastFrameNs frameStartNs phaseNames phaseNs'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-Testing'!

!FrameProbe methodsFor: 'clock'!
nowNs
	"high-resolution monotonic clock (virtual clock in deterministic mode)."
	<primitive: 323>
	^0! !

!FrameProbe methodsFor: 'initializing'!
initialize
	frameCount := 0.
	lastFrameNs := 0.
	phaseNames := OrderedCollection new.
	phaseNs := Dictionary new! !

!FrameProbe methodsFor: 'framing'!
startFrame
	phaseNs isNil ifTrue: [self initialize].
	frameStartNs := self nowNs!
phase: aSymbol do: aBlock
	"time aBlock, accumulating into aSymbol's phase total; answers its value."
	| t0 result dt |
	phaseNs isNil ifTrue: [self initialize].
	t0 := self nowNs.
	result := aBlock value.
	dt := self nowNs - t0.
	(phaseNs includesKey: aSymbol) ifFalse: [phaseNames add: aSymbol].
	phaseNs at: aSymbol put: (phaseNs at: aSymbol ifAbsent: [0]) + dt.
	^result!
endFrame
	frameCount := frameCount + 1.
	lastFrameNs := self nowNs - frameStartNs! !

!FrameProbe methodsFor: 'timing metrics'!
frameCount
	^frameCount!
lastFrameNs
	^lastFrameNs!
phaseNs: aSymbol
	^phaseNs at: aSymbol ifAbsent: [0]! !

!FrameProbe methodsFor: 'work metrics'!
counters
	"the VM's counter rows, each #(name value)."
	^Profiler primCounters!
counterNamed: aName
	"a single deterministic work metric, e.g. 'ui.bitblt_calls'; 0 if absent."
	Profiler primCounters do: [:row |
		(row at: 1) = aName ifTrue: [^row at: 2]].
	^0! !

!FrameProbe methodsFor: 'reporting'!
report
	"a human-facing summary: frame count, last-frame ns, per-phase ns, and the
	 deterministic work counters."
	| ws |
	ws := WriteStream on: (String new: 64).
	ws nextPutAll: 'frames: '.
	ws nextPutAll: frameCount printString.
	ws nextPutAll: ' lastNs: '.
	ws nextPutAll: lastFrameNs printString.
	ws nextPut: 10.
	phaseNames do: [:sym |
		ws nextPutAll: '  '.
		ws nextPutAll: sym.
		ws nextPutAll: ': '.
		ws nextPutAll: (phaseNs at: sym) printString.
		ws nextPutAll: ' ns'.
		ws nextPut: 10].
	^ws contents! !
