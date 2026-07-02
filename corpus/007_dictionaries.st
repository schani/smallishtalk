| d ws |
d := Dictionary new.
d at: #one put: 1.
d at: #two put: 2.
d at: #one put: 11.
Transcript showCr: d size printString.
Transcript showCr: (d at: #two) printString.
Transcript showCr: (d at: #missing ifAbsent: [99]) printString.
Transcript showCr: (d includesKey: #one) printString.
ws := WriteStream on: (String new: 4).
d keysAndValuesDo: [:k :v | ws nextPutAll: k. ws nextPutAll: v printString].
Transcript showCr: ws contents
