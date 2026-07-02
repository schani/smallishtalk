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
	| s c |
	s := 0.
	1 to: 20 do: [:round |
		c := OrderedCollection new.
		1 to: 3000 do: [:i | c add: i].
		1 to: c size do: [:i | s := s + (c at: i)]].
	^s! !
