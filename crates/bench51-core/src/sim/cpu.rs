//! Instruction execution.

use super::{AC, CY, OV, Sim};
use crate::sfr::{B, P2};

/// Operand location for the register-indexed columns (opcode low nibble 5..F).
#[derive(Clone, Copy)]
enum Loc {
    Direct(u8),
    Iram(u8),
}

impl Sim {
    fn fetch(&mut self) -> u8 {
        let b = self.code[self.pc as usize];
        self.pc = self.pc.wrapping_add(1);
        b
    }

    fn fetch16(&mut self) -> u16 {
        let h = self.fetch();
        let l = self.fetch();
        u16::from_be_bytes([h, l])
    }

    fn rel_jump(&mut self, off: u8) {
        self.pc = self.pc.wrapping_add(off as i8 as u16);
    }

    /// Decodes the operand selected by the low nibble: 5 = direct,
    /// 6/7 = @R0/@R1, 8..F = R0..R7.
    fn loc(&mut self, op: u8) -> Loc {
        match op & 0x0F {
            0x05 => Loc::Direct(self.fetch()),
            0x06 | 0x07 => Loc::Iram(self.reg(op & 1)),
            n => Loc::Iram(self.reg_addr(n & 7)),
        }
    }

    fn read_loc(&self, l: Loc, rmw: bool) -> u8 {
        match l {
            Loc::Direct(a) => self.read_direct(a, rmw),
            Loc::Iram(a) => self.iram[a as usize],
        }
    }

    fn write_loc(&mut self, l: Loc, v: u8) {
        match l {
            Loc::Direct(a) => self.write_direct(a, v),
            Loc::Iram(a) => self.iram[a as usize] = v,
        }
    }

    /// Source operand for the arithmetic/logic rows: 4 = #imm, otherwise `loc`.
    fn src(&mut self, op: u8) -> u8 {
        if op & 0x0F == 0x04 {
            self.fetch()
        } else {
            let l = self.loc(op);
            self.read_loc(l, false)
        }
    }

    fn add(&mut self, v: u8, carry: bool) {
        let a = self.acc();
        let c = carry as u16;
        let sum = a as u16 + v as u16 + c;
        let r = sum as u8;
        self.set_flag(CY, sum > 0xFF);
        self.set_flag(AC, (a & 0x0F) as u16 + (v & 0x0F) as u16 + c > 0x0F);
        self.set_flag(OV, (a ^ r) & (v ^ r) & 0x80 != 0);
        self.set_acc(r);
    }

    fn subb(&mut self, v: u8) {
        let a = self.acc();
        let c = self.flag(CY) as i16;
        let diff = a as i16 - v as i16 - c;
        let r = diff as u8;
        self.set_flag(CY, diff < 0);
        self.set_flag(AC, ((a & 0x0F) as i16) < (v & 0x0F) as i16 + c);
        self.set_flag(OV, (a ^ v) & (a ^ r) & 0x80 != 0);
        self.set_acc(r);
    }

    fn cjne(&mut self, a: u8, b: u8) {
        let off = self.fetch();
        self.set_flag(CY, a < b);
        if a != b {
            self.rel_jump(off);
        }
    }

    fn movx_addr_ri(&self, op: u8) -> u16 {
        // The high address byte comes from the P2 latch.
        u16::from_be_bytes([self.sfr(P2), self.reg(op & 1)])
    }

