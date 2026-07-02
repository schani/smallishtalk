"Chunk-format reader (SPEC §18): parses `!ClassName methodsFor: 'x'! ... ! !`
 files into a neutral event list, so the same source files drive both the
 GST cross-compiler and (phase 5) the in-image loader.

 Like the lexer, the scan runs over integer bytes (Platform bytesOf:) —
 all boundary decisions are ASCII byte compares, so UTF-8 content passes
 through chunks untouched on both hosts.

 Events (Arrays):
   (#doIt sourceString)
   (#methods className isMeta category (methodSource...))
 A chunk is text up to an unescaped $!; doubled !! is a literal !."

Object subclass: #StChunkReader
	instanceVariableNames: 'source bytes pos'
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
	bytes := Platform bytesOf: aString.
	pos := 1!

atEnd
	^pos > bytes size!

skipSeparators
	[self atEnd not and: [self isSeparator: (bytes at: pos)]]
		whileTrue: [pos := pos + 1]!

isSeparator: b
	^b = 32 or: [b = 9 or: [b = 10 or: [b = 13 or: [b = 12]]]]!

nextChunk
	"text up to an unescaped bang. The scan is comment-, string- and
	 character-literal-aware (bangs inside them do not terminate), and a
	 doubled bang collapses to one — matching GST file-in."
	| ws b |
	ws := StByteBuf new: 64.
	[self atEnd] whileFalse: [
		b := bytes at: pos.
		pos := pos + 1.
		b = 36 "$"
			ifTrue: [
				ws addByte: b.
				self atEnd ifFalse: [
					ws addByte: (bytes at: pos).
					pos := pos + 1]]
			ifFalse: [
			b = 34 "double quote"
				ifTrue: [ws addByte: b. self copyCommentInto: ws]
				ifFalse: [
				b = 39 "'"
					ifTrue: [ws addByte: b. self copyStringInto: ws]
					ifFalse: [
					b = 33 "!"
						ifTrue: [
							(self atEnd not and: [(bytes at: pos) = 33])
								ifTrue: [ws addByte: 33. pos := pos + 1]
								ifFalse: [^ws contents]]
						ifFalse: [ws addByte: b]]]]].
	^ws contents!

copyCommentInto: ws
	"the opening quote is already copied; copy through the closing one
	 (doubled quotes stay inside)"
	| b |
	[self atEnd] whileFalse: [
		b := bytes at: pos.
		pos := pos + 1.
		ws addByte: b.
		b = 34 ifTrue: [
			(self atEnd not and: [(bytes at: pos) = 34])
				ifTrue: [ws addByte: 34. pos := pos + 1]
				ifFalse: [^self]]].
	^self!

copyStringInto: ws
	| b |
	[self atEnd] whileFalse: [
		b := bytes at: pos.
		pos := pos + 1.
		ws addByte: b.
		b = 39 ifTrue: [
			(self atEnd not and: [(bytes at: pos) = 39])
				ifTrue: [ws addByte: 39. pos := pos + 1]
				ifFalse: [^self]]].
	^self!

isEmptyChunk: aString
	| bs k |
	bs := Platform bytesOf: aString.
	k := 1.
	[k <= bs size] whileTrue: [
		(self isSeparator: (bs at: k)) ifFalse: [^false].
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
		(bytes at: pos) = 33 "!"
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
