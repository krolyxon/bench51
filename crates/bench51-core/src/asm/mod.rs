//! Two-pass Intel 8051 assembler.
//!
//! Pass one parses every line, picks the opcode for each instruction (which
//! fixes its size) and assigns addresses to labels. Pass two evaluates operand
//! expressions with the complete symbol table and emits bytes.

mod expr;
pub mod hex;
mod lexer;
mod listing;

use std::collections::{BTreeMap, HashMap};

use crate::isa::{self, OpInfo, Operand};
use crate::sfr;
use expr::{Env, EvalError, Expr, Parser};
use lexer::{Tok, Token, lex};

#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// 0-based source line.
    pub line: usize,
    /// 0-based column.
    pub col: usize,
    pub msg: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymKind {
    Code,
    Number,
    Bit,
    Data,
    Idata,
    Xdata,
}

impl SymKind {
    pub fn name(self) -> &'static str {
        match self {
            SymKind::Code => "CODE",
            SymKind::Number => "NUMBER",
            SymKind::Bit => "BIT",
            SymKind::Data => "DATA",
            SymKind::Idata => "IDATA",
            SymKind::Xdata => "XDATA",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub value: i64,
    pub kind: SymKind,
    pub line: usize,
}

/// What one source line produced.
#[derive(Debug, Clone, Default)]
pub struct LineOut {
    /// Location counter at the start of the line, if the line occupies or
    /// labels an address.
    pub addr: Option<u16>,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct AsmOutput {
    pub lines: Vec<LineOut>,
    pub symbols: BTreeMap<String, Symbol>,
    pub diagnostics: Vec<Diagnostic>,
}

impl AsmOutput {
    pub fn ok(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// All emitted code bytes as (address, byte), in source order.
    pub fn bytes(&self) -> impl Iterator<Item = (u16, u8)> + '_ {
        self.lines.iter().flat_map(|l| {
            let base = l.addr.unwrap_or(0);
            l.bytes
                .iter()
                .enumerate()
                .map(move |(i, &b)| (base.wrapping_add(i as u16), b))
        })
    }

    /// Source line holding the instruction that starts at each address.
    pub fn addr_to_line(&self) -> HashMap<u16, usize> {
        let mut m = HashMap::new();
        for (i, l) in self.lines.iter().enumerate() {
            if let (Some(a), false) = (l.addr, l.bytes.is_empty()) {
                m.entry(a).or_insert(i);
            }
        }
        m
    }

    pub fn code_size(&self) -> usize {
        self.lines.iter().map(|l| l.bytes.len()).sum()
    }

    pub fn listing(&self, source: &str) -> String {
        listing::render(self, source)
    }

