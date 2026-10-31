//! Special function register and bit addresses of the standard 8051.

pub const P0: u8 = 0x80;
pub const SP: u8 = 0x81;
pub const DPL: u8 = 0x82;
pub const DPH: u8 = 0x83;
pub const PCON: u8 = 0x87;
pub const TCON: u8 = 0x88;
pub const TMOD: u8 = 0x89;
pub const TL0: u8 = 0x8A;
pub const TL1: u8 = 0x8B;
pub const TH0: u8 = 0x8C;
pub const TH1: u8 = 0x8D;
pub const P1: u8 = 0x90;
pub const SCON: u8 = 0x98;
pub const SBUF: u8 = 0x99;
pub const P2: u8 = 0xA0;
pub const IE: u8 = 0xA8;
pub const P3: u8 = 0xB0;
pub const IP: u8 = 0xB8;
pub const PSW: u8 = 0xD0;
pub const ACC: u8 = 0xE0;
pub const B: u8 = 0xF0;

pub const SFRS: &[(&str, u8)] = &[
    ("P0", P0),
    ("SP", SP),
    ("DPL", DPL),
    ("DPH", DPH),
    ("PCON", PCON),
    ("TCON", TCON),
    ("TMOD", TMOD),
    ("TL0", TL0),
    ("TL1", TL1),
    ("TH0", TH0),
    ("TH1", TH1),
    ("P1", P1),
    ("SCON", SCON),
    ("SBUF", SBUF),
    ("P2", P2),
    ("IE", IE),
    ("P3", P3),
    ("IP", IP),
    ("PSW", PSW),
    ("ACC", ACC),
    ("B", B),
];

pub const BITS: &[(&str, u8)] = &[
    ("IT0", 0x88),
    ("IE0", 0x89),
    ("IT1", 0x8A),
    ("IE1", 0x8B),
    ("TR0", 0x8C),
    ("TF0", 0x8D),
    ("TR1", 0x8E),
    ("TF1", 0x8F),
    ("RI", 0x98),
    ("TI", 0x99),
    ("RB8", 0x9A),
    ("TB8", 0x9B),
    ("REN", 0x9C),
    ("SM2", 0x9D),
    ("SM1", 0x9E),
    ("SM0", 0x9F),
    ("EX0", 0xA8),
    ("ET0", 0xA9),
    ("EX1", 0xAA),
    ("ET1", 0xAB),
    ("ES", 0xAC),
    ("EA", 0xAF),
    ("RXD", 0xB0),
    ("TXD", 0xB1),
    ("INT0", 0xB2),
    ("INT1", 0xB3),
    ("T0", 0xB4),
    ("T1", 0xB5),
    ("WR", 0xB6),
    ("RD", 0xB7),
    ("PX0", 0xB8),
    ("PT0", 0xB9),
    ("PX1", 0xBA),
    ("PT1", 0xBB),
    ("PS", 0xBC),
    ("P", 0xD0),
    ("OV", 0xD2),
    ("RS0", 0xD3),
    ("RS1", 0xD4),
    ("F0", 0xD5),
    ("AC", 0xD6),
    ("CY", 0xD7),
];

pub fn sfr_name(addr: u8) -> Option<&'static str> {
    SFRS.iter().find(|&&(_, a)| a == addr).map(|&(n, _)| n)
}

pub fn bit_name(addr: u8) -> Option<&'static str> {
    BITS.iter().find(|&&(_, a)| a == addr).map(|&(n, _)| n)
}

/// Byte address and bit index of a bit address.
pub fn bit_location(bit: u8) -> (u8, u8) {
    if bit < 0x80 {
        (0x20 + bit / 8, bit % 8)
    } else {
        (bit & 0xF8, bit & 7)
    }
}

/// Human-readable bit operand, e.g. `TR0`, `P1.3` or `20h.5`.
pub fn format_bit(bit: u8) -> String {
    if let Some(n) = bit_name(bit) {
        return n.to_string();
    }
    let (byte, idx) = bit_location(bit);
    match sfr_name(byte) {
        Some(n) => format!("{n}.{idx}"),
        None => format!("{}.{idx}", hex8(byte)),
    }
}

/// Human-readable direct address: SFR name or hex.
pub fn format_direct(addr: u8) -> String {
    match sfr_name(addr) {
        Some(n) => n.to_string(),
        None => hex8(addr),
    }
}

/// Assembler-style hex literal, with a leading zero when needed (`0A0h`).
pub fn hex8(v: u8) -> String {
    if v >= 0xA0 {
        format!("0{v:02X}h")
    } else {
        format!("{v:02X}h")
    }
}

pub fn hex16(v: u16) -> String {
    if v >= 0xA000 {
        format!("0{v:04X}h")
    } else {
        format!("{v:04X}h")
    }
}
