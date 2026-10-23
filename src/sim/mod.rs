//! Intel 8051 simulator: CPU core, memories and on-chip peripherals.

mod cpu;
pub mod disasm;
mod periph;

use std::collections::{HashSet, VecDeque};

use crate::isa;
use crate::sfr::{self, ACC, B, DPH, DPL, IE, IP, P0, P1, P2, P3, PCON, PSW, SBUF, SP};
pub use periph::Uart;

pub const CY: u8 = 0x80;
pub const AC: u8 = 0x40;
pub const OV: u8 = 0x04;
pub const PARITY: u8 = 0x01;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// Ran the requested number of instructions / cycle budget.
    Budget,
    Breakpoint(u16),
    /// A step-over / step-out target was reached.
    Target,
    ReservedOpcode {
        pc: u16,
        opcode: u8,
    },
    PowerDown,
}

pub struct Sim {
    pub code: Box<[u8; 0x10000]>,
    /// Internal RAM. Indirect addressing reaches all 256 bytes (8052-style
    /// upper 128); direct addresses 80h-FFh go to the SFRs instead.
    pub iram: [u8; 256],
    /// SFR space 80h-FFh, indexed by `addr - 0x80`.
    pub sfr: [u8; 128],
    pub xram: Box<[u8; 0x10000]>,
    pub pc: u16,
    /// Machine cycles since reset.
    pub cycles: u64,
    /// Levels driven onto the port pins from outside (1 = released/high).
    /// A pin reads as `latch & external`.
    pub pins: [u8; 4],
    pub uart: Uart,
    pub idle: bool,
    pub power_down: bool,
    /// Priority levels of interrupts currently in service (innermost last).
    pub in_service: Vec<u8>,
    /// Vector of the last interrupt taken, for the UI log.
    pub last_irq: Option<u16>,
    /// RETI or a write to IE/IP delays interrupt acceptance by one instruction.
    irq_hold: bool,
    /// Pin levels of P3 in the previous cycle, for edge detection.
    prev_p3: u8,
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}

impl Sim {
    pub fn new() -> Self {
        let mut s = Self {
            code: Box::new([0; 0x10000]),
            iram: [0; 256],
            sfr: [0; 128],
            xram: Box::new([0; 0x10000]),
            pc: 0,
            cycles: 0,
            pins: [0xFF; 4],
            uart: Uart::default(),
            idle: false,
            power_down: false,
            in_service: Vec::new(),
            last_irq: None,
            irq_hold: false,
            prev_p3: 0xFF,
        };
        s.reset();
        s
    }

    /// Hardware reset: SFRs to their reset values, PC to 0. Code and XRAM
    /// are kept; IRAM is cleared so runs are reproducible.
    pub fn reset(&mut self) {
        self.iram = [0; 256];
        self.sfr = [0; 128];
        for p in [P0, P1, P2, P3] {
            self.set_sfr(p, 0xFF);
        }
        self.set_sfr(SP, 0x07);
        self.pc = 0;
        self.cycles = 0;
        self.uart.reset();
        self.idle = false;
        self.power_down = false;
        self.in_service.clear();
        self.last_irq = None;
        self.irq_hold = false;
        self.prev_p3 = self.port_pins(3);
    }

    pub fn load_code(&mut self, bytes: impl Iterator<Item = (u16, u8)>) {
        self.code.fill(0);
        for (a, b) in bytes {
            self.code[a as usize] = b;
        }
    }

    // ---- Raw register access ----

    pub fn sfr(&self, addr: u8) -> u8 {
        self.sfr[(addr & 0x7F) as usize]
    }

    pub fn set_sfr(&mut self, addr: u8, v: u8) {
        self.sfr[(addr & 0x7F) as usize] = v;
    }

    pub fn acc(&self) -> u8 {
        self.sfr(ACC)
    }

    pub fn set_acc(&mut self, v: u8) {
        self.set_sfr(ACC, v);
    }

    pub fn b(&self) -> u8 {
        self.sfr(B)
    }

    pub fn psw(&self) -> u8 {
        self.sfr(PSW)
    }

    pub fn sp(&self) -> u8 {
        self.sfr(SP)
    }

    pub fn dptr(&self) -> u16 {
        u16::from_be_bytes([self.sfr(DPH), self.sfr(DPL)])
    }

    pub fn set_dptr(&mut self, v: u16) {
        let [h, l] = v.to_be_bytes();
        self.set_sfr(DPH, h);
        self.set_sfr(DPL, l);
    }

    pub fn flag(&self, mask: u8) -> bool {
        self.psw() & mask != 0
    }

    pub fn set_flag(&mut self, mask: u8, on: bool) {
        let p = self.psw();
        self.set_sfr(PSW, if on { p | mask } else { p & !mask });
    }

    pub fn bank(&self) -> u8 {
        (self.psw() >> 3) & 3
    }

