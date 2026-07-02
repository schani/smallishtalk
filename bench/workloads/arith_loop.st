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
	| s |
	s := 1.
	1 to: 400000 do: [:i |
		s := (s + i) * 3 \\ 1000003.
		s := (s bitXor: i) + (s // 7)].
	^s! !
