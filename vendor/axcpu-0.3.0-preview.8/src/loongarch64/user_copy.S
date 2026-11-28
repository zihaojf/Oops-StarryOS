.section .text
.global user_copy
user_copy:
	beqz	$a2, 3f

	or	$t1, $a0, $a1
	andi	$t1, $t1, 7
	bnez	$t1, 1f
	sltui	$t1, $a2, 8
	bnez	$t1, 1f

// Advance only after a successful store so fixups return the uncopied count.
4:	ld.d	$t0, $a1, 0
5:	st.d	$t0, $a0, 0
	addi.d	$a0, $a0, 8
	addi.d	$a1, $a1, 8
	addi.d	$a2, $a2, -8
	sltui	$t1, $a2, 8
	beqz	$t1, 4b
	beqz	$a2, 3f

1:	ld.b	$t0, $a1, 0
2:	st.b	$t0, $a0, 0
	addi.d	$a0, $a0, 1
	addi.d	$a1, $a1, 1
	addi.d	$a2, $a2, -1
	bgtz	$a2, 1b

3:	move	$a0, $a2
	jr	$ra

	_asm_extable 4b, 3b
	_asm_extable 5b, 3b
	_asm_extable 1b, 3b
	_asm_extable 2b, 3b
