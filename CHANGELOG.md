# Changelog

All notable changes to bench51. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/).

## [0.3.0] - 2026-11-01

### Added

- Vim keybindings in the GUI editor, on by default: Normal, Insert, Visual
  and Visual Line modes, motions with counts, operators with text objects,
  registers, undo/redo, `.` repeat, macros, search with highlighting, and
  `:w :q :wq :e :make`. A block cursor and a mode line show the state, and
  `gd` jumps to a label's definition.
- A Settings menu with Keybindings (Vim / Standard) and Theme, and a
  `--keymap vim|standard` flag.

### Changed

- Theme moved from the View menu to Settings.
- The terminal app moved from `src/` to `crates/bench51-tui` (package
  `bench51-tui`, binary still `bench51`); the repository root is now a
  plain workspace. From source: `cargo install --path crates/bench51-tui`.

### Fixed

- `--theme`, `--keymap` and `--clock` no longer overwrite the saved
  settings; they only apply to that run.

## [0.2.0] - 2026-11-01

### Added

- **bench51-gui**, a desktop IDE laid out like Keil µVision: menus, toolbar,
  Project window, editor with breakpoint and error gutter, Build Output, and
  in a debug session the Registers, Disassembly, Memory, Symbols, UART #1,
  Ports, Timers and Interrupts windows. Windows dock and tab anywhere; the
  edit and debug layouts are remembered separately.
- Monochrome, dot-matrix design in dark and light themes (follows the
  system by default), with Space Grotesk, Space Mono and Doto bundled.
- µVision's keys (F7, Ctrl-F5, F5, F10/F11, Ctrl-F10/F11, F9) with the
  TUI's as aliases; find (Ctrl-F); inline memory editing; clickable SFR
  bits and port pins; drag-and-drop to open files.
- Prebuilt releases for Linux (x86_64, aarch64), Windows (x86_64) and macOS
  (Apple silicon, Intel), with one-line installers: `install.sh` for Linux
  and macOS, `install.ps1` for Windows.
- CI on Linux, Windows and macOS.

### Changed

- The assembler, simulator and debugger session moved into a shared
  `bench51-core` crate used by both front ends. The terminal app behaves
  as before.
- Unsafe code is forbidden across the workspace.

## [0.1.0] - 2026-10-23

### Added

- `bench51`, a terminal (TUI) 8051 IDE: editor, two-pass assembler for the
  full instruction set with Keil-style listing and Intel HEX output,
  simulator with timers, interrupts, UART and ports, and a debugger with
  breakpoints, stepping and memory poke.
- Headless assembly with `bench51 --assemble`.
- Opening Intel HEX files as disassembled, re-assemblable source.

[0.3.0]: https://github.com/krolyxon/bench51/releases/tag/v0.3.0
[0.2.0]: https://github.com/krolyxon/bench51/releases/tag/v0.2.0
