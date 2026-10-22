; External interrupt 0 (falling edge on P3.2) counts button presses into P1.
; In debug mode, Tab to the Ports panel, select P3 bit 2 and press Space
; twice (press + release) to generate one falling edge.

        ORG 0000h
        LJMP main

        ORG 0003h           ; INT0 vector
        INC P1
        RETI

        ORG 0030h
main:   MOV P1,#00h
        SETB IT0            ; edge-triggered
        SETB EX0
        SETB EA
sleep:  ORL PCON,#01h       ; idle until an interrupt arrives
        SJMP sleep          ; ...and go back to idle afterwards

        END
