//! A debugging session: the simulator, the program loaded into it,
//! breakpoints and run state. Front ends own the source text and decide
//! when to build; everything else about running a program lives here.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::asm::{self, AsmOutput, Diagnostic};
use crate::sim::{Sim, Stop};

/// Result of the last successful assembly, which the simulator runs.
pub struct Build {
    pub source: Vec<String>,
    pub out: AsmOutput,
    pub addr_to_line: HashMap<u16, usize>,
}

impl Build {
    /// First address emitted by `line`, if any.
    pub fn line_addr(&self, line: usize) -> Option<u16> {
        self.out
            .lines
            .get(line)
            .filter(|l| !l.bytes.is_empty())
            .and_then(|l| l.addr)
    }
}

/// Instructions executed per tick (one tick ≈ 33 ms) for each speed setting.
pub const SPEEDS: [u32; 8] = [1, 5, 25, 100, 500, 2_000, 10_000, 40_000];

/// Instruction budget for step over / step out / run to cursor.
const STEP_BUDGET: u32 = 5_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemSpace {
    Iram,
    Sfr,
    Xram,
    Code,
}

impl MemSpace {
    pub const ALL: [MemSpace; 4] = [
        MemSpace::Iram,
        MemSpace::Sfr,
        MemSpace::Xram,
        MemSpace::Code,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MemSpace::Iram => "IRAM",
            MemSpace::Sfr => "SFR",
            MemSpace::Xram => "XRAM",
            MemSpace::Code => "CODE",
        }
    }
}

pub struct Session {
    pub sim: Sim,
    /// Last assembler result, even if it had errors (for diagnostics).
    pub last_asm: Option<(String, AsmOutput)>,
    /// Last error-free build, loaded into the simulator.
    pub build: Option<Build>,
    /// Breakpoints by source line.
    pub breakpoints: HashSet<usize>,
    pub running: bool,
    /// The next run chunk starts a fresh run and may leave a breakpoint.
    resume: bool,
    /// Index into `SPEEDS`.
    pub speed: usize,
    pub clock_mhz: f64,
    /// Memory before the last step or run chunk, to highlight changes.
    pub prev_iram: [u8; 256],
    pub prev_sfr: [u8; 128],
    /// Outcome of the last command, for the status line.
    pub status: String,
}

impl Session {
    pub fn new(clock_mhz: f64) -> Self {
        Self {
            sim: Sim::new(),
            last_asm: None,
            build: None,
            breakpoints: HashSet::new(),
            running: false,
            resume: false,
            speed: 5,
            clock_mhz,
            prev_iram: [0; 256],
            prev_sfr: [0; 128],
            status: String::new(),
        }
    }

    /// Forgets the program and breakpoints, for a newly loaded source.
    pub fn clear(&mut self) {
        self.breakpoints.clear();
        self.build = None;
        self.last_asm = None;
        self.running = false;
    }

