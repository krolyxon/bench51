//! Loading programs: assembly sources as they are, Intel HEX images as
//! disassembled source.

use std::path::Path;

use crate::asm::hex;
use crate::sfr::{hex8, hex16};
use crate::sim::disasm;

pub enum Loaded {
    Source(String),
    /// A HEX image turned into source that reassembles to the same bytes.
    Hex {
        source: String,
        bytes: usize,
    },
}

pub fn is_hex_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("hex") || e.eq_ignore_ascii_case("ihx"))
}

/// Reads `path`. Errors are ready to show to the user.
pub fn load(path: &Path) -> Result<Loaded, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    if !is_hex_path(path) {
        return Ok(Loaded::Source(text));
    }
    let bytes = hex::read(&text).map_err(|e| format!("invalid HEX file: {e}"))?;
    Ok(Loaded::Hex {
        source: disassemble_image(&bytes, path),
        bytes: bytes.len(),
    })
}

/// Builds an assembler source from a HEX image by disassembling each
/// contiguous run of bytes. Reassembling it reproduces the image exactly.
pub fn disassemble_image(bytes: &[(u16, u8)], path: &Path) -> String {
    let mut code = Box::new([0u8; 0x10000]);
    let mut used = vec![false; 0x10000];
    for &(a, b) in bytes {
        code[a as usize] = b;
        used[a as usize] = true;
    }
    let mut src = format!("; disassembled from {}\n", path.display());
    let mut addr = 0usize;
    while addr < 0x10000 {
        if !used[addr] {
            addr += 1;
            continue;
        }
        src.push_str(&format!("\n        ORG {}\n", hex16(addr as u16)));
        while addr < 0x10000 && used[addr] {
            let d = disasm::disassemble(&code, addr as u16);
            let size = d.size as usize;
            if (addr..addr + size).all(|a| a < 0x10000 && used[a]) {
                src.push_str(&format!("        {:<24}; {:04X}\n", d.text, addr));
                addr += size;
            } else {
                src.push_str(&format!(
                    "        DB {:<21}; {:04X}\n",
                    hex8(code[addr]),
                    addr
                ));
                addr += 1;
            }
        }
    }
    src.push_str("        END\n");
    src
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::asm::{assemble, hex};

    #[test]
    fn hex_disassembly_reassembles_to_the_same_image() {
        let out = assemble(include_str!("../../../examples/serial_hello.asm"));
        assert!(out.ok());
        let image = hex::read(&out.intel_hex()).unwrap();
        let src = super::disassemble_image(&image, Path::new("x.hex"));
        let again = assemble(&src);
        assert!(again.ok(), "{:?}", again.diagnostics);
        let mut a: Vec<_> = out.bytes().collect();
        let mut b: Vec<_> = again.bytes().collect();
        a.sort();
        b.sort();
        assert_eq!(a, b);
    }
}
