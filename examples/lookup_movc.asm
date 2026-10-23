; Count 0..9 on a common-cathode 7-segment display connected to P2,
; using a MOVC lookup table. The digit is also kept in R2.

        ORG 0000h
        LJMP main

        ORG 0030h
main:   MOV DPTR,#segs
again:  MOV R2,#0
next:   MOV A,R2
        MOVC A,@A+DPTR      ; table lookup
        MOV P2,A
        INC R2
        CJNE R2,#10,next
        SJMP again

;           0    1    2    3    4    5    6    7    8    9
segs:   DB 3Fh, 06h, 5Bh, 4Fh, 66h, 6Dh, 7Dh, 07h, 7Fh, 6Fh

        END
