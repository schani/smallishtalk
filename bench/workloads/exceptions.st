"Benchmark workload (portable dialect: runs under smallishtalk AND GNU
 Smalltalk 3.2.5). Defines `Bench class >> run`, which answers a checksum
 the harness cross-validates between hosts. Timing lives in the drivers."

Object subclass: #Bench
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Bench'!

!Bench class methodsFor: 'benchmark'!
boom
	Error new signal!
run
	| s |
	s := 0.
	1 to: 15000 do: [:i |
		s := s + ([self boom. 0] on: Error do: [:e | e return: 1])].
	^s! !
