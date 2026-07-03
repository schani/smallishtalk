"The M1 golden catalog: every instruction form the assembler offers, one
 (or a few) concrete instances each, with the expected disassembly. Three
 consumers (JIT.md §18.1):
   1. StAMD64AssemblerTests round-trips assembler -> disassembler here.
   2. st/tools/gen_amd64_goldens.st dumps name|hex|disasm lines.
   3. tests/amd64_cross_test.rs re-decodes the same bytes with objdump and
      diffs mnemonic sequences — catching self-consistent-but-wrong
      encodings, the classic assembler failure."

Object subclass: #StAMD64Goldens
	instanceVariableNames: ''
	classVariableNames: ''
	poolDictionaries: ''
	category: 'Smallishtalk-JIT'!

!StAMD64Goldens class methodsFor: 'catalog'!
do: aBlock
	"aBlock value: name value: byteCollection value: disasmText (lines
	 separated by newline)"
	self movesDo: aBlock.
	self aluDo: aBlock.
	self controlDo: aBlock!

entry: name emit: emitBlock expect: text into: aBlock
	| a |
	a := StAMD64Assembler new.
	emitBlock value: a.
	aBlock value: name value: a finish value: text!

movesDo: aBlock
	self entry: 'mov_rr' emit: [:a | a movR: 0 fromR: 3]
		expect: 'mov rax, rbx' into: aBlock.
	self entry: 'mov_rr_ext' emit: [:a | a movR: 14 fromR: 9]
		expect: 'mov r14, r9' into: aBlock.
	self entry: 'mov_load' emit: [:a | a movR: 3 fromM: 14 disp: 8]
		expect: 'mov rbx, [r14+8]' into: aBlock.
	self entry: 'mov_load_rsp' emit: [:a | a movR: 1 fromM: 4 disp: 16]
		expect: 'mov rcx, [rsp+16]' into: aBlock.
	self entry: 'mov_load_r12' emit: [:a | a movR: 1 fromM: 12 disp: 24]
		expect: 'mov rcx, [r12+24]' into: aBlock.
	self entry: 'mov_load_r13' emit: [:a | a movR: 8 fromM: 13 disp: 80]
		expect: 'mov r8, [r13+80]' into: aBlock.
	self entry: 'mov_load_neg' emit: [:a | a movR: 0 fromM: 3 disp: -32]
		expect: 'mov rax, [rbx-32]' into: aBlock.
	self entry: 'mov_store' emit: [:a | a movM: 3 disp: 40 fromR: 2]
		expect: 'mov [rbx+40], rdx' into: aBlock.
	self entry: 'mov_store_ext' emit: [:a | a movM: 6 disp: 56 fromR: 9]
		expect: 'mov [rsi+56], r9' into: aBlock.
	self entry: 'mov_load_idx' emit: [:a | a movR: 2 fromM: 7 index: 8 scale: 3 disp: 32]
		expect: 'mov rdx, [rdi+r8*8+32]' into: aBlock.
	self entry: 'mov_store_idx' emit: [:a | a movM: 7 index: 8 scale: 3 disp: 32 fromR: 2]
		expect: 'mov [rdi+r8*8+32], rdx' into: aBlock.
	self entry: 'mov32_load_idx' emit: [:a | a mov32R: 9 fromM: 10 index: 9 scale: 2 disp: 0]
		expect: 'mov r9d, [r10+r9*4+0]' into: aBlock.
	self entry: 'mov_ri32' emit: [:a | a movR: 0 imm32: 42]
		expect: 'mov eax, 42' into: aBlock.
	self entry: 'mov_ri32_ext' emit: [:a | a movR: 11 imm32: 300]
		expect: 'mov r11d, 300' into: aBlock.
	self entry: 'mov_ri64' emit: [:a | a movR: 2 imm64: 81985529216486895]
		expect: 'movabs rdx, 81985529216486895' into: aBlock.
	self entry: 'mov_mi32' emit: [:a | a movM: 3 disp: 48 imm32: 15]
		expect: 'mov qword [rbx+48], 15' into: aBlock.
	self entry: 'lea_idx' emit: [:a | a leaR: 3 base: 3 index: 1 scale: 3 disp: 8]
		expect: 'lea rbx, [rbx+rcx*8+8]' into: aBlock.
	self entry: 'lea_tag' emit: [:a | a leaR: 9 base: 1 index: 1 scale: 0 disp: 1]
		expect: 'lea r9, [rcx+rcx*1+1]' into: aBlock.
	self entry: 'lea_bd' emit: [:a | a leaR: 0 base: 5 disp: 64]
		expect: 'lea rax, [rbp+64]' into: aBlock!

