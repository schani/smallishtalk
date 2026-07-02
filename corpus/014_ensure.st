| log r |
log := WriteStream on: (String new: 4).
r := [log nextPutAll: 'body.'. 42] ensure: [log nextPutAll: 'ensured.'].
Transcript showCr: r printString.
[[log nextPutAll: 'raise.'. Error signal: 'e'] ensure: [log nextPutAll: 'unwound.']] on: Error do: [:e | log nextPutAll: 'handled.'].
[log nextPutAll: 'ok.'] ifCurtailed: [log nextPutAll: 'BAD.'].
[[Error signal] ifCurtailed: [log nextPutAll: 'curtailed.']] on: Error do: [:e | nil].
Transcript showCr: log contents
