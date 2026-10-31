use crate::isa::{self, Operand};
use crate::sfr::{format_bit, format_direct, hex8, hex16};

pub struct Disasm {
    pub text: String,
    pub size: u8,
}

/// Disassembles the instruction at `addr`.
pub fn disassemble(code: &[u8; 0x10000], addr: u16) -> Disasm {
    let byte = |i: u16| code[addr.wrapping_add(i) as usize];
    let op = byte(0);
    let Some(info) = isa::info(op) else {
        return Disasm {
            text: format!("DB {}", hex8(op)),
            size: 1,
        };
    };
    let next = addr.wrapping_add(info.size as u16);
    let mut pos = 1u16;
    let mut parts: Vec<String> = Vec::new();
    for &o in info.operands {
        let s = match o {
            Operand::A => "A".into(),
            Operand::AB => "AB".into(),
            Operand::C => "C".into(),
            Operand::Dptr => "DPTR".into(),
            Operand::Reg(n) => format!("R{n}"),
            Operand::AtReg(n) => format!("@R{n}"),
            Operand::AtDptr => "@DPTR".into(),
            Operand::AtAPlusDptr => "@A+DPTR".into(),
            Operand::AtAPlusPc => "@A+PC".into(),
            Operand::Imm8 => format!("#{}", hex8(byte(pos))),
            Operand::Imm16 => format!("#{}", hex16(u16::from_be_bytes([byte(pos), byte(pos + 1)]))),
            Operand::Direct => format_direct(byte(pos)),
            Operand::Bit => format_bit(byte(pos)),
            Operand::NotBit => format!("/{}", format_bit(byte(pos))),
            Operand::Addr11 => {
                hex16((next & 0xF800) | (((op >> 5) as u16) << 8) | byte(pos) as u16)
            }
            Operand::Addr16 => hex16(u16::from_be_bytes([byte(pos), byte(pos + 1)])),
            Operand::Rel => hex16(next.wrapping_add(byte(pos) as i8 as u16)),
        };
        pos += o.len() as u16;
        parts.push(s);
    }
    if op == 0x85 {
        // Encoded as source, destination.
        parts.swap(0, 1);
    }
    let text = if parts.is_empty() {
        info.mnemonic.to_string()
    } else {
        format!("{:<5} {}", info.mnemonic, parts.join(","))
    };
    Disasm {
        text,
        size: info.size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asm::assemble;

    /// Disassembling and re-assembling every opcode gives the same bytes.
    #[test]
    fn round_trips_every_opcode() {
        for op in 0..=255u8 {
            if op == 0xA5 {
                continue;
            }
            let mut code = Box::new([0u8; 0x10000]);
            code[0x100] = op;
            code[0x101] = 0x30;
            code[0x102] = 0x40;
            let d = disassemble(&code, 0x100);
            let src = format!("ORG 100h\n{}", d.text);
            let out = assemble(&src);
            assert!(out.ok(), "{op:02X} '{}': {:?}", d.text, out.diagnostics);
            let bytes: Vec<u8> = out.bytes().map(|b| b.1).collect();
            assert_eq!(
                bytes,
                code[0x100..0x100 + d.size as usize],
                "opcode {op:02X} '{}'",
                d.text
            );
        }
    }
}
