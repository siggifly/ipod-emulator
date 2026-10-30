@ audio-tone -- a bare-metal guest that plays a PCM ring through the WM8758 path.
@
@ Does what a Rockbox-style driver does, in the same order: programs the codec over the I2C
@ controller from a register table, enables the I2S transmit FIFO, enables the DMA controller,
@ routes the DMA interrupt as FIQ, then starts channel 0 on the first segment of a ring and
@ restarts it from the FIQ handler until the segment count runs out.
@
@ Layout, all from the image base (logical 0). The host side fills the data; this file is code only.
@   0x000  vectors            0x400  parameter block (below)
@   0x500  codec table        0x1000 PCM ring
@ Parameter block, one word each:
@   +0  ring base (physical)   +4  ring end (physical)   +8  segment bytes
@   +12 DMA0_CMD to start a segment (size already folded in)
@   +16 segments still to play +20 address of the segment in flight (physical)
@   +24 done flag (set by the FIQ handler on the last segment)
@   +28 FIQs taken             +32 address of the codec table
@ Codec table: one word per register write, byte0 | byte1 << 8 as the I2C helper sends them,
@ terminated by 0xffffffff.
@
@ Assembled with: arm-none-eabi-as -march=armv4t -o audio-tone.o audio-tone.s
@                 arm-none-eabi-objcopy -O binary -j .text audio-tone.o audio-tone.bin
@ and checked in as audio-tone.words with the disassembly beside each word.
        .arm
        .text
        .global _start
        .equ    PARAMS,     0x400
        .equ    I2C,        0x7000c000
        .equ    IIS,        0x70002800
        .equ    DMA,        0x6000a000
        .equ    DMA0,       0x6000b000
        .equ    INTC,       0x60004000
        .equ    CPU_CTRL,   0x60007000
        .equ    COP_CTL,    0x60007004
        .equ    PROC_ID,    0x60000000
        .equ    P_BASE,     0
        .equ    P_END,      4
        .equ    P_SEG,      8
        .equ    P_CMD,      12
        .equ    P_COUNT,    16
        .equ    P_ADDR,     20
        .equ    P_DONE,     24
        .equ    P_FIQS,     28
        .equ    P_TABLE,    32

_start:
        b       reset
        b       .                       @ undefined
        b       .                       @ swi
        b       .                       @ prefetch abort
        b       .                       @ data abort
        nop
        b       .                       @ irq
        b       fiq

reset:
        msr     cpsr_c, #0xd3           @ SVC, IRQ and FIQ masked

        @ both cores enter here; PROC_ID is 0x55555555 on the CPU, 0xaaaaaaaa on the COP
        ldr     r0, =PROC_ID
        ldr     r0, [r0]
        and     r0, r0, #0xff
        cmp     r0, #0xaa
        bne     cpu
        ldr     r0, =COP_CTL
        mov     r1, #0x80000000         @ PROC_SLEEP
        str     r1, [r0]
park:   b       park

cpu:    ldr     r1, =PARAMS

        @ codec register writes
        ldr     r0, =I2C
        ldr     r2, [r1, #P_TABLE]
        mov     r3, #0x34               @ 7-bit address 0x1a, write
        str     r3, [r0, #4]
next:   ldr     r3, [r2], #4
        cmn     r3, #1
        beq     codec_done
        and     r4, r3, #0xff
        str     r4, [r0, #0x0c]
        mov     r4, r3, lsr #8
        and     r4, r4, #0xff
        str     r4, [r0, #0x10]
        mov     r4, #0x82               @ send, two bytes
        str     r4, [r0, #0]
busy:   ldr     r4, [r0, #0x1c]
        tst     r4, #0x40
        bne     busy
        b       next
codec_done:

        @ I2S: TXFIFOEN, FIFO format 7; transmit request level 1
        ldr     r0, =IIS
        ldr     r4, =0x20000070
        str     r4, [r0, #0]
        mov     r4, #1
        str     r4, [r0, #0x0c]

        @ DMA controller on, channel 0 pointed at the FIFO port
        ldr     r0, =DMA
        mov     r4, #0x80000000
        str     r4, [r0]
        ldr     r0, =DMA0
        ldr     r4, =0x70002840
        str     r4, [r0, #0x18]         @ DMA0_PER_ADDR
        ldr     r4, =0x20010000         @ range fixed, 32-bit width
        str     r4, [r0, #0x1c]         @ DMA0_INCR
        mov     r4, #0x04000000
        str     r4, [r0, #0x14]         @ DMA0_FLAGS

        @ DMA interrupt (CPU_INT bit 26) as FIQ, then enabled
        ldr     r0, =INTC
        ldr     r4, [r0, #0x2c]         @ CPU_INT_PRIORITY
        orr     r4, r4, #0x04000000
        str     r4, [r0, #0x2c]
        mov     r4, #0x04000000
        str     r4, [r0, #0x24]         @ CPU_INT_EN

        msr     cpsr_c, #0x93           @ IRQ masked, FIQ open

        @ first segment
        ldr     r0, =DMA0
        ldr     r4, [r1, #P_ADDR]
        str     r4, [r0, #0x10]         @ DMA0_RAM_ADDR
        ldr     r4, [r1, #P_CMD]
        str     r4, [r0, #0]

        @ sleep until the FIQ handler reports the last segment
wait:   ldr     r4, [r1, #P_DONE]
        cmp     r4, #0
        bne     finished
        ldr     r5, =CPU_CTRL
        mov     r6, #0x80000000
        str     r6, [r5]
        b       wait
finished:
        b       finished

@ FIQ: acknowledge, count down, restart the channel on the next segment, wrapping at the ring end.
fiq:
        ldr     r8, =DMA0
        ldr     r12, [r8, #4]           @ DMA0_STATUS read clears the interrupt
        ldr     r12, =PARAMS
        ldr     r9, [r12, #P_FIQS]
        add     r9, r9, #1
        str     r9, [r12, #P_FIQS]
        ldr     r9, [r12, #P_COUNT]
        subs    r9, r9, #1
        str     r9, [r12, #P_COUNT]
        beq     last
        ldr     r9, [r12, #P_ADDR]
        ldr     r10, [r12, #P_SEG]
        add     r9, r9, r10
        ldr     r10, [r12, #P_END]
        cmp     r9, r10
        ldreq   r9, [r12, #P_BASE]
        str     r9, [r12, #P_ADDR]
        str     r9, [r8, #0x10]
        ldr     r10, [r12, #P_CMD]
        str     r10, [r8, #0]
        subs    pc, lr, #4
last:
        mov     r9, #1
        str     r9, [r12, #P_DONE]
        subs    pc, lr, #4
        .ltorg
