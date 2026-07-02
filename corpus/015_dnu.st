| r |
r := [3 frobnicate] on: MessageNotUnderstood do: [:e | e message selector].
Transcript showCr: r.
r := [5 zork: 1] on: MessageNotUnderstood do: [:e | e resume: 77].
Transcript showCr: r printString
