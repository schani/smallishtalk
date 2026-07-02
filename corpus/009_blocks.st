| add2 make counter c2 |
add2 := [:x | x + 2].
Transcript showCr: (add2 value: 40) printString.
make := [:start | | n | n := start. [:inc | n := n + inc. n]].
counter := make value: 10.
Transcript showCr: (counter value: 5) printString.
Transcript showCr: (counter value: 3) printString.
c2 := make value: 100.
Transcript showCr: (c2 value: 1) printString.
Transcript showCr: (counter value: 2) printString
