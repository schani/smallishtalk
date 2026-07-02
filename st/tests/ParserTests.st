"Phase 2 parser tests (SPEC §20): source → expected AST, structural
 equality via the canonical #sexp printing."

TestCase subclass: #StParserTests
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-Tests'!

!StParserTests methodsFor: 'helpers'!
method: source is: expected
	self assert: (StParser parseMethod: source) sexp = expected!

doIt: source is: expected
	self assert: (StParser parseDoIt: source) sexp = expected! !

!StParserTests methodsFor: 'tests'!
testUnaryPattern
	self method: 'foo ^self'
		is: '(method foo () () (return (var self)))'!

testBinaryPattern
	self method: '+ other ^other'
		is: '(method + (other) () (return (var other)))'!

testKeywordPattern
	self method: 'at: i put: v ^v'
		is: '(method at:put: (i v) () (return (var v)))'!

testTemporaries
	self method: 'foo | a b | a := 1. ^a'
		is: '(method foo () (a b) (assign a (lit 1)) (return (var a)))'!

testPragmas
	self method: 'at: i <primitive: 6> ^self err'
		is: '(method at: (i) () (pragma primitive: 6) (return (send (var self) err)))'!

testUnaryPragma
	self method: 'on: c do: b <handlerFrame> | x y z | ^nil'
		is: '(method on:do: (c b) (x y z) (pragma handlerFrame) (return (var nil)))'!

testPrecedence
	"unary > binary > keyword"
	self doIt: 'a foo + b bar at: 1 + 2'
		is: '(method DoIt () () (send (send (send (var a) foo) + (send (var b) bar)) at: (send (lit 1) + (lit 2))))'!

testParentheses
	self doIt: 'a + (b at: 1)'
		is: '(method DoIt () () (send (var a) + (send (var b) at: (lit 1))))'!

testUnaryChain
	self doIt: 'x abs printString size'
		is: '(method DoIt () () (send (send (send (var x) abs) printString) size))'!

testKeywordMessage
	self doIt: 'd at: k put: v'
		is: '(method DoIt () () (send (var d) at:put: (var k) (var v)))'!

testAssignmentChain
	self doIt: 'a := b := 3'
		is: '(method DoIt () () (assign a (assign b (lit 3))))'!

testCascade
	self doIt: 't show: ''a''; show: ''b''; cr'
		is: '(method DoIt () () (cascade (var t) (show: (lit ''a'')) (show: (lit ''b'')) (cr)))'!

testCascadeReceiverIsLastMessageReceiver
	"a foo bar; baz — the cascade receiver is (a foo)"
	self doIt: 'a foo bar; baz'
		is: '(method DoIt () () (cascade (send (var a) foo) (bar) (baz)))'!

testBlocks
	self doIt: '[:x :y | | t | t := x. t + y]'
		is: '(method DoIt () () (block (x y) (t) (assign t (var x)) (send (var t) + (var y))))'!

testEmptyBlock
	self doIt: '[]'
		is: '(method DoIt () () (block () ()))'!

testBlockWithReturn
	self doIt: '[^5]'
		is: '(method DoIt () () (block () () (return (lit 5))))'!

testNestedBlocks
	self doIt: '[[1]]'
		is: '(method DoIt () () (block () () (block () () (lit 1))))'!

testLiteralArray
	self doIt: '#(1 foo #bar ''s'' $c (2 3) true nil at:put:)'
		is: '(method DoIt () () (lit #(1 #foo #bar ''s'' $c #(2 3) true nil #at:put:)))'!

testNegativeInLiteralArray
	self doIt: '#(1 -2)'
		is: '(method DoIt () () (lit #(1 -2)))'!

testByteArray
	self doIt: '#[1 2 255]'
		is: '(method DoIt () () (lit #[1 2 255]))'!

testVerticalBarAsBinary
	self doIt: 'a | b'
		is: '(method DoIt () () (send (var a) | (var b)))'!

testDoItTemps
	self doIt: '| a | a := 1. a'
		is: '(method DoIt () (a) (assign a (lit 1)) (var a))'!

testSuperSendFlag
	| m stmt |
	m := StParser parseDoIt: 'super foo'.
	stmt := m statements first.
	self assert: stmt isSuperSend!

testStatementsAfterReturnIsError
	self should: [StParser parseMethod: 'foo ^1. 2'] raise: Error!

testKeywordChainInBinaryArgs
	self doIt: 'a max: b + 1 min: c'
		is: '(method DoIt () () (send (var a) max:min: (send (var b) + (lit 1)) (var c)))'! !