    /// Whether `lines` differ from the source the simulator is running.
    pub fn is_stale<'a>(&self, lines: impl IntoIterator<Item = &'a str>) -> bool {
        self.build
            .as_ref()
            .is_none_or(|b| !b.source.iter().map(String::as_str).eq(lines))
    }

    /// Diagnostics of the last assembly, if it was of exactly `source`.
    pub fn diagnostics_for(&self, source: &str) -> &[Diagnostic] {
        match &self.last_asm {
            Some((src, out)) if src == source => &out.diagnostics,
            _ => &[],
        }
    }

    // ---- Build ----

    /// Assembles `source`. On success the program is loaded into a freshly
    /// reset simulator and, if `path` (the saved source file) is given,
    /// `.lst` and `.hex` files are written next to it.
    pub fn assemble(&mut self, source: &str, path: Option<&Path>) -> bool {
        let out = asm::assemble(source);
        let ok = out.ok();
        if ok {
            self.sim.load_code(out.bytes());
            self.sim.reset();
            self.running = false;
            self.status = format!("assembled: {} bytes, 0 errors", out.code_size());
            if let Some(path) = path {
                self.write_outputs(&out, source, path);
            }
            self.build = Some(Build {
                source: source.split('\n').map(String::from).collect(),
                addr_to_line: out.addr_to_line(),
                out: out.clone(),
            });
            self.snapshot();
        } else {
            let first = &out.diagnostics[0];
            self.status = format!(
                "{} error(s); line {}: {}",
                out.diagnostics.len(),
                first.line + 1,
                first.msg
            );
        }
        self.last_asm = Some((source.to_string(), out));
        ok
    }

    /// Makes sure a build of `source` is loaded; returns false on errors.
    pub fn ensure_built(&mut self, source: &str, path: Option<&Path>) -> bool {
        !self.is_stale(source.split('\n')) || self.assemble(source, path)
    }

    fn write_outputs(&mut self, out: &AsmOutput, source: &str, path: &Path) {
        let res = std::fs::write(path.with_extension("lst"), out.listing(source))
            .and_then(|_| std::fs::write(path.with_extension("hex"), out.intel_hex()));
        if let Err(e) = res {
            self.status = format!("assembled, but writing .lst/.hex failed: {e}");
        }
    }

    // ---- Breakpoints ----

    pub fn toggle_breakpoint(&mut self, line: usize) {
        if !self.breakpoints.remove(&line) {
            self.breakpoints.insert(line);
        }
    }

    pub fn bp_addrs(&self) -> HashSet<u16> {
        let Some(b) = &self.build else {
            return HashSet::new();
        };
        self.breakpoints
            .iter()
            .filter_map(|&l| b.line_addr(l))
            .collect()
    }

    // ---- Execution ----

    pub fn snapshot(&mut self) {
        self.prev_iram = self.sim.iram;
        self.prev_sfr = self.sim.sfr;
    }

    /// Source line of the instruction at the PC.
    pub fn pc_line(&self) -> Option<usize> {
        self.build.as_ref()?.addr_to_line.get(&self.sim.pc).copied()
    }

    /// Stops running and describes why in `status`.
    pub fn report(&mut self, stop: Stop) {
        self.running = false;
        self.status = match stop {
            Stop::Breakpoint(pc) => format!("breakpoint at {pc:04X}h"),
            Stop::ReservedOpcode { pc, opcode } => {
                format!("reserved opcode {opcode:02X}h at {pc:04X}h — stopped")
            }
            Stop::PowerDown => "CPU in power-down mode (PCON.PD) — reset to continue".into(),
            Stop::Target | Stop::Budget => format!("stopped at {:04X}h", self.sim.pc),
        };
    }

    fn step_with(&mut self, f: impl FnOnce(&mut Sim, &HashSet<u16>) -> Stop) {
        self.running = false;
        self.snapshot();
        let bps = self.bp_addrs();
        match f(&mut self.sim, &bps) {
            Stop::Target => self.status = format!("PC = {:04X}h", self.sim.pc),
            s => self.report(s),
        }
    }

    pub fn step_into(&mut self) {
        self.step_with(|s, _| match s.step() {
            Ok(()) => Stop::Target,
            Err(e) => e,
        });
    }

    pub fn step_over(&mut self) {
        self.step_with(|s, b| s.step_over(STEP_BUDGET, b));
    }

    pub fn step_out(&mut self) {
        self.step_with(|s, b| s.step_out(STEP_BUDGET, b));
    }

    /// Runs until the first instruction of `line`, or a breakpoint.
    /// Returns false if the line holds no code.
    pub fn run_to_line(&mut self, line: usize) -> bool {
        let Some(target) = self.build.as_ref().and_then(|b| b.line_addr(line)) else {
            self.status = "no code on that line".into();
            return false;
        };
        self.step_with(|s, bps| {
            let mut bps = bps.clone();
            bps.insert(target);
            match s.run(STEP_BUDGET, &bps, true) {
                Stop::Breakpoint(pc) if pc == target => Stop::Target,
                other => other,
            }
        });
        true
    }

    /// Starts free running; `tick` then advances the program. Needs a build.
    pub fn start(&mut self) {
        if self.build.is_none() {
            return;
        }
        self.running = true;
        self.resume = true;
        self.status = "running…".into();
    }

    pub fn pause(&mut self) {
        self.running = false;
        self.status = format!("paused at {:04X}h", self.sim.pc);
    }

    /// Runs one chunk of `SPEEDS[speed]` instructions if running. Front ends
    /// call this every ~33 ms.
    pub fn tick(&mut self) {
        if !self.running {
            return;
        }
        self.snapshot();
        let bps = self.bp_addrs();
        let resume = std::mem::take(&mut self.resume);
        match self.sim.run(SPEEDS[self.speed], &bps, resume) {
            Stop::Budget => {}
            s => self.report(s),
        }
    }

    pub fn reset(&mut self) {
        self.sim.reset();
        self.running = false;
        self.snapshot();
        self.status = "reset".into();
    }

    /// Writes `value` at `addr` in `space`. Returns false if out of range.
    pub fn poke(&mut self, space: MemSpace, addr: u32, value: u8) -> bool {
        let ok = match space {
            MemSpace::Iram if addr <= 0xFF => {
                self.sim.iram[addr as usize] = value;
                true
            }
            MemSpace::Sfr if (0x80..=0xFF).contains(&addr) => {
                self.sim.set_sfr(addr as u8, value);
                true
            }
            MemSpace::Xram if addr <= 0xFFFF => {
                self.sim.xram[addr as usize] = value;
                true
            }
            MemSpace::Code if addr <= 0xFFFF => {
                self.sim.code[addr as usize] = value;
                true
            }
            _ => false,
        };
        self.status = if ok {
            format!("wrote {value:02X}h at {addr:X}h")
        } else {
            "address out of range".into()
        };
        ok
    }

    /// Simulated time since reset at `clock_mhz` (12 clocks per machine cycle).
    pub fn elapsed(&self) -> String {
        let micros = self.sim.cycles as f64 * 12.0 / self.clock_mhz;
        if micros >= 1_000_000.0 {
            format!("{:.3} s", micros / 1e6)
        } else if micros >= 1000.0 {
            format!("{:.3} ms", micros / 1e3)
        } else {
            format!("{micros:.2} µs")
        }
    }
}

