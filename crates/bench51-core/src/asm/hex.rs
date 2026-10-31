//! Intel HEX reading and writing.

use std::collections::BTreeMap;
use std::fmt::Write;

const RECORD_LEN: usize = 16;

/// Writes data records for contiguous runs of bytes, then the EOF record.
pub fn write(bytes: impl Iterator<Item = (u16, u8)>) -> String {
    let image: BTreeMap<u16, u8> = bytes.collect();
    let mut out = String::new();
    let mut run: Vec<u8> = Vec::new();
    let mut run_start = 0u16;
    let mut flush = |start: u16, data: &mut Vec<u8>| {
        for (i, chunk) in data.chunks(RECORD_LEN).enumerate() {
            record(
                &mut out,
                start.wrapping_add((i * RECORD_LEN) as u16),
                0x00,
                chunk,
            );
        }
        data.clear();
    };
    for (&addr, &b) in &image {
        if !run.is_empty() && run_start.wrapping_add(run.len() as u16) != addr {
            flush(run_start, &mut run);
        }
        if run.is_empty() {
            run_start = addr;
        }
        run.push(b);
    }
    flush(run_start, &mut run);
    record(&mut out, 0, 0x01, &[]);
    out
}

fn record(out: &mut String, addr: u16, kind: u8, data: &[u8]) {
    let mut sum = data.len() as u8;
    sum = sum
        .wrapping_add((addr >> 8) as u8)
        .wrapping_add(addr as u8)
        .wrapping_add(kind);
    let _ = write!(out, ":{:02X}{addr:04X}{kind:02X}", data.len());
    for &b in data {
        sum = sum.wrapping_add(b);
        let _ = write!(out, "{b:02X}");
    }
    let _ = writeln!(out, "{:02X}", sum.wrapping_neg());
}

/// Parses Intel HEX into (address, byte) pairs.
pub fn read(text: &str) -> Result<Vec<(u16, u8)>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let fail = |m: &str| format!("line {}: {m}", n + 1);
        let hex = line
            .strip_prefix(':')
            .ok_or_else(|| fail("record must start with ':'"))?;
        if hex.len() % 2 != 0 || hex.len() < 10 {
            return Err(fail("malformed record"));
        }
        let raw = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
            .collect::<Result<Vec<u8>, _>>()
            .map_err(|_| fail("invalid hex digit"))?;
        let len = raw[0] as usize;
        if raw.len() != len + 5 {
            return Err(fail("record length mismatch"));
        }
        if raw.iter().fold(0u8, |a, &b| a.wrapping_add(b)) != 0 {
            return Err(fail("checksum mismatch"));
        }
        let addr = u16::from_be_bytes([raw[1], raw[2]]);
        match raw[3] {
            0x00 => out.extend(
                raw[4..4 + len]
                    .iter()
                    .enumerate()
                    .map(|(i, &b)| (addr.wrapping_add(i as u16), b)),
            ),
            0x01 => break,
            // Extended address / start address records don't apply to a 64K 8051.
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let bytes: Vec<(u16, u8)> = (0..20u16)
            .map(|i| (i, i as u8))
            .chain([(0x100, 0xAA)])
            .collect();
        let text = write(bytes.iter().copied());
        assert!(text.ends_with(":00000001FF\n"));
        assert_eq!(read(&text).unwrap(), bytes);
    }

    #[test]
    fn known_record() {
        // MOV A,#55h ; SJMP $
        let text = write([(0, 0x74), (1, 0x55), (2, 0x80), (3, 0xFE)].into_iter());
        assert_eq!(text.lines().next().unwrap(), ":04000000745580FEB5");
    }
}
