| s |
s := 0.
1 to: 10 do: [:k | s := s + k].
Transcript showCr: s printString.
s := 0.
1 to: 5 do: [:k | s := s + (k * k)].
Transcript showCr: s printString.
s := 0.
10 to: 2 by: -2 do: [:k | s := s + k].
Transcript showCr: s printString.
s := 0.
3 timesRepeat: [s := s + 7].
Transcript showCr: s printString
