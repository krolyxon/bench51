//! Timers 0/1, the UART and interrupt dispatch, advanced per machine cycle.

use std::collections::VecDeque;

use super::Sim;
use crate::sfr::{IE, IP, PCON, SCON, TCON, TH0, TH1, TL0, TL1, TMOD};

// TCON bits
const TF1: u8 = 0x80;
const TR1: u8 = 0x40;
const TF0: u8 = 0x20;
const TR0: u8 = 0x10;
const IE1: u8 = 0x08;
const IT1: u8 = 0x04;
const IE0: u8 = 0x02;
const IT0: u8 = 0x01;
// SCON bits
const SM2: u8 = 0x20;
const REN: u8 = 0x10;
const RB8: u8 = 0x04;
const TI: u8 = 0x02;
const RI: u8 = 0x01;
// P3 pins
const INT0_PIN: u8 = 0x04;
const INT1_PIN: u8 = 0x08;
const T0_PIN: u8 = 0x10;
const T1_PIN: u8 = 0x20;

#[derive(Debug, Clone, Copy)]
struct Shift {
    byte: u8,
    /// Remaining time in the mode's unit (see `Sim::uart_units`).
    remaining: u32,
}

#[derive(Debug, Default)]
pub struct Uart {
    /// Everything the 8051 has transmitted.
    pub tx_log: Vec<u8>,
    /// Bytes waiting to be received by the 8051.
    pub rx_queue: VecDeque<u8>,
    /// The receive half of SBUF.
    pub sbuf_rx: u8,
    tx: Option<Shift>,
    rx: Option<Shift>,
}

impl Uart {
    /// Clears the shift registers and the TX log; queued input is kept.
    pub fn reset(&mut self) {
        self.tx_log.clear();
        self.sbuf_rx = 0;
        self.tx = None;
        self.rx = None;
    }

    pub fn transmitting(&self) -> bool {
        self.tx.is_some()
    }
}

/// Interrupt sources in hardware polling order: (flag test, IE/IP bit, vector).
const SOURCES: [(u8, u16); 5] = [
    (0x01, 0x03),
    (0x02, 0x0B),
    (0x04, 0x13),
    (0x08, 0x1B),
    (0x10, 0x23),
];

impl Sim {
    pub(super) fn tick(&mut self, cycles: u32) {
        for _ in 0..cycles {
            self.cycles += 1;
            let p3 = self.port_pins(3);
            let fell = self.prev_p3 & !p3;
            self.prev_p3 = p3;
            self.external_interrupts(p3, fell);
            let t1_overflows = self.timers(p3, fell);
            self.uart_tick(t1_overflows);
        }
    }

    fn external_interrupts(&mut self, p3: u8, fell: u8) {
        let mut tcon = self.sfr(TCON);
        for (pin, it, ie) in [(INT0_PIN, IT0, IE0), (INT1_PIN, IT1, IE1)] {
            if tcon & it != 0 {
                if fell & pin != 0 {
                    tcon |= ie;
                }
            } else if p3 & pin == 0 {
                tcon |= ie;
            } else {
                tcon &= !ie;
            }
        }
        self.set_sfr(TCON, tcon);
    }

