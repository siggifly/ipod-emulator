@ timer-loop — the first hardware/emulator differential snippet.
@
@ Checked in as `timer-loop.words` too, hand-assembled, because the repository keeps no binaries
@ outside docs/media/. `cargo test` holds the two to each other. Run: `snippet timer-loop.words`,
@ or against a device capture: `snippet timer-loop.words --device=REPORT --timing=1,2,3`.
@
@ Contract: ARM state, position-independent, called as `void f(u32 io[16])` with IRQs masked.
@   io[0]  PROC_ID read as a word        (0x60000000)
@   io[1]  USEC_TIMER before the loop    (0x60005010)
@   io[2]  USEC_TIMER after the loop
@   io[3]  io[2] - io[1]: microseconds the loop took
@   io[4]  iterations the loop ran — input, default 10 000 if zero
@   io[5]  number of calls so far (accumulates across --iterations)
@
@ The loop is three instructions (subs, bne, and the implicit refill) so its cost is dominated by
@ the core's branch penalty and nothing else — no memory traffic inside it. On the part that is a
@ measurement of cycles per iteration at the running clock; here it is instructions at --clock.
        .arm
        .text
        .global snippet
snippet:
        ldr     r1, proc_id
        ldr     r2, [r1]
        str     r2, [r0, #0]
        ldr     r3, [r0, #16]
        cmp     r3, #0
        ldreq   r3, default_n
        streq   r3, [r0, #16]
        ldr     r1, usec_timer
        ldr     r2, [r1]
1:      subs    r3, r3, #1
        bne     1b
        ldr     r12, [r1]
        str     r2, [r0, #4]
        str     r12, [r0, #8]
        sub     r12, r12, r2
        str     r12, [r0, #12]
        ldr     r2, [r0, #20]
        add     r2, r2, #1
        str     r2, [r0, #20]
        bx      lr
proc_id:    .word 0x60000000
usec_timer: .word 0x60005010
default_n:  .word 10000
