//! The 8051 instruction set as a single opcode table, shared by the
//! assembler (pattern -> opcode), the disassembler and the simulator
//! (sizes and machine-cycle counts).

use std::sync::OnceLock;

/// Operand forms as they appear in an instruction encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    A,
    AB,
    C,
    Dptr,
    Reg(u8),
    AtReg(u8),
    AtDptr,
    AtAPlusDptr,
    AtAPlusPc,
    Imm8,
    Imm16,
    Direct,
    Bit,
    NotBit,
    Addr11,
    Addr16,
    Rel,
}

impl Operand {
    /// Bytes this operand occupies after the opcode byte.
    pub(crate) fn len(self) -> u8 {
        match self {
            Operand::Imm8 | Operand::Direct | Operand::Bit | Operand::NotBit => 1,
            Operand::Addr11 | Operand::Rel => 1,
            Operand::Imm16 | Operand::Addr16 => 2,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OpInfo {
    pub opcode: u8,
    pub mnemonic: &'static str,
    pub operands: &'static [Operand],
    pub size: u8,
    pub cycles: u8,
}

use Operand::*;

const RN: [[Operand; 1]; 8] = [
    [Reg(0)],
    [Reg(1)],
    [Reg(2)],
    [Reg(3)],
    [Reg(4)],
    [Reg(5)],
    [Reg(6)],
    [Reg(7)],
];
const A_RN: [[Operand; 2]; 8] = [
    [A, Reg(0)],
    [A, Reg(1)],
    [A, Reg(2)],
    [A, Reg(3)],
    [A, Reg(4)],
    [A, Reg(5)],
    [A, Reg(6)],
    [A, Reg(7)],
];
const RN_A: [[Operand; 2]; 8] = [
    [Reg(0), A],
    [Reg(1), A],
    [Reg(2), A],
    [Reg(3), A],
    [Reg(4), A],
    [Reg(5), A],
    [Reg(6), A],
    [Reg(7), A],
];
const RN_IMM: [[Operand; 2]; 8] = [
    [Reg(0), Imm8],
    [Reg(1), Imm8],
    [Reg(2), Imm8],
    [Reg(3), Imm8],
    [Reg(4), Imm8],
    [Reg(5), Imm8],
    [Reg(6), Imm8],
    [Reg(7), Imm8],
];
const DIR_RN: [[Operand; 2]; 8] = [
    [Direct, Reg(0)],
    [Direct, Reg(1)],
    [Direct, Reg(2)],
    [Direct, Reg(3)],
    [Direct, Reg(4)],
    [Direct, Reg(5)],
    [Direct, Reg(6)],
    [Direct, Reg(7)],
];
const RN_DIR: [[Operand; 2]; 8] = [
    [Reg(0), Direct],
    [Reg(1), Direct],
    [Reg(2), Direct],
    [Reg(3), Direct],
    [Reg(4), Direct],
    [Reg(5), Direct],
    [Reg(6), Direct],
    [Reg(7), Direct],
];
const RN_REL: [[Operand; 2]; 8] = [
    [Reg(0), Rel],
    [Reg(1), Rel],
    [Reg(2), Rel],
    [Reg(3), Rel],
    [Reg(4), Rel],
    [Reg(5), Rel],
    [Reg(6), Rel],
    [Reg(7), Rel],
];
const RN_IMM_REL: [[Operand; 3]; 8] = [
    [Reg(0), Imm8, Rel],
    [Reg(1), Imm8, Rel],
    [Reg(2), Imm8, Rel],
    [Reg(3), Imm8, Rel],
    [Reg(4), Imm8, Rel],
    [Reg(5), Imm8, Rel],
    [Reg(6), Imm8, Rel],
    [Reg(7), Imm8, Rel],
];
const RI: [[Operand; 1]; 2] = [[AtReg(0)], [AtReg(1)]];
const A_RI: [[Operand; 2]; 2] = [[A, AtReg(0)], [A, AtReg(1)]];
const RI_A: [[Operand; 2]; 2] = [[AtReg(0), A], [AtReg(1), A]];
const RI_IMM: [[Operand; 2]; 2] = [[AtReg(0), Imm8], [AtReg(1), Imm8]];
const DIR_RI: [[Operand; 2]; 2] = [[Direct, AtReg(0)], [Direct, AtReg(1)]];
const RI_DIR: [[Operand; 2]; 2] = [[AtReg(0), Direct], [AtReg(1), Direct]];
const RI_IMM_REL: [[Operand; 3]; 2] = [[AtReg(0), Imm8, Rel], [AtReg(1), Imm8, Rel]];

/// Fixed (non register-indexed) opcodes.
const FIXED: &[(u8, &str, &[Operand], u8)] = &[
    (0x00, "NOP", &[], 1),
    (0x02, "LJMP", &[Addr16], 2),
    (0x03, "RR", &[A], 1),
    (0x04, "INC", &[A], 1),
    (0x05, "INC", &[Direct], 1),
    (0x10, "JBC", &[Bit, Rel], 2),
    (0x12, "LCALL", &[Addr16], 2),
    (0x13, "RRC", &[A], 1),
    (0x14, "DEC", &[A], 1),
    (0x15, "DEC", &[Direct], 1),
    (0x20, "JB", &[Bit, Rel], 2),
    (0x22, "RET", &[], 2),
    (0x23, "RL", &[A], 1),
    (0x24, "ADD", &[A, Imm8], 1),
    (0x25, "ADD", &[A, Direct], 1),
    (0x30, "JNB", &[Bit, Rel], 2),
    (0x32, "RETI", &[], 2),
    (0x33, "RLC", &[A], 1),
    (0x34, "ADDC", &[A, Imm8], 1),
    (0x35, "ADDC", &[A, Direct], 1),
    (0x40, "JC", &[Rel], 2),
    (0x42, "ORL", &[Direct, A], 1),
    (0x43, "ORL", &[Direct, Imm8], 2),
    (0x44, "ORL", &[A, Imm8], 1),
    (0x45, "ORL", &[A, Direct], 1),
    (0x50, "JNC", &[Rel], 2),
    (0x52, "ANL", &[Direct, A], 1),
    (0x53, "ANL", &[Direct, Imm8], 2),
    (0x54, "ANL", &[A, Imm8], 1),
    (0x55, "ANL", &[A, Direct], 1),
    (0x60, "JZ", &[Rel], 2),
    (0x62, "XRL", &[Direct, A], 1),
    (0x63, "XRL", &[Direct, Imm8], 2),
    (0x64, "XRL", &[A, Imm8], 1),
    (0x65, "XRL", &[A, Direct], 1),
    (0x70, "JNZ", &[Rel], 2),
    (0x72, "ORL", &[C, Bit], 2),
    (0x73, "JMP", &[AtAPlusDptr], 2),
    (0x74, "MOV", &[A, Imm8], 1),
    (0x75, "MOV", &[Direct, Imm8], 2),
    (0x80, "SJMP", &[Rel], 2),
    (0x82, "ANL", &[C, Bit], 2),
    (0x83, "MOVC", &[A, AtAPlusPc], 2),
    (0x84, "DIV", &[AB], 4),
    // Encoded as: opcode, src, dest (handled specially by the encoder).
    (0x85, "MOV", &[Direct, Direct], 2),
    (0x90, "MOV", &[Dptr, Imm16], 2),
    (0x92, "MOV", &[Bit, C], 2),
    (0x93, "MOVC", &[A, AtAPlusDptr], 2),
    (0x94, "SUBB", &[A, Imm8], 1),
    (0x95, "SUBB", &[A, Direct], 1),
    (0xA0, "ORL", &[C, NotBit], 2),
    (0xA2, "MOV", &[C, Bit], 1),
    (0xA3, "INC", &[Dptr], 2),
    (0xA4, "MUL", &[AB], 4),
    (0xB0, "ANL", &[C, NotBit], 2),
    (0xB2, "CPL", &[Bit], 1),
    (0xB3, "CPL", &[C], 1),
    (0xB4, "CJNE", &[A, Imm8, Rel], 2),
    (0xB5, "CJNE", &[A, Direct, Rel], 2),
    (0xC0, "PUSH", &[Direct], 2),
    (0xC2, "CLR", &[Bit], 1),
    (0xC3, "CLR", &[C], 1),
    (0xC4, "SWAP", &[A], 1),
    (0xC5, "XCH", &[A, Direct], 1),
    (0xD0, "POP", &[Direct], 2),
    (0xD2, "SETB", &[Bit], 1),
    (0xD3, "SETB", &[C], 1),
    (0xD4, "DA", &[A], 1),
    (0xD5, "DJNZ", &[Direct, Rel], 2),
    (0xE0, "MOVX", &[A, AtDptr], 2),
    (0xE4, "CLR", &[A], 1),
    (0xE5, "MOV", &[A, Direct], 1),
    (0xF0, "MOVX", &[AtDptr, A], 2),
    (0xF4, "CPL", &[A], 1),
    (0xF5, "MOV", &[Direct, A], 1),
];

/// Register-indexed groups: (base opcode, mnemonic, cycles, @Ri forms, Rn forms).
type Group = (
    u8,
    &'static str,
    u8,
    Option<&'static [[Operand; 2]; 2]>,
    Option<&'static [[Operand; 2]; 8]>,
);
const GROUPS2: &[Group] = &[
    (0x20, "ADD", 1, Some(&A_RI), Some(&A_RN)),
    (0x30, "ADDC", 1, Some(&A_RI), Some(&A_RN)),
    (0x40, "ORL", 1, Some(&A_RI), Some(&A_RN)),
    (0x50, "ANL", 1, Some(&A_RI), Some(&A_RN)),
    (0x60, "XRL", 1, Some(&A_RI), Some(&A_RN)),
    (0x70, "MOV", 1, Some(&RI_IMM), Some(&RN_IMM)),
    (0x80, "MOV", 2, Some(&DIR_RI), Some(&DIR_RN)),
    (0x90, "SUBB", 1, Some(&A_RI), Some(&A_RN)),
    (0xA0, "MOV", 2, Some(&RI_DIR), Some(&RN_DIR)),
    (0xC0, "XCH", 1, Some(&A_RI), Some(&A_RN)),
    (0xD0, "XCHD", 1, Some(&A_RI), None),
    (0xD0, "DJNZ", 2, None, Some(&RN_REL)),
    (0xE0, "MOV", 1, Some(&A_RI), Some(&A_RN)),
    (0xF0, "MOV", 1, Some(&RI_A), Some(&RN_A)),
];

fn size_of(ops: &[Operand]) -> u8 {
    1 + ops.iter().map(|o| o.len()).sum::<u8>()
}

fn build() -> Vec<Option<OpInfo>> {
    let mut t: Vec<Option<OpInfo>> = vec![None; 256];
    let mut put = |opcode: u8, mnemonic: &'static str, operands: &'static [Operand], cycles: u8| {
        assert!(
            t[opcode as usize].is_none(),
            "duplicate opcode {opcode:02X}"
        );
        t[opcode as usize] = Some(OpInfo {
            opcode,
            mnemonic,
            operands,
            size: size_of(operands),
            cycles,
        });
    };

    for &(op, m, ops, cyc) in FIXED {
        put(op, m, ops, cyc);
    }
    // AJMP / ACALL: addr11 high bits live in the opcode's top three bits.
    for page in 0..8u8 {
        put((page << 5) | 0x01, "AJMP", &[Addr11], 2);
        put((page << 5) | 0x11, "ACALL", &[Addr11], 2);
    }
    for i in 0..2 {
        put(0x06 + i as u8, "INC", &RI[i], 1);
        put(0x16 + i as u8, "DEC", &RI[i], 1);
        put(0xB6 + i as u8, "CJNE", &RI_IMM_REL[i], 2);
        put(0xE2 + i as u8, "MOVX", &A_RI[i], 2);
        put(0xF2 + i as u8, "MOVX", &RI_A[i], 2);
    }
    for n in 0..8 {
        put(0x08 + n as u8, "INC", &RN[n], 1);
        put(0x18 + n as u8, "DEC", &RN[n], 1);
        put(0xB8 + n as u8, "CJNE", &RN_IMM_REL[n], 2);
    }
    for &(base, m, cyc, ri, rn) in GROUPS2 {
        if let Some(ri) = ri {
            for (i, ops) in ri.iter().enumerate() {
                put(base + 6 + i as u8, m, ops, cyc);
            }
        }
        if let Some(rn) = rn {
            for (n, ops) in rn.iter().enumerate() {
                put(base + 8 + n as u8, m, ops, cyc);
            }
        }
    }
    t
}

fn table() -> &'static [Option<OpInfo>] {
    static TABLE: OnceLock<Vec<Option<OpInfo>>> = OnceLock::new();
    TABLE.get_or_init(build)
}

/// Info for an opcode; `None` only for the reserved opcode 0xA5.
pub fn info(opcode: u8) -> Option<&'static OpInfo> {
    table()[opcode as usize].as_ref()
}

/// All defined opcodes with the given mnemonic (upper case).
pub fn by_mnemonic(mnemonic: &str) -> impl Iterator<Item = &'static OpInfo> + '_ {
    table()
        .iter()
        .flatten()
        .filter(move |i| i.mnemonic == mnemonic)
}

pub fn is_mnemonic(s: &str) -> bool {
    matches!(s, "JMP" | "CALL") || table().iter().flatten().any(|i| i.mnemonic == s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a5_is_undefined() {
        let undefined: Vec<_> = (0..=255u8).filter(|&op| info(op).is_none()).collect();
        assert_eq!(undefined, vec![0xA5]);
    }

    #[test]
    fn spot_check_sizes_and_cycles() {
        let i = info(0x85).unwrap();
        assert_eq!((i.mnemonic, i.size, i.cycles), ("MOV", 3, 2));
        let i = info(0xB8).unwrap();
        assert_eq!((i.mnemonic, i.size, i.cycles), ("CJNE", 3, 2));
        let i = info(0xD6).unwrap();
        assert_eq!((i.mnemonic, i.size), ("XCHD", 1));
        let i = info(0xA4).unwrap();
        assert_eq!((i.mnemonic, i.cycles), ("MUL", 4));
        let i = info(0xF1).unwrap();
        assert_eq!((i.mnemonic, i.size), ("ACALL", 2));
        let i = info(0x90).unwrap();
        assert_eq!((i.mnemonic, i.size), ("MOV", 3));
    }
}
