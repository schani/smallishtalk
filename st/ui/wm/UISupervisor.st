"Window-manager L3 (UI.md §6.2): the single high-priority UI process. Its loop
 drains events EVERY tick (independent of redraw, UI.md §3.2), dispatches them,
 then repaints and presents. `runStep` is one iteration — the headless driver
 and tests call it directly so scenarios are bounded and deterministic; the
 windowed `run` wraps it in the classic Delay-paced loop, terminating on the
 window-close event."

Object subclass: #UISupervisor
	instanceVariableNames: 'display running probe'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'UI-WindowManager'!

!UISupervisor class methodsFor: 'creation'!
on: aDisplay
	| s |
	s := self new.
	s setDisplay: aDisplay.
	^s! !

!UISupervisor methodsFor: 'initializing'!
setDisplay: aDisplay
	display := aDisplay.
	running := true.
	probe := FrameProbe new! !

!UISupervisor methodsFor: 'accessing'!
display
	^display!
probe
	^probe!
running
	^running! !

!UISupervisor methodsFor: 'events'!
pumpEvents
	"drain ALL pending host events this tick, even if nothing will be
	 redrawn — a static screen must still receive input (UI.md §3.2)."
	| ev n |
	n := 0.
	[(ev := Event next) notNil] whileTrue: [
		self dispatch: ev.
		n := n + 1].
	^n!
dispatch: anEvent
	anEvent isClose ifTrue: [running := false. ^self].
	anEvent isMouseMove ifTrue: [
		display cursorX: anEvent position x y: anEvent position y.
		^self].
	"keystrokes and button hits are routed to widgets in M3."
	^self! !

!UISupervisor methodsFor: 'the loop'!
runStep
	"one UI iteration, instrumented per phase (UI.md §13)."
	probe startFrame.
	probe phase: #events do: [self pumpEvents].
	probe phase: #paint do: [display draw].
	probe phase: #present do: [display flush].
	probe endFrame.
	^self!
run
	"the live windowed loop; not used by headless tests (it would not
	 terminate until a window-close event arrives)."
	[running] whileTrue: [
		self runStep.
		(Delay forMilliseconds: 16) wait].
	^self! !
