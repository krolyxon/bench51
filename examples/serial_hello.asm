; UART: 9600 baud at 11.0592 MHz (timer 1, mode 2, TH1 = FDh).
; Prints a greeting, then echoes everything typed in the Serial panel
; (Tab to the Serial panel in debug mode, type, press Enter).

        ORG 0000h
        LJMP main

        ORG 0030h
main:   MOV TMOD,#20h       ; timer 1: mode 2 (8-bit auto-reload)
        MOV TH1,#0FDh
        SETB TR1
        MOV SCON,#50h       ; serial mode 1, receiver enabled
        MOV DPTR,#msg
        ACALL puts
echo:   ACALL getc
        ACALL putc
        CJNE A,#0Dh,echo    ; after CR, also send LF
        MOV A,#0Ah
        ACALL putc
        SJMP echo

; Send the zero-terminated string at DPTR.
puts:   CLR A
        MOVC A,@A+DPTR
        JZ done
        ACALL putc
        INC DPTR
        SJMP puts
done:   RET

putc:   MOV SBUF,A
wait:   JNB TI,wait
        CLR TI
        RET

getc:   JNB RI,getc
        MOV A,SBUF
        CLR RI
        RET

msg:    DB "Hello from the 8051!", 0Dh, 0Ah, "Type something: ", 0

        END
