| r |
r := [[Error signal: 'inner'] on: ZeroDivide do: [:e | 'wrong']] on: Error do: [:e | 'outer'].
Transcript showCr: r.
r := [[Error signal: 'a'] on: Error do: [:e | Error signal: 'b'. 'x']] on: Error do: [:e | 'outer2: ' , e messageText].
Transcript showCr: r.
r := [[Error signal: 'p'] on: Error do: [:e | e pass. 'after-pass']] on: Error do: [:e | e return: 'outer3'].
Transcript showCr: r
