| tries r |
tries := 0.
r := [tries := tries + 1. tries < 3 ifTrue: [Error signal: 'again']. tries] on: Error do: [:e | e retry].
Transcript showCr: r printString
