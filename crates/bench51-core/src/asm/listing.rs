use std::fmt::Write;

use super::AsmOutput;

const BYTES_PER_ROW: usize = 4;

/// Keil A51-style listing: location, object bytes, line number, source,
/// with errors inline and a symbol table at the end.
pub fn render(out: &AsmOutput, source: &str) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "LOC  OBJ          LINE  SOURCE");
    let _ = writeln!(s);
    let mut diags = out.diagnostics.iter().peekable();
    for (i, text) in source.lines().enumerate() {
        let line = out.lines.get(i).cloned().unwrap_or_default();
        let loc = line.addr.map_or("    ".to_string(), |a| format!("{a:04X}"));
        let mut chunks = line.bytes.chunks(BYTES_PER_ROW);
        let obj = chunks.next().map(hex_bytes).unwrap_or_default();
        let _ = writeln!(s, "{loc} {obj:<12} {:>5}  {}", i + 1, text.trim_end());
        let mut addr = line.addr.unwrap_or(0) as usize;
        for c in chunks {
            addr += BYTES_PER_ROW;
            let _ = writeln!(s, "{:04X} {}", addr & 0xFFFF, hex_bytes(c));
        }
        while let Some(d) = diags.next_if(|d| d.line == i) {
            let _ = writeln!(s, "*** ERROR at column {}: {}", d.col + 1, d.msg);
        }
    }

    let _ = writeln!(s);
    let _ = writeln!(s, "SYMBOL TABLE");
    let _ = writeln!(s, "NAME                TYPE    VALUE");
    for (name, sym) in &out.symbols {
        let _ = writeln!(
            s,
            "{name:<19} {:<7} {:04X}h",
            sym.kind.name(),
            sym.value & 0xFFFF
        );
    }
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "ASSEMBLY COMPLETE: {} ERROR(S), {} BYTE(S) OF CODE",
        out.diagnostics.len(),
        out.code_size()
    );
    s
}

fn hex_bytes(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}
