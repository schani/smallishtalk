| a |
a := Array new: 3.
a at: 1 put: 10.
a at: 2 put: 20.
a at: 3 put: 30.
Transcript showCr: a size printString.
Transcript showCr: (a at: 2) printString.
Transcript showCr: (a includes: 20) printString.
Transcript showCr: (a inject: 0 into: [:x :y | x + y]) printString.
Transcript showCr: #(1 2 3) printString.
Transcript showCr: #(foo 'bar' 42) printString