/// Name of the interrupt source served at vector `v`.
pub fn vector_name(v: u16) -> &'static str {
    match v {
        0x03 => "INT0",
        0x0B => "T0",
        0x13 => "INT1",
        0x1B => "T1",
        0x23 => "UART",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::{MemSpace, Session};

    const PROG: &str = "\
        ORG 0
        LJMP main
        ORG 30h
main:   MOV R0,#0
loop:   ACALL bump
        SJMP loop
bump:   INC R0
        NOP
        RET
        END";

    fn session() -> Session {
        let mut s = Session::new(12.0);
        assert!(s.assemble(PROG, None), "{}", s.status);
        s
    }

    #[test]
    fn breakpoints_steps_and_run_to_line() {
        let mut s = session();
        s.toggle_breakpoint(6); // bump: INC R0
        s.start();
        s.tick();
        assert!(!s.running);
        assert_eq!(s.pc_line(), Some(6));
        assert_eq!(s.status, format!("breakpoint at {:04X}h", s.sim.pc));

        s.step_into(); // INC R0
        assert_eq!(s.sim.reg(0), 1);
        assert_eq!(s.pc_line(), Some(7));
        s.step_out(); // back after ACALL
        assert_eq!(s.pc_line(), Some(5));

        s.toggle_breakpoint(6);
        s.step_into(); // SJMP loop
        assert_eq!(s.pc_line(), Some(4));
        s.step_over(); // whole call
        assert_eq!((s.pc_line(), s.sim.reg(0)), (Some(5), 2));

        assert!(s.run_to_line(7));
        assert_eq!((s.pc_line(), s.sim.reg(0)), (Some(7), 3));
        assert!(!s.run_to_line(0)); // ORG: no code
    }

    #[test]
    fn staleness_errors_and_poke() {
        let mut s = session();
        assert!(!s.is_stale(PROG.split('\n')));
        let edited = PROG.replace("NOP", "NOP\n        NOP");
        assert!(s.is_stale(edited.split('\n')));
        assert!(s.ensure_built(&edited, None));
        assert!(!s.is_stale(edited.split('\n')));

        let bad = "  MOV A,\n  FOO\n";
        assert!(!s.assemble(bad, None));
        assert_eq!(s.diagnostics_for(bad).len(), 2);
        assert!(s.diagnostics_for(PROG).is_empty());
        assert!(s.build.is_some(), "a failed build keeps the last good one");

        assert!(s.poke(MemSpace::Iram, 0x30, 0xAB));
        assert_eq!(s.sim.iram[0x30], 0xAB);
        assert!(s.poke(MemSpace::Sfr, 0x90, 0x0F));
        assert!(!s.poke(MemSpace::Sfr, 0x20, 1));
        assert_eq!(s.status, "address out of range");
    }
}