    /// Advances both timers by one machine cycle. Returns the number of
    /// timer 1 overflows (the UART baud clock in modes 1 and 3).
    fn timers(&mut self, p3: u8, fell: u8) -> u32 {
        let tmod = self.sfr(TMOD);
        let mut tcon = self.sfr(TCON);
        let t0_mode = tmod & 0x03;
        let mut t1_overflows = 0;

        for t in 0..2u8 {
            let ctl = (tmod >> (4 * t)) & 0x0F;
            let mode = ctl & 0x03;
            let (tr, int_pin, count_pin) = if t == 0 {
                (TR0, INT0_PIN, T0_PIN)
            } else {
                (TR1, INT1_PIN, T1_PIN)
            };
            let gate_ok = ctl & 0x08 == 0 || p3 & int_pin != 0;
            let running = tcon & tr != 0 && gate_ok;
            let event = if ctl & 0x04 != 0 {
                fell & count_pin != 0
            } else {
                true
            };

            // Timer 1 is stopped in mode 3; timer 0 mode 3 also clocks TH0 from TR1.
            if t == 1 && mode == 3 {
                continue;
            }
            let (tl_addr, th_addr) = if t == 0 { (TL0, TH0) } else { (TL1, TH1) };
            if t == 0 && mode == 3 && tcon & TR1 != 0 {
                let th = self.sfr(TH0).wrapping_add(1);
                self.set_sfr(TH0, th);
                if th == 0 {
                    tcon |= TF1;
                }
            }
            if !(running && event) {
                continue;
            }
            let (tl, th) = (self.sfr(tl_addr), self.sfr(th_addr));
            let (tl, th, overflow) = match mode {
                0 => {
                    let low = (tl.wrapping_add(1)) & 0x1F;
                    let th2 = if low == 0 { th.wrapping_add(1) } else { th };
                    ((tl & 0xE0) | low, th2, low == 0 && th2 == 0)
                }
                1 => {
                    let v = u16::from_be_bytes([th, tl]).wrapping_add(1);
                    let [h, l] = v.to_be_bytes();
                    (l, h, v == 0)
                }
                2 => {
                    let l = tl.wrapping_add(1);
                    if l == 0 {
                        (th, th, true)
                    } else {
                        (l, th, false)
                    }
                }
                _ => {
                    // Timer 0 mode 3: TL0 is an 8-bit timer.
                    let l = tl.wrapping_add(1);
                    (l, th, l == 0)
                }
            };
            self.set_sfr(tl_addr, tl);
            self.set_sfr(th_addr, th);
            if overflow {
                if t == 1 {
                    t1_overflows += 1;
                    // With timer 0 in mode 3, TF1 belongs to TH0.
                    if t0_mode != 3 {
                        tcon |= TF1;
                    }
                } else {
                    tcon |= TF0;
                }
            }
        }
        self.set_sfr(TCON, tcon);
        t1_overflows
    }

    /// Frame length of the current serial mode, in the units that
    /// `uart_tick` counts: machine cycles (mode 0), oscillator periods
    /// (mode 2) or timer 1 overflows (modes 1 and 3).
    fn uart_frame(&self) -> u32 {
        let smod = self.sfr(PCON) & 0x80 != 0;
        let bit = |base: u32| if smod { base / 2 } else { base };
        match self.sfr(SCON) >> 6 {
            0 => 8,
            1 => 10 * bit(32),
            2 => 11 * bit(64),
            _ => 11 * bit(32),
        }
    }

    pub(super) fn uart_transmit(&mut self, byte: u8) {
        let remaining = self.uart_frame();
        self.uart.tx = Some(Shift { byte, remaining });
    }

    fn uart_tick(&mut self, t1_overflows: u32) {
        let mode = self.sfr(SCON) >> 6;
        let units = match mode {
            0 => 1,
            2 => 12,
            _ => t1_overflows,
        };
        if units == 0 {
            return;
        }
        if let Some(tx) = &mut self.uart.tx {
            tx.remaining = tx.remaining.saturating_sub(units);
            if tx.remaining == 0 {
                self.uart.tx_log.push(tx.byte);
                self.uart.tx = None;
                self.set_sfr(SCON, self.sfr(SCON) | TI);
            }
        }

        let scon = self.sfr(SCON);
        if self.uart.rx.is_none()
            && scon & REN != 0
            && scon & RI == 0
            && let Some(byte) = self.uart.rx_queue.pop_front()
        {
            self.uart.rx = Some(Shift {
                byte,
                remaining: self.uart_frame(),
            });
        }
        if let Some(rx) = &mut self.uart.rx {
            rx.remaining = rx.remaining.saturating_sub(units);
            if rx.remaining == 0 {
                let byte = rx.byte;
                self.uart.rx = None;
                // Terminal input has no 9th data bit; in modes 1-3 RB8 gets
                // the stop bit (1). With SM2 set in modes 2/3 that 9th bit
                // would be 0, so the byte is discarded.
                let mut scon = self.sfr(SCON);
                if !(mode >= 2 && scon & SM2 != 0) {
                    self.uart.sbuf_rx = byte;
                    scon |= RI;
                    if mode == 1 {
                        scon |= RB8;
                    } else if mode >= 2 {
                        scon &= !RB8;
                    }
                }
                self.set_sfr(SCON, scon);
            }
        }
    }