    /// IRAM address of register Rn in the active bank.
    pub fn reg_addr(&self, n: u8) -> u8 {
        self.bank() * 8 + n
    }

    pub fn reg(&self, n: u8) -> u8 {
        self.iram[self.reg_addr(n) as usize]
    }

    /// Pin levels of port `n`: latch AND externally driven level.
    pub fn port_pins(&self, n: usize) -> u8 {
        self.sfr(P0 + 0x10 * n as u8) & self.pins[n]
    }

    // ---- Addressing modes ----

    /// Reads a direct address. Ports read their pins, except for
    /// read-modify-write instructions (`rmw`) which read the latch.
    pub fn read_direct(&self, addr: u8, rmw: bool) -> u8 {
        match addr {
            0x00..=0x7F => self.iram[addr as usize],
            P0 | P1 | P2 | P3 if !rmw => self.port_pins(((addr - P0) >> 4) as usize),
            SBUF => self.uart.sbuf_rx,
            _ => self.sfr(addr),
        }
    }

    pub fn write_direct(&mut self, addr: u8, v: u8) {
        match addr {
            0x00..=0x7F => self.iram[addr as usize] = v,
            SBUF => self.uart_transmit(v),
            IE | IP => {
                self.set_sfr(addr, v);
                self.irq_hold = true;
            }
            PCON => {
                // IDL (bit 0) and PD (bit 1) take effect immediately.
                self.set_sfr(addr, v);
                self.power_down |= v & 0x02 != 0;
                self.idle |= v & 0x01 != 0;
            }
            _ => self.set_sfr(addr, v),
        }
    }

    pub fn read_bit(&self, bit: u8, rmw: bool) -> bool {
        let (byte, idx) = sfr::bit_location(bit);
        self.read_direct(byte, rmw) & (1 << idx) != 0
    }

    pub fn write_bit(&mut self, bit: u8, on: bool) {
        let (byte, idx) = sfr::bit_location(bit);
        let v = self.read_direct(byte, true);
        self.write_direct(byte, if on { v | (1 << idx) } else { v & !(1 << idx) });
    }

    pub fn push(&mut self, v: u8) {
        let sp = self.sp().wrapping_add(1);
        self.set_sfr(SP, sp);
        self.iram[sp as usize] = v;
    }

    pub fn pop(&mut self) -> u8 {
        let sp = self.sp();
        self.set_sfr(SP, sp.wrapping_sub(1));
        self.iram[sp as usize]
    }

    pub fn push_pc(&mut self) {
        let [h, l] = self.pc.to_be_bytes();
        self.push(l);
        self.push(h);
    }

    pub fn pop_pc(&mut self) {
        let h = self.pop();
        let l = self.pop();
        self.pc = u16::from_be_bytes([h, l]);
    }

    // ---- Execution control ----

    /// Executes one instruction (or one idle cycle), then services interrupts.
    pub fn step(&mut self) -> Result<(), Stop> {
        if self.power_down {
            return Err(Stop::PowerDown);
        }
        if self.idle {
            self.tick(1);
        } else {
            let pc = self.pc;
            let opcode = self.code[pc as usize];
            let Some(info) = isa::info(opcode) else {
                return Err(Stop::ReservedOpcode { pc, opcode });
            };
            self.pc = pc.wrapping_add(1);
            self.execute(opcode);
            let p = self.acc().count_ones() % 2 == 1;
            self.set_flag(PARITY, p);
            self.tick(info.cycles as u32);
        }
        self.service_interrupts();
        Ok(())
    }

    fn current_call_len(&self) -> Option<u16> {
        let op = self.code[self.pc as usize];
        match op {
            0x12 => Some(3),
            _ if op & 0x1F == 0x11 => Some(2),
            _ => None,
        }
    }

    /// Runs up to `max_steps` instructions, stopping at breakpoints.
    /// `resume` ignores a breakpoint at the starting PC, so execution can
    /// continue from the breakpoint it last stopped at.
    pub fn run(&mut self, max_steps: u32, breakpoints: &HashSet<u16>, resume: bool) -> Stop {
        for i in 0..max_steps {
            if (i > 0 || !resume) && !self.idle && breakpoints.contains(&self.pc) {
                return Stop::Breakpoint(self.pc);
            }
            if let Err(s) = self.step() {
                return s;
            }
        }
        Stop::Budget
    }

    /// Like `step`, but executes a whole ACALL/LCALL subroutine.
    pub fn step_over(&mut self, max_steps: u32, breakpoints: &HashSet<u16>) -> Stop {
        match self.current_call_len() {
            Some(len) => {
                let target = self.pc.wrapping_add(len);
                let sp = self.sp();
                self.run_until(max_steps, breakpoints, |s| s.pc == target && s.sp() == sp)
            }
            None => match self.step() {
                Ok(()) => Stop::Target,
                Err(s) => s,
            },
        }
    }

