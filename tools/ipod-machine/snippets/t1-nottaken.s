@ t1-nottaken — written/35 T1 (CPU timing): one compare and a NOT-taken branch per iteration (1 cycle on ARM7TDMI).
@
@ Same contract as timer-loop.s: ARM state, position-independent, `void f(u32 io[16])`, IRQs masked.
@   io[0] PROC_ID · io[1]/io[2] USEC_TIMER before/after · io[3] the difference (µs)
@   io[4] iterations (input, default 10 000) · io[5] calls so far · io[6..8] scratch the body may use
@ The loop is the body plus `subs`/`bne`; subtract timer-loop's time per iteration to get the body's.
        .arm
        .text
        .global snippet
snippet:
        push    {r4-r7, lr}
        ldr     r1, proc_id
        ldr     r2, [r1]
        str     r2, [r0, #0]
        ldr     r3, [r0, #16]
        cmp     r3, #0
        ldreq   r3, default_n
        streq   r3, [r0, #16]
        add     r4, r0, #24
        ldr     r1, usec_timer
        ldr     r2, [r1]
1:
        cmp     r3, #0
        beq     2f
        subs    r3, r3, #1
        bne     1b
2:      ldr     r12, [r1]
        str     r2, [r0, #4]
        str     r12, [r0, #8]
        sub     r12, r12, r2
        str     r12, [r0, #12]
        ldr     r2, [r0, #20]
        add     r2, r2, #1
        str     r2, [r0, #20]
        pop     {r4-r7, lr}
        bx      lr
proc_id:    .word 0x60000000
usec_timer: .word 0x60005010
default_n:  .word 10000
iram:       .word 0x40000000
