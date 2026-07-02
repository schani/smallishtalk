"Chunk-format reader (SPEC §18): parses `!ClassName methodsFor: 'x'! ... ! !`
 files into a neutral event list, so the same source files drive both the
 GST cross-compiler and (phase 5) the in-image loader.

 Events (Arrays):
   (#doIt sourceString)
   (#methods className isMeta category (methodSource...))
 A chunk is text up to an unescaped $!; doubled !! is a literal !."

Object subclass: #StChunkReader
	instanceVariableNames: 'source pos'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Compiler'!

!StChunkReader class methodsFor: 'reading'!
eventsFrom: aString
	| r |
	r := self new.
	r setSource: aString.
	^r events! !

!StChunkReader methodsFor: 'private'!
setSource: aString
	source := aString.
	pos := 1!

atEnd
	^pos > source size!

skipSeparators
	[self atEnd not and: [self isSeparator: (source charAt: pos)]]
		whileTrue: [pos := pos + 1]!

isSeparator: c
	^c value = 32 or: [c value = 9 or: [c value = 10 or: [c value = 13 or: [c value = 12]]]]!

nextChunk
	"text up to an unescaped bang. The scan is comment-, string- and
	 character-literal-aware (bangs inside them do not terminate), and a
	 doubled bang collapses to one — matching GST file-in."
	| ws c |
	ws := WriteStream on: String new.
	[self atEnd] whileFalse: [
		c := source charAt: pos.
		pos := pos + 1.
		c = $$
			ifTrue: [
				ws nextPut: c.
				self atEnd ifFalse: [
					ws nextPut: (source charAt: pos).
					pos := pos + 1]]
			ifFalse: [
			c = $"
				ifTrue: [ws nextPut: c. self copyCommentInto: ws]
				ifFalse: [
				c = $'
					ifTrue: [ws nextPut: c. self copyStringInto: ws]
					ifFalse: [
					c = $!
						ifTrue: [
							(self atEnd not and: [(source charAt: pos) = $!])
								ifTrue: [ws nextPut: $!. pos := pos + 1]
								ifFalse: [^ws contents]]
						ifFalse: [ws nextPut: c]]]]].
	^ws contents!

copyCommentInto: ws
	"the opening quote is already copied; copy through the closing one
	 (doubled quotes stay inside)"
	| c |
	[self atEnd] whileFalse: [
		c := source charAt: pos.
		pos := pos + 1.
		ws nextPut: c.
		c = $" ifTrue: [
			(self atEnd not and: [(source charAt: pos) = $"])
				ifTrue: [ws nextPut: $". pos := pos + 1]
				ifFalse: [^self]]].
	^self!

copyStringInto: ws
	| c |
	[self atEnd] whileFalse: [
		c := source charAt: pos.
		pos := pos + 1.
		ws nextPut: c.
		c = $' ifTrue: [
			(self atEnd not and: [(source charAt: pos) = $'])
				ifTrue: [ws nextPut: $'. pos := pos + 1]
				ifFalse: [^self]]].
	^self!

isEmptyChunk: aString
	| k |
	k := 1.
	[k <= aString size] whileTrue: [
		(self isSeparator: (aString charAt: k)) ifFalse: [^false].
		k := k + 1].
	^true!

isMethodHeader: aChunk
	"identifier ['class'] methodsFor: 'category' — with nothing else.
	 Lets a group header work even when a preceding comment chunk
	 swallowed the leading bang marker."
	| toks i |
	toks := [StLexer tokenize: aChunk] on: Error do: [:e | e return: nil].
	toks isNil ifTrue: [^false].
	toks size < 3 ifTrue: [^false].
	((toks at: 1) type = #identifier) ifFalse: [^false].
	i := 2.
	((toks at: i) type = #identifier and: [(toks at: i) value = 'class'])
		ifTrue: [i := i + 1].
	((toks at: i) type = #keyword and: [(toks at: i) value = 'methodsFor:'])
		ifFalse: [^false].
	i := i + 1.
	i + 1 = toks size ifFalse: [^false].
	^(toks at: i) type = #string! !

!StChunkReader methodsFor: 'reading'!
events
	| events chunk |
	events := OrderedCollection new.
	[self skipSeparators.
	 self atEnd] whileFalse: [
		(source charAt: pos) = $!
			ifTrue: [
				pos := pos + 1.
				self skipSeparators.
				events add: (self readMethodGroup: self nextChunk)]
			ifFalse: [
				chunk := self nextChunk.
				(self isEmptyChunk: chunk)
					ifFalse: [
						(self isMethodHeader: chunk)
							ifTrue: [events add: (self readMethodGroup: chunk)]
							ifFalse: [events add: (Array with: #doIt with: chunk)]]]].
	^events!

readMethodGroup: headerChunk
	"headerChunk: `Foo methodsFor: 'category'` or `Foo class methodsFor: '...'`"
	| toks className isMeta i category sources chunk |
	toks := StLexer tokenize: headerChunk.
	((toks at: 1) type = #identifier)
		ifFalse: [self error: 'bad method-group header: ' , headerChunk].
	className := (toks at: 1) value.
	isMeta := false.
	i := 2.
	((toks at: i) type = #identifier and: [(toks at: i) value = 'class'])
		ifTrue: [isMeta := true. i := i + 1].
	((toks at: i) type = #keyword and: [(toks at: i) value = 'methodsFor:'])
		ifFalse: [self error: 'expected methodsFor: in ' , headerChunk].
	((toks at: i + 1) type = #string)
		ifFalse: [self error: 'expected category string in ' , headerChunk].
	category := (toks at: i + 1) value.
	sources := OrderedCollection new.
	[chunk := self nextChunk.
	 self isEmptyChunk: chunk] whileFalse: [sources add: chunk].
	^Array with: #methods with: className with: isMeta with: category with: sources! !
