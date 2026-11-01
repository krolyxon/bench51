mod ui;

use std::path::PathBuf;
use std::process::ExitCode;

use bench51_core::asm;
use clap::Parser;

/// Intel 8051 assembler, simulator and TUI debugger.
#[derive(Parser, Debug)]
#[command(name = "bench51", version, about)]
struct Args {
    /// Assembly source (.asm) or Intel HEX (.hex) file to open.
    file: Option<PathBuf>,

    /// Assemble FILE without starting the TUI, writing the Intel HEX file.
    #[arg(short, long, requires = "file")]
    assemble: bool,

    /// Output HEX path (default: FILE with a .hex extension).
    #[arg(short, long, requires = "assemble")]
    output: Option<PathBuf>,

    /// Also write a listing to this path (default: FILE with a .lst extension).
    #[arg(long, requires = "assemble")]
    lst: Option<PathBuf>,

    /// Oscillator frequency in MHz, used to show elapsed time.
    #[arg(long, default_value_t = 11.0592)]
    clock: f64,
}

fn assemble_file(args: &Args) -> ExitCode {
    let path = args.file.as_ref().unwrap();
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let out = asm::assemble(&source);
    let lst = args
        .lst
        .clone()
        .unwrap_or_else(|| path.with_extension("lst"));
    if let Err(e) = std::fs::write(&lst, out.listing(&source)) {
        eprintln!("error: cannot write {}: {e}", lst.display());
    }
    if !out.ok() {
        for d in &out.diagnostics {
            eprintln!(
                "{}:{}:{}: error: {}",
                path.display(),
                d.line + 1,
                d.col + 1,
                d.msg
            );
        }
        eprintln!("{} error(s); see {}", out.diagnostics.len(), lst.display());
        return ExitCode::FAILURE;
    }
    let hex = args
        .output
        .clone()
        .unwrap_or_else(|| path.with_extension("hex"));
    if let Err(e) = std::fs::write(&hex, out.intel_hex()) {
        eprintln!("error: cannot write {}: {e}", hex.display());
        return ExitCode::FAILURE;
    }
    println!(
        "{} bytes -> {}, listing -> {}",
        out.code_size(),
        hex.display(),
        lst.display()
    );
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let args = Args::parse();
    if args.assemble {
        return assemble_file(&args);
    }
    match ui::run(args.file, args.clock) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
