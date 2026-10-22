; Packed BCD addition with DA A: 4-digit 1234 + 5678 = 6912.
; Watch A, CY and AC in the Registers panel while stepping.

        ORG 0000h
        MOV R0,#34h         ; low byte of first number
        MOV R1,#12h         ; high byte
        MOV R2,#78h         ; low byte of second number
        MOV R3,#56h         ; high byte

        MOV A,R0
        ADD A,R2            ; 34h + 78h = ACh (binary)
        DA A                ; -> 12h, CY = 1
        MOV 30h,A
        MOV A,R1
        ADDC A,R3           ; 12h + 56h + carry
        DA A                ; -> 69h
        MOV 31h,A           ; result 6912 in 31h:30h

        SJMP $

        END
