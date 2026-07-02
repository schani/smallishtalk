"GST-only compatibility shims for the portable dialect (SPEC §18): under
 the self-hosted kernel these exist natively. String at: answers bytes on
 the VM (§15); charAt: is the portable Character accessor."

!String methodsFor: 'smallishtalk-compat'!
charAt: i
	^self at: i! !