    pub(super) fn service_interrupts(&mut self) {
        if self.irq_hold {
            self.irq_hold = false;
            return;
        }
        let ie = self.sfr(IE);
        if ie & 0x80 == 0 {
            return;
        }
        let tcon = self.sfr(TCON);
        let scon = self.sfr(SCON);
        let pending = |mask: u8| match mask {
            0x01 => tcon & IE0 != 0,
            0x02 => tcon & TF0 != 0,
            0x04 => tcon & IE1 != 0,
            0x08 => tcon & TF1 != 0,
            _ => scon & (RI | TI) != 0,
        };
        let ip = self.sfr(IP);
        let current = self.in_service.last().copied();
        for level in [1u8, 0] {
            if current.is_some_and(|c| c >= level) {
                continue;
            }
            let found = SOURCES.iter().find(|&&(mask, _)| {
                ie & mask != 0 && (ip & mask != 0) == (level == 1) && pending(mask)
            });
            if let Some(&(mask, vector)) = found {
                // Hardware clears timer flags and edge-triggered external flags.
                let mut tcon = self.sfr(TCON);
                match mask {
                    0x01 if tcon & IT0 != 0 => tcon &= !IE0,
                    0x02 => tcon &= !TF0,
                    0x04 if tcon & IT1 != 0 => tcon &= !IE1,
                    0x08 => tcon &= !TF1,
                    _ => {}
                }
                self.set_sfr(TCON, tcon);
                self.idle = false;
                self.set_sfr(PCON, self.sfr(PCON) & !0x01);
                self.push_pc();
                self.pc = vector;
                self.in_service.push(level);
                self.last_irq = Some(vector);
                self.tick(2);
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::asm::assemble;
    use crate::sim::Sim;

    fn sim(src: &str) -> Sim {
        let out = assemble(src);
        assert!(out.ok(), "{:?}", out.diagnostics);
        let mut s = Sim::new();
        s.load_code(out.bytes());
        s
    }

    #[test]
    fn timer0_mode1_interrupt() {
        let mut s = sim("
            ORG 0
            LJMP main
            ORG 0Bh
            INC R7
            RETI
main:       MOV TMOD,#01h
            MOV TH0,#0FFh
            MOV TL0,#0F0h
            SETB ET0
            SETB EA
            SETB TR0
            SJMP $
");
        let mut saw_vector = false;
        for _ in 0..200 {
            s.step().unwrap();
            saw_vector |= s.pc == 0x0B;
        }
        assert!(saw_vector);
        assert!(s.reg(7) >= 1);
        // TF0 is cleared by hardware on vectoring.
        assert_eq!(s.sfr(crate::sfr::TCON) & 0x20, 0);
    }

    #[test]
    fn timer_mode2_auto_reload() {
        let mut s = sim("MOV TMOD,#02h\nMOV TH0,#0F0h\nMOV TL0,#0FEh\nSETB TR0\nSJMP $");
        for _ in 0..8 {
            s.step().unwrap();
        }
        assert!(s.sfr(crate::sfr::TCON) & 0x20 != 0, "TF0 set");
        assert!(s.sfr(crate::sfr::TL0) >= 0xF0);
    }

    #[test]
    fn uart_tx_and_rx() {
        let mut s = sim("
            MOV TMOD,#20h       ; timer 1 mode 2
            MOV TH1,#0FDh       ; 9600 baud @ 11.0592 MHz
            SETB TR1
            MOV SCON,#50h       ; mode 1, REN
            MOV SBUF,#'H'
w:          JNB TI,w
            CLR TI
r:          JNB RI,r
            MOV A,SBUF
            SJMP $
");
        s.uart_send(b"x");
        for _ in 0..10_000 {
            s.step().unwrap();
        }
        assert_eq!(s.uart.tx_log, b"H");
        assert_eq!(s.acc(), b'x');
    }

    #[test]
    fn external_interrupt_edge() {
        let mut s = sim("
            LJMP main
            ORG 03h
            INC R0
            RETI
main:       SETB IT0
            SETB EX0
            SETB EA
            SJMP $
");
        for _ in 0..10 {
            s.step().unwrap();
        }
        s.pins[3] &= !0x04; // pull INT0 low
        for _ in 0..10 {
            s.step().unwrap();
        }
        s.pins[3] |= 0x04;
        for _ in 0..10 {
            s.step().unwrap();
        }
        assert_eq!(s.reg(0), 1, "one falling edge, one interrupt");
    }
}
