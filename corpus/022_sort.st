| c |
c := OrderedCollection new.
#(5 3 9 1 7 3) do: [:e | c add: e].
c sort: [:a :b | a < b].
Transcript showCr: c asArray printString.
c := OrderedCollection new.
#('pear' 'apple' 'plum' 'banana') do: [:e | c add: e].
c sort: [:a :b | a < b].
c do: [:e | Transcript showCr: e].
c := OrderedCollection new.
c add: 42.
c sort: [:a :b | a < b].
Transcript showCr: c asArray printString.
c := OrderedCollection new.
c sort: [:a :b | a < b].
Transcript showCr: c size printString
