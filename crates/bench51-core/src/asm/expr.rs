use super::lexer::{Tok, Token};

#[derive(Debug, Clone)]
pub enum Expr {
    Num(i64),
    Sym(String, usize),
    /// `$`, the current location counter.
    Here,
    Neg(Box<Expr>),
    Not(Box<Expr>),
    High(Box<Expr>),
    Low(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    /// `byte.bit` bit addressing, e.g. `P1.3` or `20h.0`.
    Dot,
}

pub enum EvalError {
    Undefined(String, usize),
    Other(String),
}

pub trait Env {
    fn lookup(&self, name: &str) -> Option<i64>;
    fn here(&self) -> i64;
}

impl Expr {
    pub fn eval(&self, env: &dyn Env) -> Result<i64, EvalError> {
        Ok(match self {
            Expr::Num(n) => *n,
            Expr::Here => env.here(),
            Expr::Sym(s, col) => env
                .lookup(s)
                .ok_or_else(|| EvalError::Undefined(s.clone(), *col))?,
            Expr::Neg(e) => e.eval(env)?.wrapping_neg(),
            Expr::Not(e) => !e.eval(env)? & 0xFFFF,
            Expr::High(e) => (e.eval(env)? >> 8) & 0xFF,
            Expr::Low(e) => e.eval(env)? & 0xFF,
            Expr::Bin(op, l, r) => {
                let (l, r) = (l.eval(env)?, r.eval(env)?);
                match op {
                    BinOp::Add => l.wrapping_add(r),
                    BinOp::Sub => l.wrapping_sub(r),
                    BinOp::Mul => l.wrapping_mul(r),
                    BinOp::Div | BinOp::Mod if r == 0 => {
                        return Err(EvalError::Other("division by zero".into()));
                    }
                    BinOp::Div => l / r,
                    BinOp::Mod => l % r,
                    BinOp::And => l & r,
                    BinOp::Or => l | r,
                    BinOp::Xor => l ^ r,
                    BinOp::Shl => l.wrapping_shl(r as u32),
                    BinOp::Shr => l.wrapping_shr(r as u32),
                    BinOp::Dot => bit_address(l, r)?,
                }
            }
        })
    }
}

fn bit_address(byte: i64, bit: i64) -> Result<i64, EvalError> {
    if !(0..8).contains(&bit) {
        return Err(EvalError::Other(format!(
            "bit index {bit} out of range 0..7"
        )));
    }
    match byte {
        0x20..=0x2F => Ok((byte - 0x20) * 8 + bit),
        0x80..=0xFF if byte % 8 == 0 => Ok(byte + bit),
        _ => Err(EvalError::Other(format!(
            "address {byte:02X}h is not bit-addressable"
        ))),
    }
}

/// Recursive-descent parser over a token slice.
pub struct Parser<'a> {
    toks: &'a [Token],
    pos: usize,
}

type PResult<T> = Result<T, (usize, String)>;

impl<'a> Parser<'a> {
    pub fn new(toks: &'a [Token]) -> Self {
        Self { toks, pos: 0 }
    }