    /// Runs until the current subroutine (or interrupt handler) returns.
    pub fn step_out(&mut self, max_steps: u32, breakpoints: &HashSet<u16>) -> Stop {
        let sp = self.sp();
        let mut returning = false;
        self.run_until_with(max_steps, breakpoints, |s, before| {
            if before {
                let op = s.code[s.pc as usize];
                returning = (op == 0x22 || op == 0x32) && s.sp() <= sp && !s.idle;
                false
            } else {
                returning
            }
        })
    }

    fn run_until(
        &mut self,
        max_steps: u32,
        bps: &HashSet<u16>,
        mut done: impl FnMut(&Sim) -> bool,
    ) -> Stop {
        self.run_until_with(max_steps, bps, |s, before| !before && done(s))
    }

    /// `done(sim, before)` is called before (`true`) and after (`false`) each step.
    fn run_until_with(
        &mut self,
        max_steps: u32,
        bps: &HashSet<u16>,
        mut done: impl FnMut(&Sim, bool) -> bool,
    ) -> Stop {
        for i in 0..max_steps {
            if i > 0 && !self.idle && bps.contains(&self.pc) {
                return Stop::Breakpoint(self.pc);
            }
            done(self, true);
            if let Err(s) = self.step() {
                return s;
            }
            if done(self, false) {
                return Stop::Target;
            }
        }
        Stop::Budget
    }

    /// Queue bytes for the UART receiver, as if typed on a serial terminal.
    pub fn uart_send(&mut self, bytes: &[u8]) {
        self.uart.rx_queue.extend(bytes.iter().copied());
    }

    pub fn uart_rx_pending(&self) -> &VecDeque<u8> {
        &self.uart.rx_queue
    }
}

#[cfg(test)]
mod example_tests {
    use super::Sim;
    use crate::asm::assemble;

    fn load(src: &str) -> Sim {
        let out = assemble(src);
        assert!(out.ok(), "{:?}", out.diagnostics);
        let mut s = Sim::new();
        s.load_code(out.bytes());
        s
    }

    fn steps(s: &mut Sim, n: usize) {
        for _ in 0..n {
            s.step().unwrap();
        }
    }

    #[test]
    fn serial_hello_greets_and_echoes() {
        let mut s = load(include_str!("../../examples/serial_hello.asm"));
        steps(&mut s, 60_000);
        assert_eq!(
            String::from_utf8_lossy(&s.uart.tx_log),
            "Hello from the 8051!\r\nType something: "
        );
        s.uart_send(b"hi\r");
        steps(&mut s, 20_000);
        assert!(String::from_utf8_lossy(&s.uart.tx_log).ends_with("hi\r\n"));
    }

    #[test]
    fn timer_irq_ticks() {
        let mut s = load(include_str!("../../examples/timer_irq.asm"));
        steps(&mut s, 5_000);
        // ~10000 cycles at one interrupt per 1000 cycles.
        assert!((3..=10).contains(&s.reg(7)), "R7 = {}", s.reg(7));
    }

    #[test]
    fn button_counts_edges_while_idle() {
        let mut s = load(include_str!("../../examples/button_int0.asm"));
        steps(&mut s, 20);
        assert!(s.idle);
        for _ in 0..3 {
            s.pins[3] &= !0x04;
            steps(&mut s, 10);
            s.pins[3] |= 0x04;
            steps(&mut s, 10);
        }
        assert_eq!(s.sfr(crate::sfr::P1), 3);
    }

    #[test]
    fn bcd_and_lookup() {
        let mut s = load(include_str!("../../examples/bcd_add.asm"));
        steps(&mut s, 20);
        assert_eq!((s.iram[0x31], s.iram[0x30]), (0x69, 0x12));
        let mut s = load(include_str!("../../examples/lookup_movc.asm"));
        steps(&mut s, 3 + 4 * 5 + 3); // setup, digits 0-3, then up to MOV P2,A
        assert_eq!(s.sfr(crate::sfr::P2), 0x66); // digit 4
    }
}

#[cfg(test)]
mod run_tests {
    use super::{Sim, Stop};
    use crate::asm::assemble;
    use std::collections::HashSet;

    #[test]
    fn breakpoint_at_chunk_boundary_is_hit() {
        let out = assemble("l: NOP\nNOP\nSJMP l");
        let mut s = Sim::new();
        s.load_code(out.bytes());
        let bps = HashSet::from([1u16]);
        assert_eq!(s.run(1, &bps, true), Stop::Budget); // stops at PC 1 by budget
        assert_eq!(s.run(10, &bps, false), Stop::Breakpoint(1)); // next chunk must not skip it
        assert_eq!(s.run(10, &bps, true), Stop::Breakpoint(1)); // resuming loops back to it
        assert_eq!(s.pc, 1);
    }
}
