; Blink: toggle all of P1 with a software delay loop.
; Step with F11, or run with F5 and watch the Ports panel.

        ORG 0000h
        LJMP main

        ORG 0030h
main:   MOV P1,#0FFh
loop:   CPL P1.0            ; toggle the LED on P1.0
        MOV A,P1
        CPL A
        MOV P2,A            ; P2 mirrors P1 inverted
        ACALL delay
        SJMP loop

; About 2 * 50 * 250 machine cycles.
delay:  MOV R6,#50
d1:     MOV R7,#250
d2:     DJNZ R7,d2
        DJNZ R6,d1
        RET

        END