    /// Parses the whole slice as one expression.
    pub fn parse_all(toks: &'a [Token]) -> PResult<Expr> {
        let mut p = Self::new(toks);
        let e = p.expr()?;
        if let Some(t) = p.toks.get(p.pos) {
            return Err((t.col, "unexpected token in expression".into()));
        }
        Ok(e)
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.tok)
    }

    fn peek_word(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(s)) if s == w)
    }

    fn col(&self) -> usize {
        self.toks
            .get(self.pos)
            .or(self.toks.last())
            .map_or(0, |t| t.col)
    }

    fn expr(&mut self) -> PResult<Expr> {
        let mut l = self.xor()?;
        while self.peek_word("OR") || self.peek() == Some(&Tok::Punct('|')) {
            self.pos += 1;
            l = Expr::Bin(BinOp::Or, Box::new(l), Box::new(self.xor()?));
        }
        Ok(l)
    }

    fn xor(&mut self) -> PResult<Expr> {
        let mut l = self.and()?;
        while self.peek_word("XOR") || self.peek() == Some(&Tok::Punct('^')) {
            self.pos += 1;
            l = Expr::Bin(BinOp::Xor, Box::new(l), Box::new(self.and()?));
        }
        Ok(l)
    }

    fn and(&mut self) -> PResult<Expr> {
        let mut l = self.shift()?;
        while self.peek_word("AND") || self.peek() == Some(&Tok::Punct('&')) {
            self.pos += 1;
            l = Expr::Bin(BinOp::And, Box::new(l), Box::new(self.shift()?));
        }
        Ok(l)
    }

    fn shift(&mut self) -> PResult<Expr> {
        let mut l = self.add()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Shl) => BinOp::Shl,
                Some(Tok::Shr) => BinOp::Shr,
                Some(Tok::Ident(s)) if s == "SHL" => BinOp::Shl,
                Some(Tok::Ident(s)) if s == "SHR" => BinOp::Shr,
                _ => return Ok(l),
            };
            self.pos += 1;
            l = Expr::Bin(op, Box::new(l), Box::new(self.add()?));
        }
    }

    fn add(&mut self) -> PResult<Expr> {
        let mut l = self.mul()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Punct('+')) => BinOp::Add,
                Some(Tok::Punct('-')) => BinOp::Sub,
                _ => return Ok(l),
            };
            self.pos += 1;
            l = Expr::Bin(op, Box::new(l), Box::new(self.mul()?));
        }
    }

    fn mul(&mut self) -> PResult<Expr> {
        let mut l = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Punct('*')) => BinOp::Mul,
                Some(Tok::Punct('/')) => BinOp::Div,
                Some(Tok::Punct('%')) => BinOp::Mod,
                Some(Tok::Ident(s)) if s == "MOD" => BinOp::Mod,
                _ => return Ok(l),
            };
            self.pos += 1;
            l = Expr::Bin(op, Box::new(l), Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> PResult<Expr> {
        let wrap: fn(Box<Expr>) -> Expr = match self.peek() {
            Some(Tok::Punct('-')) => Expr::Neg,
            Some(Tok::Punct('+')) => {
                self.pos += 1;
                return self.unary();
            }
            Some(Tok::Punct('~')) => Expr::Not,
            Some(Tok::Ident(s)) if s == "NOT" => Expr::Not,
            Some(Tok::Ident(s)) if s == "HIGH" => Expr::High,
            Some(Tok::Ident(s)) if s == "LOW" => Expr::Low,
            _ => return self.dot(),
        };
        self.pos += 1;
        Ok(wrap(Box::new(self.unary()?)))
    }

    fn dot(&mut self) -> PResult<Expr> {
        let l = self.primary()?;
        if self.peek() == Some(&Tok::Punct('.')) {
            self.pos += 1;
            let r = self.primary()?;
            return Ok(Expr::Bin(BinOp::Dot, Box::new(l), Box::new(r)));
        }
        Ok(l)
    }

    fn primary(&mut self) -> PResult<Expr> {
        let col = self.col();
        let Some(tok) = self.peek().cloned() else {
            return Err((col, "expected expression".into()));
        };
        self.pos += 1;
        match tok {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Punct('$') => Ok(Expr::Here),
            Tok::Ident(s) => Ok(Expr::Sym(s, col)),
            Tok::Str(b) if b.len() == 1 => Ok(Expr::Num(b[0] as i64)),
            Tok::Str(b) if b.len() == 2 => Ok(Expr::Num(((b[0] as i64) << 8) | b[1] as i64)),
            Tok::Str(_) => Err((col, "string too long for an expression".into())),
            Tok::Punct('(') => {
                let e = self.expr()?;
                if self.peek() != Some(&Tok::Punct(')')) {
                    return Err((self.col(), "expected ')'".into()));
                }
                self.pos += 1;
                Ok(e)
            }
            _ => Err((col, "expected expression".into())),
        }
    }
}