    pub(super) fn execute(&mut self, op: u8) {
        match op {
            0x00 => {}

            // AJMP / ACALL addr11
            _ if op & 0x1F == 0x01 || op & 0x1F == 0x11 => {
                let lo = self.fetch();
                let target = (self.pc & 0xF800) | ((op as u16 >> 5) << 8) | lo as u16;
                if op & 0x10 != 0 {
                    self.push_pc();
                }
                self.pc = target;
            }
            0x02 => self.pc = self.fetch16(),
            0x12 => {
                let target = self.fetch16();
                self.push_pc();
                self.pc = target;
            }
            0x22 => self.pop_pc(),
            0x32 => {
                self.pop_pc();
                self.in_service.pop();
                self.irq_hold = true;
            }
            0x73 => self.pc = self.dptr().wrapping_add(self.acc() as u16),
            0x80 => {
                let off = self.fetch();
                self.rel_jump(off);
            }

            // Rotates
            0x03 => self.set_acc(self.acc().rotate_right(1)),
            0x23 => self.set_acc(self.acc().rotate_left(1)),
            0x13 => {
                let a = self.acc();
                self.set_acc((a >> 1) | ((self.flag(CY) as u8) << 7));
                self.set_flag(CY, a & 1 != 0);
            }
            0x33 => {
                let a = self.acc();
                self.set_acc((a << 1) | self.flag(CY) as u8);
                self.set_flag(CY, a & 0x80 != 0);
            }

            // INC / DEC
            0x04 => self.set_acc(self.acc().wrapping_add(1)),
            0x14 => self.set_acc(self.acc().wrapping_sub(1)),
            0x05..=0x0F | 0x15..=0x1F => {
                let l = self.loc(op);
                let v = self.read_loc(l, true);
                self.write_loc(
                    l,
                    if op < 0x10 {
                        v.wrapping_add(1)
                    } else {
                        v.wrapping_sub(1)
                    },
                );
            }
            0xA3 => self.set_dptr(self.dptr().wrapping_add(1)),

            // Conditional jumps on bits
            0x10 | 0x20 | 0x30 => {
                let bit = self.fetch();
                let off = self.fetch();
                let set = self.read_bit(bit, op == 0x10);
                if op == 0x10 && set {
                    self.write_bit(bit, false);
                }
                if set == (op != 0x30) {
                    self.rel_jump(off);
                }
            }
            0x40 | 0x50 | 0x60 | 0x70 => {
                let off = self.fetch();
                let take = match op {
                    0x40 => self.flag(CY),
                    0x50 => !self.flag(CY),
                    0x60 => self.acc() == 0,
                    _ => self.acc() != 0,
                };
                if take {
                    self.rel_jump(off);
                }
            }

            // Arithmetic
            0x24..=0x2F => {
                let v = self.src(op);
                self.add(v, false);
            }
            0x34..=0x3F => {
                let v = self.src(op);
                self.add(v, self.flag(CY));
            }
            0x94..=0x9F => {
                let v = self.src(op);
                self.subb(v);
            }
            0x84 => {
                let (a, b) = (self.acc(), self.b());
                self.set_flag(CY, false);
                // Division by zero sets OV and leaves A and B unchanged.
                match a.checked_div(b) {
                    Some(q) => {
                        self.set_acc(q);
                        self.set_sfr(B, a % b);
                        self.set_flag(OV, false);
                    }
                    None => self.set_flag(OV, true),
                }
            }
            0xA4 => {
                let r = self.acc() as u16 * self.b() as u16;
                self.set_acc(r as u8);
                self.set_sfr(B, (r >> 8) as u8);
                self.set_flag(CY, false);
                self.set_flag(OV, r > 0xFF);
            }
            0xD4 => {
                let mut a = self.acc() as u16;
                if a & 0x0F > 9 || self.flag(AC) {
                    a += 0x06;
                }
                if a > 0xFF {
                    self.set_flag(CY, true);
                }
                if (a >> 4) & 0x1F > 9 || self.flag(CY) {
                    a += 0x60;
                }
                if a > 0xFF {
                    self.set_flag(CY, true);
                }
                self.set_acc(a as u8);
            }

            // Logic: ORL/ANL/XRL direct,A / direct,#imm / A,src
            0x42 | 0x43 | 0x52 | 0x53 | 0x62 | 0x63 => {
                let addr = self.fetch();
                let v = if op & 1 == 1 {
                    self.fetch()
                } else {
                    self.acc()
                };
                let d = self.read_direct(addr, true);
                self.write_direct(addr, logic(op, d, v));
            }
            0x44..=0x4F | 0x54..=0x5F | 0x64..=0x6F => {
                let v = self.src(op);
                self.set_acc(logic(op, self.acc(), v));
            }
            0xC4 => self.set_acc(self.acc().rotate_left(4)),
            0xE4 => self.set_acc(0),
            0xF4 => self.set_acc(!self.acc()),

            // Boolean (carry) operations
            0x72 | 0x82 | 0xA0 | 0xB0 => {
                let bit = self.fetch();
                let v = self.read_bit(bit, false) != (op >= 0xA0);
                let c = self.flag(CY);
                self.set_flag(CY, if op & 0x10 == 0 { c || v } else { c && v });
            }
            0x92 => {
                let bit = self.fetch();
                self.write_bit(bit, self.flag(CY));
            }
            0xA2 => {
                let bit = self.fetch();
                self.set_flag(CY, self.read_bit(bit, false));
            }
            0xB2 | 0xC2 | 0xD2 => {
                let bit = self.fetch();
                let v = match op {
                    0xB2 => !self.read_bit(bit, true),
                    0xC2 => false,
                    _ => true,
                };
                self.write_bit(bit, v);
            }
            0xB3 => self.set_flag(CY, !self.flag(CY)),
            0xC3 => self.set_flag(CY, false),
            0xD3 => self.set_flag(CY, true),

            // Data transfer
            0x74 => {
                let v = self.fetch();
                self.set_acc(v);
            }
            0x75 => {
                let addr = self.fetch();
                let v = self.fetch();
                self.write_direct(addr, v);
            }
            0x76..=0x7F => {
                let l = self.loc(op);
                let v = self.fetch();
                self.write_loc(l, v);
            }
            0x85 => {
                let src = self.fetch();
                let dst = self.fetch();
                let v = self.read_direct(src, false);
                self.write_direct(dst, v);
            }
            0x86..=0x8F => {
                let l = self.loc(op);
                let addr = self.fetch();
                let v = self.read_loc(l, false);
                self.write_direct(addr, v);
            }
            0xA6..=0xAF => {
                let l = self.loc(op);
                let addr = self.fetch();
                let v = self.read_direct(addr, false);
                self.write_loc(l, v);
            }
            0xE5..=0xEF => {
                let v = self.src(op);
                self.set_acc(v);
            }
            0xF5..=0xFF => {
                let l = self.loc(op);
                self.write_loc(l, self.acc());
            }
            0x90 => {
                let v = self.fetch16();
                self.set_dptr(v);
            }
            0x83 => self.set_acc(self.code[self.pc.wrapping_add(self.acc() as u16) as usize]),
            0x93 => self.set_acc(self.code[self.dptr().wrapping_add(self.acc() as u16) as usize]),
            0xE0 => self.set_acc(self.xram[self.dptr() as usize]),
            0xF0 => self.xram[self.dptr() as usize] = self.acc(),
            0xE2 | 0xE3 => self.set_acc(self.xram[self.movx_addr_ri(op) as usize]),
            0xF2 | 0xF3 => self.xram[self.movx_addr_ri(op) as usize] = self.acc(),
            0xC0 => {
                let addr = self.fetch();
                let v = self.read_direct(addr, false);
                self.push(v);
            }
            0xD0 => {
                let addr = self.fetch();
                let v = self.pop();
                self.write_direct(addr, v);
            }
            0xC5..=0xCF => {
                let l = self.loc(op);
                let v = self.read_loc(l, true);
                self.write_loc(l, self.acc());
                self.set_acc(v);
            }
            0xD6 | 0xD7 => {
                let addr = self.reg(op & 1) as usize;
                let (a, m) = (self.acc(), self.iram[addr]);
                self.set_acc((a & 0xF0) | (m & 0x0F));
                self.iram[addr] = (m & 0xF0) | (a & 0x0F);
            }

            // Compare and loop
            0xB4 => {
                let v = self.fetch();
                self.cjne(self.acc(), v);
            }
            0xB5 => {
                let addr = self.fetch();
                let v = self.read_direct(addr, false);
                self.cjne(self.acc(), v);
            }
            0xB6..=0xBF => {
                let l = self.loc(op);
                let a = self.read_loc(l, false);
                let v = self.fetch();
                self.cjne(a, v);
            }
            0xD5 | 0xD8..=0xDF => {
                let l = self.loc(op);
                let v = self.read_loc(l, true).wrapping_sub(1);
                self.write_loc(l, v);
                let off = self.fetch();
                if v != 0 {
                    self.rel_jump(off);
                }
            }

            // 0xA5 is reserved and rejected before `execute` is called.
            _ => unreachable!("unhandled opcode {op:02X}"),
        }
    }
}

