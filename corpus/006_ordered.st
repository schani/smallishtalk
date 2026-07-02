| c |
c := OrderedCollection new.
c add: 3.
c add: 1.
c add: 4.
Transcript showCr: c size printString.
Transcript showCr: (c at: 3) printString.
c addAll: #(1 5).
Transcript showCr: (c inject: 0 into: [:a :b | a + b]) printString.
Transcript showCr: c removeLast printString.
Transcript showCr: c size printString.
Transcript showCr: c asArray printString.
Transcript showCr: (c detect: [:x | x > 3] ifNone: [0]) printString
