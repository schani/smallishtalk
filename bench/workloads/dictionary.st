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
	| d s |
	d := Dictionary new.
	1 to: 1500 do: [:i | d at: i put: i * 2].
	s := 0.
	1 to: 1500 do: [:i | s := s + (d at: i)].
	^s! !