fn logic(op: u8, a: u8, b: u8) -> u8 {
    match op & 0xF0 {
        0x40 => a | b,
        0x50 => a & b,
        _ => a ^ b,
    }
}

#[cfg(test)]
mod tests {
    use crate::asm::assemble;
    use crate::sim::{AC, CY, OV, PARITY, Sim};
    use std::collections::HashSet;

    /// Assembles `src`, then steps until the PC reaches the `done` label.
    fn run(src: &str) -> Sim {
        let src = format!("{src}\ndone: SJMP done\n");
        let out = assemble(&src);
        assert!(out.ok(), "{:?}", out.diagnostics);
        let done = out.symbols["DONE"].value as u16;
        let mut s = Sim::new();
        s.load_code(out.bytes());
        for _ in 0..100_000 {
            if s.pc == done {
                return s;
            }
            s.step().unwrap();
        }
        panic!("program did not reach 'done'");
    }

    #[test]
    fn add_flags() {
        let s = run("MOV A,#0x7F\nADD A,#1");
        assert_eq!(s.acc(), 0x80);
        assert!(s.flag(OV) && s.flag(AC) && !s.flag(CY));
        assert!(s.flag(PARITY));
        let s = run("MOV A,#0xFF\nSETB C\nADDC A,#0");
        assert_eq!(s.acc(), 0);
        assert!(s.flag(CY) && s.flag(AC) && !s.flag(OV));
    }

