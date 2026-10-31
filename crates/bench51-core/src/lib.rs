//! Intel 8051 assembler, simulator and debugger session shared by the
//! bench51 front ends (the `bench51` TUI and the `bench51-gui` desktop app).

pub mod asm;
pub mod files;
pub mod isa;
pub mod session;
pub mod sfr;
pub mod sim;
pub mod syntax;