    pub fn intel_hex(&self) -> String {
        hex::write(self.bytes())
    }
}

/// Operand as written in the source, before matching it against the ISA.
#[derive(Debug, Clone)]
enum Syn {
    A,
    AB,
    C,
    Dptr,
    Reg(u8),
    AtReg(u8),
    AtDptr,
    AtAPlusDptr,
    AtAPlusPc,
    Imm(Expr),
    NotBit(Expr),
    Expr(Expr),
}

#[derive(Debug, Clone)]
enum DbItem {
    Bytes(Vec<u8>),
    Expr(Expr, usize),
}

#[derive(Debug, Clone)]
enum Stmt {
    Empty,
    Instr {
        info: &'static OpInfo,
        args: Vec<(Expr, usize)>,
    },
    Org(Expr, usize),
    Db(Vec<DbItem>),
    Dw(Vec<(Expr, usize)>),
    Ds(Expr, usize),
    Equ {
        name: String,
        kind: SymKind,
        expr: Expr,
        col: usize,
        redefinable: bool,
    },
    End,
}

impl Stmt {
    fn size(&self) -> u16 {
        match self {
            Stmt::Instr { info, .. } => info.size as u16,
            Stmt::Db(items) => items
                .iter()
                .map(|i| match i {
                    DbItem::Bytes(b) => b.len() as u16,
                    DbItem::Expr(..) => 1,
                })
                .sum(),
            Stmt::Dw(items) => 2 * items.len() as u16,
            _ => 0,
        }
    }
}

struct ParsedLine {
    label: Option<(String, usize)>,
    stmt: Stmt,
}

type PResult<T> = Result<T, (usize, String)>;

fn split_commas(toks: &[Token]) -> Vec<&[Token]> {
    if toks.is_empty() {
        return Vec::new();
    }
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, t) in toks.iter().enumerate() {
        match t.tok {
            Tok::Punct('(') => depth += 1,
            Tok::Punct(')') => depth -= 1,
            Tok::Punct(',') if depth == 0 => {
                parts.push(&toks[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&toks[start..]);
    parts
}

fn parse_expr(toks: &[Token], fallback_col: usize) -> PResult<Expr> {
    if toks.is_empty() {
        return Err((fallback_col, "missing operand".into()));
    }
    Parser::parse_all(toks)
}

fn reg_num(s: &str) -> Option<u8> {
    let n = s.strip_prefix('R')?;
    if n.len() == 1 {
        n.parse().ok().filter(|&n: &u8| n < 8)
    } else {
        None
    }
}

fn parse_operand(toks: &[Token], col: usize) -> PResult<Syn> {
    let Some(first) = toks.first() else {
        return Err((col, "missing operand".into()));
    };
    let words: Vec<Option<&str>> = toks.iter().map(|t| t.ident()).collect();
    if toks.len() == 1
        && let Some(w) = first.ident()
    {
        match w {
            "A" => return Ok(Syn::A),
            "AB" => return Ok(Syn::AB),
            "C" => return Ok(Syn::C),
            "DPTR" => return Ok(Syn::Dptr),
            _ => {}
        }
        if let Some(n) = reg_num(w) {
            return Ok(Syn::Reg(n));
        }
    }
    match &first.tok {
        Tok::Punct('@') => match (&words[1..], toks.get(2)) {
            ([Some("R0")], _) => Ok(Syn::AtReg(0)),
            ([Some("R1")], _) => Ok(Syn::AtReg(1)),
            ([Some("DPTR")], _) => Ok(Syn::AtDptr),
            ([Some("A"), None, Some("DPTR")], Some(p)) if p.is_punct('+') => Ok(Syn::AtAPlusDptr),
            ([Some("A"), None, Some("PC")], Some(p)) if p.is_punct('+') => Ok(Syn::AtAPlusPc),
            _ => Err((
                first.col,
                "invalid indirect operand (expected @R0, @R1, @DPTR, @A+DPTR or @A+PC)".into(),
            )),
        },
        Tok::Punct('#') => Ok(Syn::Imm(parse_expr(&toks[1..], first.col + 1)?)),
        Tok::Punct('/') => Ok(Syn::NotBit(parse_expr(&toks[1..], first.col + 1)?)),
        _ => Ok(Syn::Expr(parse_expr(toks, col)?)),
    }
}

fn operand_matches(op: Operand, syn: &Syn) -> bool {
    match (op, syn) {
        (Operand::A, Syn::A)
        | (Operand::AB, Syn::AB)
        | (Operand::C, Syn::C)
        | (Operand::Dptr, Syn::Dptr)
        | (Operand::AtDptr, Syn::AtDptr)
        | (Operand::AtAPlusDptr, Syn::AtAPlusDptr)
        | (Operand::AtAPlusPc, Syn::AtAPlusPc) => true,
        (Operand::Reg(a), Syn::Reg(b)) | (Operand::AtReg(a), Syn::AtReg(b)) => a == *b,
        (Operand::Imm8 | Operand::Imm16, Syn::Imm(_)) => true,
        (Operand::NotBit, Syn::NotBit(_)) => true,
        (
            Operand::Direct | Operand::Bit | Operand::Addr11 | Operand::Addr16 | Operand::Rel,
            Syn::Expr(_),
        ) => true,
        _ => false,
    }
}

fn parse_instruction(mnemonic: &str, col: usize, rest: &[Token]) -> PResult<Stmt> {
    let mut syns = Vec::new();
    let mut cols = Vec::new();
    for part in split_commas(rest) {
        let c = part.first().map_or(col, |t| t.col);
        syns.push(parse_operand(part, c)?);
        cols.push(c);
    }
    // Generic JMP/CALL always assemble to the long forms, so sizes never
    // depend on label values.
    let lookup = match (mnemonic, syns.as_slice()) {
        ("JMP", [Syn::Expr(_)]) => "LJMP",
        ("CALL", _) => "LCALL",
        (m, _) => m,
    };
    let info = isa::by_mnemonic(lookup)
        .find(|i| {
            i.operands.len() == syns.len()
                && i.operands
                    .iter()
                    .zip(&syns)
                    .all(|(&o, s)| operand_matches(o, s))
        })
        .ok_or_else(|| (col, format!("invalid operands for {mnemonic}")))?;
    let args = syns
        .into_iter()
        .zip(cols)
        .filter_map(|(s, c)| match s {
            Syn::Imm(e) | Syn::NotBit(e) | Syn::Expr(e) => Some((e, c)),
            _ => None,
        })
        .collect();
    Ok(Stmt::Instr { info, args })
}

fn parse_line(toks: &[Token]) -> PResult<ParsedLine> {
    let mut toks = toks;
    let mut label = None;

    // `name EQU expr` and friends.
    if let (Some(name), Some(kw)) = (
        toks.first().and_then(Token::ident),
        toks.get(1).and_then(Token::ident),
    ) {
        let kind = match kw {
            "EQU" | "SET" => Some(SymKind::Number),
            "BIT" => Some(SymKind::Bit),
            "DATA" => Some(SymKind::Data),
            "IDATA" => Some(SymKind::Idata),
            "XDATA" => Some(SymKind::Xdata),
            "CODE" => Some(SymKind::Code),
            _ => None,
        };
        if let Some(kind) = kind {
            let expr = parse_expr(&toks[2..], toks[1].col + kw.len())?;
            return Ok(ParsedLine {
                label: None,
                stmt: Stmt::Equ {
                    name: name.to_string(),
                    kind,
                    expr,
                    col: toks[0].col,
                    redefinable: kw == "SET",
                },
            });
        }
    }

    if let (Some(name), Some(colon)) = (toks.first().and_then(Token::ident), toks.get(1))
        && colon.is_punct(':')
    {
        label = Some((name.to_string(), toks[0].col));
        toks = &toks[2..];
    }

    // Allow `.org`, `.db`, ... spellings.
    if toks.first().is_some_and(|t| t.is_punct('.')) {
        toks = &toks[1..];
    }

    let Some(first) = toks.first() else {
        return Ok(ParsedLine {
            label,
            stmt: Stmt::Empty,
        });
    };
    let Some(word) = first.ident() else {
        return Err((first.col, "expected instruction or directive".into()));
    };
    let rest = &toks[1..];
    let end_col = first.col + word.len();

    let stmt = match word {
        "ORG" => Stmt::Org(parse_expr(rest, end_col)?, first.col),
        "CSEG" | "XSEG" | "DSEG" if rest.first().and_then(Token::ident) == Some("AT") => {
            Stmt::Org(parse_expr(&rest[1..], end_col)?, first.col)
        }
        "DB" | "BYTE" => {
            let mut items = Vec::new();
            for part in split_commas(rest) {
                match part {
                    [
                        Token {
                            tok: Tok::Str(b), ..
                        },
                    ] if b.len() != 1 => items.push(DbItem::Bytes(b.clone())),
                    _ => {
                        let c = part.first().map_or(end_col, |t| t.col);
                        items.push(DbItem::Expr(parse_expr(part, c)?, c));
                    }
                }
            }
            if items.is_empty() {
                return Err((end_col, "DB needs at least one value".into()));
            }
            Stmt::Db(items)
        }
        "DW" | "WORD" => {
            let mut items = Vec::new();
            for part in split_commas(rest) {
                let c = part.first().map_or(end_col, |t| t.col);
                items.push((parse_expr(part, c)?, c));
            }
            if items.is_empty() {
                return Err((end_col, "DW needs at least one value".into()));
            }
            Stmt::Dw(items)
        }
        "DS" => Stmt::Ds(parse_expr(rest, end_col)?, first.col),
        "END" => Stmt::End,
        // Accepted for compatibility, no effect in a single-module assembler.
        "CSEG" | "USING" | "NAME" => Stmt::Empty,
        m if isa::is_mnemonic(m) => parse_instruction(m, first.col, rest)?,
        _ if rest.first().is_some_and(|t| t.is_punct(':')) => {
            return Err((
                first.col,
                "a label must be the first thing on a line".into(),
            ));
        }
        _ => {
            return Err((
                first.col,
                format!("unknown instruction or directive '{word}'"),
            ));
        }
    };
    Ok(ParsedLine { label, stmt })
}

struct Symbols {
    user: HashMap<String, Symbol>,
    here: i64,
}

impl Env for Symbols {
    fn lookup(&self, name: &str) -> Option<i64> {
        if let Some(s) = self.user.get(name) {
            return Some(s.value);
        }
        sfr::SFRS
            .iter()
            .chain(sfr::BITS)
            .find(|(n, _)| *n == name)
            .map(|&(_, v)| v as i64)
    }

    fn here(&self) -> i64 {
        self.here
    }
}

fn is_predefined(name: &str) -> bool {
    sfr::SFRS.iter().chain(sfr::BITS).any(|(n, _)| *n == name)
}

fn is_reserved(name: &str) -> bool {
    matches!(name, "A" | "AB" | "C" | "DPTR" | "PC")
        || reg_num(name).is_some()
        || isa::is_mnemonic(name)
}

pub fn assemble(source: &str) -> AsmOutput {
    let mut diags: Vec<Diagnostic> = Vec::new();
    let mut syms = Symbols {
        user: HashMap::new(),
        here: 0,
    };
    let mut parsed: Vec<Option<ParsedLine>> = Vec::new();
    let mut addrs: Vec<i64> = Vec::new();
    let mut pending_equ: Vec<usize> = Vec::new();

    let err = |diags: &mut Vec<Diagnostic>, line: usize, col: usize, msg: String| {
        diags.push(Diagnostic { line, col, msg });
    };

    // ---- Pass 1: parse, size, assign label addresses. ----
    let mut loc: i64 = 0;
    let mut ended = false;
    for (ln, text) in source.lines().enumerate() {
        addrs.push(loc);
        if ended {
            parsed.push(None);
            continue;
        }
        let line = match lex(text).and_then(|t| parse_line(&t)) {
            Ok(l) => l,
            Err((col, msg)) => {
                err(&mut diags, ln, col, msg);
                parsed.push(None);
                continue;
            }
        };

        if let Some((name, col)) = &line.label {
            define(
                &mut syms,
                &mut diags,
                name,
                *col,
                ln,
                loc,
                SymKind::Code,
                false,
            );
        }

        syms.here = loc;
        match &line.stmt {
            Stmt::Org(e, col) | Stmt::Ds(e, col) => match e.eval(&syms) {
                Ok(v) if matches!(line.stmt, Stmt::Org(..)) => {
                    if !(0..=0xFFFF).contains(&v) {
                        err(
                            &mut diags,
                            ln,
                            *col,
                            format!("ORG address {v} out of range"),
                        );
                    } else {
                        loc = v;
                        addrs[ln] = loc;
                    }
                }
                Ok(v) => {
                    if v < 0 {
                        err(&mut diags, ln, *col, "DS size must not be negative".into());
                    } else {
                        loc += v;
                    }
                }
                Err(EvalError::Undefined(s, c)) => err(
                    &mut diags,
                    ln,
                    c,
                    format!("'{s}' must be defined before it is used here"),
                ),
                Err(EvalError::Other(m)) => err(&mut diags, ln, *col, m),
            },
            Stmt::Equ {
                name,
                kind,
                expr,
                col,
                redefinable,
            } => match expr.eval(&syms) {
                Ok(v) => define(
                    &mut syms,
                    &mut diags,
                    name,
                    *col,
                    ln,
                    v,
                    *kind,
                    *redefinable,
                ),
                Err(EvalError::Undefined(..)) => pending_equ.push(ln),
                Err(EvalError::Other(m)) => err(&mut diags, ln, *col, m),
            },
            Stmt::End => ended = true,
            s => loc += s.size() as i64,
        }
        if loc > 0x10000 {
            err(&mut diags, ln, 0, "code exceeds 64K address space".into());
            loc &= 0xFFFF;
        }
        parsed.push(Some(line));
    }

    // Resolve EQUs that referred to symbols defined later.
    loop {
        let before = pending_equ.len();
        pending_equ.retain(|&ln| {
            let Some(ParsedLine {
                stmt:
                    Stmt::Equ {
                        name,
                        kind,
                        expr,
                        col,
                        redefinable,
                    },
                ..
            }) = &parsed[ln]
            else {
                return false;
            };
            syms.here = addrs[ln];
            match expr.eval(&syms) {
                Ok(v) => {
                    define(
                        &mut syms,
                        &mut diags,
                        name,
                        *col,
                        ln,
                        v,
                        *kind,
                        *redefinable,
                    );
                    false
                }
                Err(_) => true,
            }
        });
        if pending_equ.len() == before {
            break;
        }
    }
    for ln in pending_equ {
        if let Some(ParsedLine {
            stmt: Stmt::Equ { expr, col, .. },
            ..
        }) = &parsed[ln]
        {
            syms.here = addrs[ln];
            match expr.eval(&syms) {
                Err(EvalError::Undefined(s, c)) => {
                    err(&mut diags, ln, c, format!("undefined symbol '{s}'"))
                }
                Err(EvalError::Other(m)) => err(&mut diags, ln, *col, m),
                Ok(_) => {}
            }
        }
    }

    // ---- Pass 2: emit bytes. ----
    let mut lines = Vec::with_capacity(parsed.len());
    let mut used = vec![false; 0x10000];
    for (ln, p) in parsed.iter().enumerate() {
        let mut out = LineOut::default();
        let Some(p) = p else {
            lines.push(out);
            continue;
        };
        let pc = addrs[ln];
        syms.here = pc;
        let has_addr = p.label.is_some()
            || matches!(
                p.stmt,
                Stmt::Instr { .. } | Stmt::Db(_) | Stmt::Dw(_) | Stmt::Ds(..) | Stmt::Org(..)
            );
        if has_addr {
            out.addr = Some(pc as u16);
        }

        let eval = |e: &Expr, col: usize, diags: &mut Vec<Diagnostic>| -> Option<i64> {
            match e.eval(&syms) {
                Ok(v) => Some(v),
                Err(EvalError::Undefined(s, c)) => {
                    err(diags, ln, c, format!("undefined symbol '{s}'"));
                    None
                }
                Err(EvalError::Other(m)) => {
                    err(diags, ln, col, m);
                    None
                }
            }
        };

        match &p.stmt {
            Stmt::Instr { info, args } => {
                let mut bytes = vec![info.opcode];
                let kinds = info.operands.iter().filter(|o| o.len() > 0);
                let mut failed = false;
                for (&kind, (e, col)) in kinds.zip(args) {
                    let Some(v) = eval(e, *col, &mut diags) else {
                        failed = true;
                        continue;
                    };
                    if let Err(m) = encode_operand(kind, v, pc, info.size, &mut bytes) {
                        err(&mut diags, ln, *col, m);
                        failed = true;
                    }
                }
                if !failed {
                    if info.opcode == 0x85 {
                        // MOV dest,src is encoded with the source byte first.
                        bytes.swap(1, 2);
                    }
                    out.bytes = bytes;
                }
            }
            Stmt::Db(items) => {
                for item in items {
                    match item {
                        DbItem::Bytes(b) => out.bytes.extend_from_slice(b),
                        DbItem::Expr(e, col) => {
                            let v = eval(e, *col, &mut diags).unwrap_or(0);
                            if !(-128..=255).contains(&v) {
                                err(
                                    &mut diags,
                                    ln,
                                    *col,
                                    format!("value {v} does not fit in a byte"),
                                );
                            }
                            out.bytes.push(v as u8);
                        }
                    }
                }
            }
            Stmt::Dw(items) => {
                for (e, col) in items {
                    let v = eval(e, *col, &mut diags).unwrap_or(0);
                    if !(-32768..=0xFFFF).contains(&v) {
                        err(
                            &mut diags,
                            ln,
                            *col,
                            format!("value {v} does not fit in a word"),
                        );
                    }
                    out.bytes.push((v >> 8) as u8);
                    out.bytes.push(v as u8);
                }
            }
            _ => {}
        }

        for i in 0..out.bytes.len() {
            let a = (pc as usize + i) & 0xFFFF;
            if used[a] {
                err(
                    &mut diags,
                    ln,
                    0,
                    format!("code overlaps previously emitted code at {a:04X}h"),
                );
                break;
            }
            used[a] = true;
        }
        lines.push(out);
    }

    diags.sort_by_key(|d| (d.line, d.col));
    let symbols = syms.user.into_iter().collect();
    AsmOutput {
        lines,
        symbols,
        diagnostics: diags,
    }
}

#[allow(clippy::too_many_arguments)]
fn define(
    syms: &mut Symbols,
    diags: &mut Vec<Diagnostic>,
    name: &str,
    col: usize,
    line: usize,
    value: i64,
    kind: SymKind,
    redefinable: bool,
) {
    let msg = if is_reserved(name) {
        Some(format!("'{name}' is a reserved word"))
    } else if is_predefined(name) {
        Some(format!("'{name}' is a predefined SFR or bit name"))
    } else {
        syms.user
            .get(name)
            .filter(|_| !redefinable)
            .map(|prev| format!("'{name}' already defined on line {}", prev.line + 1))
    };
    match msg {
        Some(msg) => diags.push(Diagnostic { line, col, msg }),
        None => {
            syms.user
                .insert(name.to_string(), Symbol { value, kind, line });
        }
    }
}

fn encode_operand(
    kind: Operand,
    v: i64,
    pc: i64,
    size: u8,
    bytes: &mut Vec<u8>,
) -> Result<(), String> {
    let range = |lo: i64, hi: i64, what: &str| {
        if (lo..=hi).contains(&v) {
            Ok(())
        } else {
            Err(format!("{what} {v} out of range"))
        }
    };
    match kind {
        Operand::Imm8 => {
            range(-128, 255, "immediate value")?;
            bytes.push(v as u8);
        }
        Operand::Imm16 => {
            range(-32768, 0xFFFF, "immediate value")?;
            bytes.extend_from_slice(&(v as u16).to_be_bytes());
        }
        Operand::Direct => {
            range(0, 255, "direct address")?;
            bytes.push(v as u8);
        }
        Operand::Bit | Operand::NotBit => {
            range(0, 255, "bit address")?;
            bytes.push(v as u8);
        }
        Operand::Addr16 => {
            range(0, 0xFFFF, "address")?;
            bytes.extend_from_slice(&(v as u16).to_be_bytes());
        }
        Operand::Addr11 => {
            range(0, 0xFFFF, "address")?;
            let next = pc + size as i64;
            if (next & 0xF800) != (v & 0xF800) {
                return Err(format!("target {v:04X}h is outside the current 2K page"));
            }
            bytes[0] |= (((v >> 8) & 7) as u8) << 5;
            bytes.push(v as u8);
        }
        Operand::Rel => {
            let off = v - (pc + size as i64);
            if !(-128..=127).contains(&off) {
                return Err(format!(
                    "jump target out of range (offset {off}, must be -128..127)"
                ));
            }
            bytes.push(off as u8);
        }
        _ => unreachable!("operand {kind:?} carries no value"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(src: &str) -> Vec<u8> {
        let out = assemble(src);
        assert!(out.ok(), "{:?}", out.diagnostics);
        out.bytes().map(|(_, b)| b).collect()
    }

    fn one(line: &str) -> Vec<u8> {
        bytes(line)
    }

    #[test]
    fn encodes_operand_forms() {
        assert_eq!(one("MOV A,#55h"), [0x74, 0x55]);
        assert_eq!(one("mov a, r3"), [0xEB]);
        assert_eq!(one("MOV A,@R1"), [0xE7]);
        assert_eq!(one("MOV A,30h"), [0xE5, 0x30]);
        assert_eq!(one("MOV A,B"), [0xE5, 0xF0]);
        assert_eq!(one("MOV 30h,#1"), [0x75, 0x30, 0x01]);
        assert_eq!(one("MOV 30h,40h"), [0x85, 0x40, 0x30]);
        assert_eq!(one("MOV R7,#0"), [0x7F, 0x00]);
        assert_eq!(one("MOV @R0,A"), [0xF6]);
        assert_eq!(one("MOV DPTR,#1234h"), [0x90, 0x12, 0x34]);
        assert_eq!(one("MOV C,P1.3"), [0xA2, 0x93]);
        assert_eq!(one("MOV P1.3,C"), [0x92, 0x93]);
        assert_eq!(one("SETB TR0"), [0xD2, 0x8C]);
        assert_eq!(one("CLR C"), [0xC3]);
        assert_eq!(one("ANL C,/ACC.7"), [0xB0, 0xE7]);
        assert_eq!(one("CPL 20h.0"), [0xB2, 0x00]);
        assert_eq!(one("MOVC A,@A+DPTR"), [0x93]);
        assert_eq!(one("MOVX @DPTR,A"), [0xF0]);
        assert_eq!(one("JMP @A+DPTR"), [0x73]);
        assert_eq!(one("MUL AB"), [0xA4]);
        assert_eq!(one("PUSH ACC"), [0xC0, 0xE0]);
        assert_eq!(one("ORL 20h,#0Fh"), [0x43, 0x20, 0x0F]);
        assert_eq!(one("XCHD A,@R0"), [0xD6]);
        assert_eq!(one("MOV A,#-1"), [0x74, 0xFF]);
        assert_eq!(one("MOV A,#HIGH(1234h)"), [0x74, 0x12]);
    }

    #[test]
    fn jumps_and_labels() {
        let src = "
start:  SJMP start
        DJNZ R2, start
        CJNE A,#3,fwd
        LJMP fwd
        CALL fwd
fwd:    AJMP start
        SJMP $
";
        assert_eq!(
            bytes(src),
            [
                0x80, 0xFE, // SJMP start
                0xDA, 0xFC, // DJNZ R2,start
                0xB4, 0x03, 0x06, // CJNE -> fwd at 0x0D
                0x02, 0x00, 0x0D, // LJMP
                0x12, 0x00, 0x0D, // LCALL
                0x01, 0x00, // AJMP 0
                0x80, 0xFE,
            ]
        );
    }

    #[test]
    fn directives() {
        let src = "
COUNT   EQU 3
LED     BIT P1.0
        ORG 100h
        MOV R0,#COUNT
        SETB LED
tbl:    DB 1, 'AB', COUNT*2
        DW tbl
        DS 2
after:  NOP
        END
        garbage that is ignored
";
        let out = assemble(src);
        assert!(out.ok(), "{:?}", out.diagnostics);
        let v: Vec<_> = out.bytes().collect();
        assert_eq!(v[0], (0x100, 0x78));
        let b: Vec<u8> = v.iter().map(|x| x.1).collect();
        assert_eq!(b, [0x78, 3, 0xD2, 0x90, 1, b'A', b'B', 6, 0x01, 0x04, 0x00]);
        assert_eq!(out.symbols["AFTER"].value, 0x10C);
    }

    #[test]
    fn forward_equ() {
        let out = assemble("X EQU Y+1\nY EQU 2\nMOV A,#X");
        assert!(out.ok(), "{:?}", out.diagnostics);
        assert_eq!(out.bytes().map(|b| b.1).collect::<Vec<_>>(), [0x74, 3]);
    }

    #[test]
    fn reports_errors_with_lines() {
        let out = assemble("NOP\nMOV A,@R2\nSJMP nowhere\nFOO\nMOV A,#300\n");
        let lines: Vec<_> = out.diagnostics.iter().map(|d| d.line).collect();
        assert_eq!(lines, [1, 2, 3, 4]);
    }

    #[test]
    fn relative_range_checked() {
        let out = assemble("SJMP far\nORG 200h\nfar: NOP");
        assert_eq!(out.diagnostics.len(), 1);
        assert!(out.diagnostics[0].msg.contains("out of range"));
    }
}
