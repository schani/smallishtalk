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
	| total ws |
	total := 0.
	1 to: 5 do: [:round |
		ws := WriteStream on: (String new: 16).
		1 to: 40000 do: [:i |
			ws nextPut: (Character value: 65 + (i \\ 26))].
		total := total + ws contents size].
	^total! !
