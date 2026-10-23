; Timer 0 interrupt: mode 1 (16-bit) overflow every 1000 cycles toggles P1.0
; and counts ticks in R7. The main loop just idles.

RELOAD  EQU 65536 - 1000

        ORG 0000h
        LJMP main

        ORG 000Bh           ; timer 0 overflow vector
        LJMP t0_isr

        ORG 0030h
main:   MOV SP,#5Fh
        MOV TMOD,#01h       ; T0: mode 1, timer
        MOV TH0,#HIGH RELOAD
        MOV TL0,#LOW RELOAD
        SETB ET0            ; enable timer 0 interrupt
        SETB EA             ; global interrupt enable
        SETB TR0            ; start timer 0
idle:   SJMP idle

t0_isr: MOV TH0,#HIGH RELOAD
        MOV TL0,#LOW RELOAD
        CPL P1.0
        INC R7
        RETI

        END
