| ws |
ws := WriteStream on: (String new: 2).
ws nextPutAll: 'ab'.
ws nextPut: 99.
ws nextPut: $d.
Transcript showCr: ws contents.
Transcript showCr: ws contents size printString
