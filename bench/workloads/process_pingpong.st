"Benchmark workload (portable dialect: runs under smallishtalk AND GNU
 Smalltalk 3.2.5). Defines `Bench class >> run`, which answers a checksum
 the harness cross-validates between hosts. Timing lives in the drivers."

Object subclass: #Bench
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Bench'!

!Bench class methodsFor: 'benchmark'!
run
	| a b done count |
	a := Semaphore new.
	b := Semaphore new.
	done := Semaphore new.
	count := 20000.
	[1 to: count do: [:i | a wait. b signal]. done signal] fork.
	1 to: count do: [:i | a signal. b wait].
	done wait.
	^count! !
