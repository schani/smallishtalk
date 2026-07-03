"Window-manager L3 (UI.md §6.2): the single high-priority UI process. Its loop
 drains events EVERY tick (independent of redraw, UI.md §3.2), dispatches them,
 then repaints and presents. `runStep` is one iteration — the headless driver
 and tests call it directly so scenarios are bounded and deterministic; the
 windowed `run` wraps it in the classic Delay-paced loop, terminating on the
 window-close event."

Object subclass: #UISupervisor
	instanceVariableNames: 'display running probe rootView damaged'
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
	damaged := true.
	probe := FrameProbe new! !

!UISupervisor methodsFor: 'accessing'!
display
	^display!
probe
	^probe!
running
	^running!
rootView
	^rootView!
rootView: aView
	"a single top-level app view (e.g. a ClassBrowser) filling the display;
	 when set, the loop paints and routes events to it instead of the WM tracks."
	rootView := aView! !

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
	anEvent isMouseMove
		ifTrue: [
			display cursorX: anEvent position x y: anEvent position y.
			"a move only dirties the screen when we draw the cursor ourselves
			 (headless); windowed, the OS cursor moves without a repaint."
			display cursorVisible ifTrue: [damaged := true]]
		ifFalse: [
			"buttons, keys, scroll, resize can all change what is on screen."
			damaged := true].
	"route buttons and keystrokes (and move) to the app view, if any."
	rootView notNil ifTrue: [rootView handleEvent: anEvent].
	^self! !

!UISupervisor methodsFor: 'the loop'!
runStep
	"one UI iteration, instrumented per phase (UI.md §13). Paint is
	 damage-driven: repainting the whole screen in interpreted Smalltalk every
	 tick pegs a core, so an undamaged tick skips it. The flush is NOT skipped:
	 windowed, presenting is also what pumps the OS event loop (minifb only
	 refreshes its input snapshot inside update), so a static screen must keep
	 presenting to keep receiving input."
	probe startFrame.
	probe phase: #events do: [self pumpEvents].
	probe phase: #paint do: [damaged ifTrue: [self paint. damaged := false]].
	probe phase: #present do: [display flush].
	probe endFrame.
	^self!
paint
	"paint the app view filling the display if one is set, else the WM tracks."
	| canvas |
	rootView isNil ifTrue: [^display draw].
	display form fill: 0.
	canvas := Canvas on: display form.
	rootView displayOn: canvas.
	display blitCursorXor!
run
	"the live windowed loop; not used by headless tests (it would not
	 terminate until a window-close event arrives)."
	[running] whileTrue: [
		self runStep.
		(Delay forMilliseconds: 16) wait].
	^self! !
