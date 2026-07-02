| s blks blk |
s := 0.
1 to: 4 do: [:i | s := s + i].
Transcript showCr: s printString.
Transcript showCr: (5 to: 4 do: [:i | s := 99]) printString.
Transcript showCr: s printString.
s := 0.
(2 + 1) to: (2 * 3) do: [:i | s := s + i].
Transcript showCr: s printString.
blks := OrderedCollection new.
1 to: 3 do: [:i | blks add: [i]].
Transcript showCr: ((blks at: 1) value + ((blks at: 2) value * 10) + ((blks at: 3) value * 100)) printString.
s := 0.
blk := [:i | s := s + i].
1 to: 3 do: blk.
Transcript showCr: s printString.
Transcript showCr: nil isNil printString.
Transcript showCr: 42 isNil printString.
Transcript showCr: nil notNil printString.
Transcript showCr: 'x' notNil printString.
1 to: 5 do: [:i | i = 3 ifTrue: [Transcript showCr: 'stop-at-3'. ^nil]].
Transcript showCr: 'unreachable'
