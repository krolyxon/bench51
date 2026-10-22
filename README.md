# bench51 — a TUI 8051 assembler, simulator & debugger

A small, Keil µVision-style IDE for the Intel 8051 that runs in your terminal.
Write assembly in the built-in editor, assemble it, inspect the listing and
Intel HEX output, then run, pause and single-step the program while watching
registers, SFRs, memory, ports, timers, interrupts and the serial port.

Built in Rust with [ratatui](https://ratatui.rs). It is a learning tool, not a
cycle-exact replacement for a vendor simulator.

## Why this exists

I had microcontroller practicals to get through, and the course expects you to
use Keil µVision. Keil µVision doesn't run on Linux. Rather than fight with
Wine or a Windows VM every lab session, I wrote my own: bench51 does the parts
of µVision those practicals actually need (write the code, assemble it, look
at the listing and HEX, step through it and watch the registers) right in the
terminal.

-----

## Features

- **Editor** with undo/redo, breakpoint and error markers in the gutter —
  or press Ctrl-G to edit in your own `$EDITOR` (vim, nvim, nano, …).
- **Two-pass assembler** for the full 8051 instruction set (all 255 opcodes):
  - labels, expressions (`+ - * / MOD AND OR XOR NOT SHL SHR HIGH LOW`, `$`)
  - number formats `42`, `2Ah`, `0x2A`, `101010b`, `52o`, `'*'`
  - bit addressing (`P1.3`, `ACC.7`, `20h.0`) and all predefined SFR/bit names
  - directives `ORG DB DW DS END EQU SET BIT DATA IDATA XDATA CODE CSEG AT`
  - generic `JMP`/`CALL` (assembled as `LJMP`/`LCALL`)
  - Keil-style `.lst` listing with symbol table, Intel HEX output
- **Simulator**
  - all instructions with correct flags (CY, AC, OV, P) and machine-cycle counts
  - 4 register banks, 256 bytes IRAM, SFRs, 64K code, 64K XRAM
  - ports P0–P3 with latch vs. pin semantics (read-modify-write reads the latch)
  - timers 0/1 in modes 0–3, gate and counter inputs
  - interrupts: INT0, T0, INT1, T1, serial; two priority levels; edge/level triggers
  - UART in modes 0–3 with baud rate timing from timer 1
  - idle and power-down modes (PCON)
- **Debugger**: run / pause, step into / over / out, run to cursor,
  breakpoints, changed-value highlighting, memory poke, adjustable speed.
- **Open `.hex` files**: they're disassembled into editable source that
  re-assembles to the identical image.

## Usage

```bash
cargo install --path .                               # installs the `bench51` binary

cargo run --release -- examples/serial_hello.asm      # open in the TUI
cargo run --release                                   # start with a template
cargo run --release -- firmware.hex                   # disassemble & debug a HEX file

# Headless assembly: writes prog.hex and prog.lst
cargo run --release -- --assemble prog.asm
cargo run --release -- --assemble prog.asm -o out.hex --lst out.lst
```

`--clock <MHz>` sets the oscillator frequency used for the elapsed-time display
(default 11.0592).

## Keys

| Key | Action |
| --- | --- |
| F1 | help |
| F2 / Ctrl-S | save (assembling a saved file also writes `.lst` and `.hex` next to it) |
| F3 / Ctrl-O | open `.asm` or `.hex` |
| Ctrl-G | edit in `$EDITOR`; on save & exit the buffer reloads and reassembles |
| F4 | switch Edit ⇄ Debug |
| F7 | assemble, reset and load |
| F5 | run / pause (assembles first if the source changed) |
| F11 / F10 / F12 | step into / step over / step out |
| F8 | run to cursor |
| F9 | toggle breakpoint on the cursor line |
| F6 | reset |
| Ctrl-Q | quit |

Edit mode: `Alt-1/2/3` switch between Listing, HEX and Errors; `Alt-↑/↓`,
`Alt-PgUp/PgDn` scroll them. The editor uses Emacs-style keys
(`Ctrl-U` undo, `Ctrl-R` redo, `Ctrl-Y` paste).

Debug mode: `Tab` cycles focus between Source, Memory, Ports and Serial.
Letter shortcuts work too, for terminals that swallow function keys:
`r` run/pause, `s` step, `n` step over, `o` step out, `g` run to cursor,
`b` breakpoint, `x` reset, `i`/`f`/`d`/`c` show IRAM/SFR/XRAM/CODE,
`p` poke memory, `+`/`-` speed, `.` jump to PC, `Esc` pause / back to editor.

- **Ports**: select a pin and press Space to pull it low from outside (a
  button press); the program sees the pin as `latch AND external level`.
- **Serial**: type a line and press Enter to send it (followed by CR) to the
  8051's receiver; everything the 8051 transmits appears above.

## Examples

| File | Shows |
| --- | --- |
| `examples/blink.asm` | port toggling, delay loops, `ACALL`/`RET` |
| `examples/timer_irq.asm` | timer 0 mode 1 interrupt |
| `examples/serial_hello.asm` | UART setup, string output with `MOVC`, echo |
| `examples/button_int0.asm` | edge-triggered INT0, idle mode |
| `examples/bcd_add.asm` | `ADDC` and `DA A` with packed BCD |
| `examples/lookup_movc.asm` | 7-segment table lookup |

## Notes and limitations

- Indirect addressing reaches all 256 bytes of IRAM (8052-style upper 128).
- Serial input has no 9th data bit; in modes 2/3 `RB8` reads 0.
- No 8052 timer 2, external code memory or on-chip peripherals of derivatives.

## Development

```bash
cargo test     # assembler, CPU, peripheral and example-program tests
cargo clippy
```

Code layout: `src/isa.rs` (opcode table shared by everything), `src/asm/`
(assembler, listing, Intel HEX), `src/sim/` (CPU, peripherals, disassembler),
`src/ui/` (ratatui front end).
