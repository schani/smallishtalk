"GST-only compatibility shims for the portable dialect (SPEC §18): under
 the self-hosted kernel these exist natively. String at: answers bytes on
 the VM (§15); charAt: is the portable Character accessor and byteAt: /
 byteAt:put: the portable byte accessors (the lexer scans integer bytes)."

!String methodsFor: 'smallishtalk-compat'!
charAt: i
	^self at: i!
byteAt: i
	^(self at: i) value!
byteAt: i put: b
	self at: i put: (Character value: b).
	^b! !