aluDo: aBlock
	self entry: 'add_rr' emit: [:a | a addR: 0 fromR: 1]
		expect: 'add rax, rcx' into: aBlock.
	self entry: 'sub_rr' emit: [:a | a subR: 2 fromR: 10]
		expect: 'sub rdx, r10' into: aBlock.
	self entry: 'or_rr' emit: [:a | a orR: 0 fromR: 2]
		expect: 'or rax, rdx' into: aBlock.
	self entry: 'xor_rr' emit: [:a | a xorR: 7 fromR: 7]
		expect: 'xor rdi, rdi' into: aBlock.
	self entry: 'and_rr' emit: [:a | a andR: 1 fromR: 2]
		expect: 'and rcx, rdx' into: aBlock.
	self entry: 'cmp_rr' emit: [:a | a cmpR: 1 fromR: 2]
		expect: 'cmp rcx, rdx' into: aBlock.
	self entry: 'cmp32_rr' emit: [:a | a cmp32R: 9 fromR: 0]
		expect: 'cmp r9d, eax' into: aBlock.
	self entry: 'imul_rr' emit: [:a | a imulR: 0 fromR: 3]
		expect: 'imul rax, rbx' into: aBlock.
	self entry: 'add_ri32' emit: [:a | a addR: 0 imm32: 1000]
		expect: 'add rax, 1000' into: aBlock.
	self entry: 'and_ri32' emit: [:a | a andR: 8 imm32: 255]
		expect: 'and r8, 255' into: aBlock.
	self entry: 'sub_ri32' emit: [:a | a subR: 2 imm32: 7]
		expect: 'sub rdx, 7' into: aBlock.
	self entry: 'cmp_ri32' emit: [:a | a cmpR: 2 imm32: 1]
		expect: 'cmp rdx, 1' into: aBlock.
	self entry: 'add_ri8' emit: [:a | a addR: 10 imm8: -1]
		expect: 'add r10, -1' into: aBlock.
	self entry: 'cmp32_ri_patch' emit: [:a | a cmp32R: 11 imm32: 42]
		expect: 'cmp r11d, 42' into: aBlock.
	self entry: 'test_rr' emit: [:a | a testR: 0 fromR: 0]
		expect: 'test rax, rax' into: aBlock.
	self entry: 'test_ri32' emit: [:a | a testR: 2 imm32: 1]
		expect: 'test rdx, 1' into: aBlock.
	self entry: 'neg_r' emit: [:a | a negR: 1]
		expect: 'neg rcx' into: aBlock.
	self entry: 'idiv_r' emit: [:a | a idivR: 6]
		expect: 'idiv rsi' into: aBlock.
	self entry: 'cqo' emit: [:a | a cqo]
		expect: 'cqo' into: aBlock.
	self entry: 'shl' emit: [:a | a shlR: 9 by: 4]
		expect: 'shl r9, 4' into: aBlock.
	self entry: 'shr' emit: [:a | a shrR: 10 by: 24]
		expect: 'shr r10, 24' into: aBlock.
	self entry: 'sar' emit: [:a | a sarR: 1 by: 1]
		expect: 'sar rcx, 1' into: aBlock.
	self entry: 'cmp_m8' emit: [:a | a cmpM8: 8 disp: 0 imm8: 0]
		expect: 'cmp byte [r8+0], 0' into: aBlock.
	self entry: 'cmp_rm' emit: [:a | a cmpR: 0 fromM: 14 disp: 24]
		expect: 'cmp rax, [r14+24]' into: aBlock.
	self entry: 'cmp_rm_neg' emit: [:a | a cmpR: 2 fromM: 6 disp: -8]
		expect: 'cmp rdx, [rsi-8]' into: aBlock.
	self entry: 'movzx_b' emit: [:a | a movzxR: 2 fromM8: 2 index: 7 disp: 8]
		expect: 'movzx rdx, byte [rdx+rdi*1+8]' into: aBlock.
	self entry: 'mov_b_store' emit: [:a | a movM8: 0 index: 7 disp: 8 fromR: 2]
		expect: 'mov byte [rax+rdi*1+8], dl' into: aBlock!

controlDo: aBlock
	self entry: 'jmp_fwd' emit: [:a |
			| l | l := a newLabel.
			a jmpTo: l.
			a int3.
			a bind: l.
			a int3]
		expect: 'jmp 0x6
int3
int3' into: aBlock.
	self entry: 'je_back' emit: [:a |
			| l | l := a newLabel.
			a bind: l.
			a int3.
			a j: #e to: l]
		expect: 'int3
je 0x0' into: aBlock.
	self entry: 'jne_fwd' emit: [:a |
			| l | l := a newLabel.
			a j: #ne to: l.
			a int3.
			a bind: l]
		expect: 'jne 0x7
int3' into: aBlock.
	self entry: 'jle' emit: [:a |
			| l | l := a newLabel.
			a j: #le to: l.
			a bind: l]
		expect: 'jle 0x6' into: aBlock.
	self entry: 'jg' emit: [:a |
			| l | l := a newLabel.
			a j: #g to: l.
			a bind: l]
		expect: 'jg 0x6' into: aBlock.
	self entry: 'jo' emit: [:a |
			| l | l := a newLabel.
			a j: #o to: l.
			a bind: l]
		expect: 'jo 0x6' into: aBlock.
	self entry: 'jmp_patchable' emit: [:a |
			| l | l := a newLabel.
			a jmpPatchableTo: l.
			a bind: l]
		expect: 'jmp 0x5' into: aBlock.
	self entry: 'jmp_r' emit: [:a | a jmpR: 11]
		expect: 'jmp r11' into: aBlock.
	self entry: 'jmp_m' emit: [:a | a jmpM: 13 disp: 80]
		expect: 'jmp qword [r13+80]' into: aBlock.
	self entry: 'call_m' emit: [:a | a callM: 13 disp: 16]
		expect: 'call qword [r13+16]' into: aBlock.
	self entry: 'int3' emit: [:a | a int3]
		expect: 'int3' into: aBlock.
	self entry: 'ud2' emit: [:a | a ud2]
		expect: 'ud2' into: aBlock! !
