| r |
r := [Error signal: 'boom'] on: Error do: [:e | e messageText].
Transcript showCr: r.
r := [Error signal: 'x'. 'not reached'] on: Error do: [:e | e return: 'returned'].
Transcript showCr: r.
r := [(Error signal: 'y') , '.'] on: Error do: [:e | e resume: 'resumed'].
Transcript showCr: r.
r := [ZeroDivide signal: 'z'] on: Error do: [:e | e class name].
Transcript showCr: r.
r := [7 // 0] on: ZeroDivide do: [:e | -1].
Transcript showCr: r printString.
r := [3 + 4] on: Error do: [:e | -1].
Transcript showCr: r printString
