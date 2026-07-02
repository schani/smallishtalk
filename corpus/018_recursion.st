| fib down |
fib := nil.
fib := [:n | n < 2 ifTrue: [n] ifFalse: [(fib value: n - 1) + (fib value: n - 2)]].
Transcript showCr: (fib value: 15) printString.
down := nil.
down := [:n | n = 0 ifTrue: [0] ifFalse: [(down value: n - 1) + 1]].
Transcript showCr: (down value: 2000) printString
