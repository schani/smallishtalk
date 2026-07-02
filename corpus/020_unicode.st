"Unicode conventions (§15, §18): source is UTF-8; string literals, quoted
 symbols and comments (like this ✓ one) preserve their bytes verbatim;
 String size counts BYTES; a $-literal decodes exactly one Unicode scalar
 into a Character (2-, 3- and 4-byte sequences below)."
| s |
s := 'héllo'.
Transcript showCr: s size printString.
Transcript showCr: $é value printString.
Transcript showCr: $✓ value printString.
Transcript showCr: $😀 value printString.
Transcript showCr: #'s✓' size printString.
($é = $é) ifTrue: [Transcript showCr: 'eq-ok'].
($é value = (s at: 3)) ifFalse: [Transcript showCr: 'bytes-not-scalars']
