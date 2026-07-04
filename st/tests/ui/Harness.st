"The in-image test harness (UI.md §12): a minimal SUnit for tests that run
 ON the smallishtalk VM. TestCase subclasses define unary test* methods with
 in-Smalltalk assertions; TestRunner runs every listed case and prints a
 machine-checkable verdict — the launcher (tests/st_suite.rs) and CI grep
 for ALL-TESTS-PASSED. Failure messages carry expected/actual so no
 transcript diffing is ever needed."

Error subclass: #TestFailure
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Testing'!

Object subclass: #TestCase
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Testing'!

Object subclass: #TestRunner
	instanceVariableNames: 'run failed'
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Testing'!

!TestCase methodsFor: 'running'!
setUp
	^self! !

!TestCase methodsFor: 'asserting'!
assert: aBoolean
	aBoolean == true ifFalse: [TestFailure new signal: 'assertion failed']!
assert: aBoolean description: aString
	aBoolean == true ifFalse: [TestFailure new signal: aString]!
deny: aBoolean
	aBoolean == false ifFalse: [TestFailure new signal: 'denied expression is true']!
assert: actual equals: expected
	actual = expected ifFalse: [
		TestFailure new signal:
			'expected: ' , expected printString , ' actual: ' , actual printString]!
fail: aString
	TestFailure new signal: aString! !

!TestRunner class methodsFor: 'running'!
runAll: caseClasses
	^self new runAll: caseClasses! !

!TestRunner methodsFor: 'running'!
runAll: caseClasses
	run := 0.
	failed := 0.
	caseClasses do: [:cls | self runCase: cls].
	Transcript showCr: 'TOTAL: ' , run printString , ' run, '
		, failed printString , ' failed'.
	(failed = 0 and: [run > 0])
		ifTrue: [Transcript showCr: 'ALL-TESTS-PASSED']
		ifFalse: [Transcript showCr: 'TESTS-FAILED']!
runCase: cls
	| beforeRun beforeFailed |
	beforeRun := run.
	beforeFailed := failed.
	cls selectors do: [:sel |
		(self isTestSelector: sel) ifTrue: [self run: sel of: cls]].
	Transcript showCr: cls name asString , ': ' , (run - beforeRun) printString
		, ' run, ' , (failed - beforeFailed) printString , ' failed'!
run: sel of: cls
	run := run + 1.
	[| case |
	 case := cls new.
	 case setUp.
	 case perform: sel withArguments: (Array new: 0)]
		on: Error
		do: [:e |
			failed := failed + 1.
			Transcript showCr: '  FAIL ' , cls name asString , '>>' , sel asString.
			e messageText isNil
				ifFalse: [Transcript showCr: '    ' , e messageText]]!
isTestSelector: sel
	"unary and named test..."
	| s |
	s := sel asString.
	s size < 5 ifTrue: [^false].
	(s copyFrom: 1 to: 4) = 'test' ifFalse: [^false].
	1 to: s size do: [:i | (s at: i) = 58 ifTrue: [^false]].
	^true! !