    #[test]
    fn subb_flags() {
        let s = run("CLR C\nMOV A,#0x80\nSUBB A,#1");
        assert_eq!(s.acc(), 0x7F);
        assert!(s.flag(OV) && s.flag(AC) && !s.flag(CY));
        let s = run("SETB C\nMOV A,#5\nSUBB A,#5");
        assert_eq!(s.acc(), 0xFF);
        assert!(s.flag(CY));
    }

    #[test]
    fn decimal_adjust() {
        let s = run("MOV A,#0x56\nADD A,#0x67\nDA A");
        assert_eq!(s.acc(), 0x23);
        assert!(s.flag(CY));
        let s = run("MOV A,#0x19\nADD A,#0x28\nDA A");
        assert_eq!(s.acc(), 0x47);
        assert!(!s.flag(CY));
    }

    #[test]
    fn mul_div() {
        let s = run("MOV A,#80\nMOV B,#160\nMUL AB");
        assert_eq!((s.acc(), s.b()), (0x00, 0x32));
        assert!(s.flag(OV));
        let s = run("MOV A,#251\nMOV B,#18\nDIV AB");
        assert_eq!((s.acc(), s.b()), (13, 17));
        let s = run("MOV B,#0\nDIV AB");
        assert!(s.flag(OV));
    }

    #[test]
    fn banks_and_indirect() {
        let s = run("MOV R0,#1\nSETB RS0\nMOV R0,#2\nMOV R1,#40h\nMOV @R1,#99\nMOV A,40h");
        assert_eq!(s.iram[0], 1);
        assert_eq!(s.iram[8], 2);
        assert_eq!(s.acc(), 99);
        assert_eq!(s.bank(), 1);
    }

    #[test]
    fn stack_and_calls() {
        let s = run("MOV A,#5\nPUSH ACC\nMOV A,#0\nPOP B\nCALL sub\nSJMP done\nsub: INC A\nRET");
        assert_eq!(s.b(), 5);
        assert_eq!(s.acc(), 1);
        assert_eq!(s.sp(), 7);
    }

    #[test]
    fn loops_and_compares() {
        let s = run("MOV R2,#10\nCLR A\nl: ADD A,#2\nDJNZ R2,l");
        assert_eq!(s.acc(), 20);
        let s = run("MOV A,#3\nCJNE A,#5,ne\nne: NOP");
        assert!(s.flag(CY));
    }

    #[test]
    fn bits() {
        let s = run("SETB 20h.3\nMOV C,20h.3\nCPL ACC.0\nANL C,/ACC.0\nMOV 21h.0,C");
        assert_eq!(s.iram[0x20], 0x08);
        assert_eq!(s.acc(), 1);
        assert_eq!(s.iram[0x21], 0);
        let s = run("SETB 20h.1\nJBC 20h.1,x\nx: NOP");
        assert_eq!(s.iram[0x20], 0);
    }

    #[test]
    fn movc_movx_xchd() {
        let s = run(
            "MOV DPTR,#tbl\nMOV A,#2\nMOVC A,@A+DPTR\nSJMP go\ntbl: DB 10,20,30\ngo: MOV DPTR,#1234h\nMOVX @DPTR,A\nMOV R0,#30h\nMOV @R0,#0x5A\nMOV A,#0xC3\nXCHD A,@R0",
        );
        assert_eq!(s.xram[0x1234], 30);
        assert_eq!(s.acc(), 0xCA);
        assert_eq!(s.iram[0x30], 0x53);
    }

    #[test]
    fn ports_read_pins_but_rmw_reads_latch() {
        let mut s = run("MOV P1,#0FFh");
        s.pins[1] = 0xF0;
        assert_eq!(s.read_direct(0x90, false), 0xF0);
        assert_eq!(s.read_direct(0x90, true), 0xFF);
    }

    #[test]
    fn step_over_and_out() {
        let out = assemble("CALL f\nNOP\nSJMP $\nf: MOV A,#1\nMOV A,#2\nRET");
        let mut s = Sim::new();
        s.load_code(out.bytes());
        let none = HashSet::new();
        s.step_over(1000, &none);
        assert_eq!((s.pc, s.acc()), (3, 2));
        s.reset();
        s.step().unwrap(); // into f
        s.step_out(1000, &none);
        assert_eq!(s.pc, 3);
    }
}
