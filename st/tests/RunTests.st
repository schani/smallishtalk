"Test driver: run every filed-in TestCase subclass in the Smallishtalk
 test category and print a machine-checkable verdict. The run-st-tests.sh
 script greps for ALL-TESTS-PASSED."

| classes total failures errors |
classes := OrderedCollection new.
TestCase allSubclasses do: [:c |
    (c category = 'Smallishtalk-Tests') ifTrue: [classes add: c]].
classes := classes asSortedCollection: [:a :b | a name <= b name].
total := 0. failures := 0. errors := 0.
classes do: [:c | | result |
    result := c suite run.
    Transcript showCr: c name , ': ' , result printString.
    total := total + result runCount.
    failures := failures + result failureCount.
    errors := errors + result errorCount.
    result failures do: [:t | Transcript showCr: '  FAILURE: ' , t printString].
    result errors do: [:t | Transcript showCr: '  ERROR: ' , t printString]].
Transcript showCr: 'TOTAL: ' , total printString , ' run, '
    , failures printString , ' failures, ' , errors printString , ' errors'.
(failures = 0 and: [errors = 0 and: [total > 0]])
    ifTrue: [Transcript showCr: 'ALL-TESTS-PASSED']
    ifFalse: [Transcript showCr: 'TESTS-FAILED'].
