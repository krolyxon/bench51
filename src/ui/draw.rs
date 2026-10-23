use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Tabs, Wrap};
use ratatui_textarea::TextArea;

use super::app::{App, Focus, MemTab, Mode, OutTab, PromptKind, SPEEDS};
use super::syntax::highlight;
use crate::sfr::{self, IE, IP, PCON, PSW, SCON, TCON, TH0, TH1, TL0, TL1, TMOD};

const ACCENT: Color = Color::Cyan;
const CHANGED: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
const DIM: Style = Style::new().fg(Color::DarkGray);
const LABEL: Style = Style::new().fg(Color::Gray);
const ON: Style = Style::new()
    .fg(Color::LightGreen)
    .add_modifier(Modifier::BOLD);
const PC_BG: Color = Color::Indexed(58);
const CURSOR_BG: Color = Color::Indexed(236);

pub fn style_editor(ed: &mut TextArea<'static>) {
    ed.set_line_number_style(DIM);
    ed.set_cursor_line_style(Style::new().bg(CURSOR_BG));
    ed.set_tab_length(8);
}

fn panel(title: &str, focused: bool) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if focused {
            Style::new().fg(ACCENT)
        } else {
            DIM
        })
        .title(Span::styled(
            format!(" {title} "),
            if focused {
                Style::new().fg(ACCENT).bold()
            } else {
                LABEL
            },
        ))
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .areas(f.area());
    draw_header(f, app, header);
    match app.mode {
        Mode::Edit => draw_edit(f, app, body),
        Mode::Debug => draw_debug(f, app, body),
    }
    draw_footer(f, app, footer);
    if app.show_help {
        draw_help(f);
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let (mode, mode_style) = match app.mode {
        Mode::Edit => (
            " EDIT ",
            Style::new().bg(Color::Blue).fg(Color::White).bold(),
        ),
        Mode::Debug => (
            " DEBUG ",
            Style::new().bg(Color::Magenta).fg(Color::White).bold(),
        ),
    };
    let (state, state_style) = if app.running {
        (
            " RUNNING ",
            Style::new().bg(Color::Green).fg(Color::Black).bold(),
        )
    } else if app.build.is_none() {
        (
            " NOT BUILT ",
            Style::new().bg(Color::DarkGray).fg(Color::White),
        )
    } else {
        (
            " STOPPED ",
            Style::new().bg(Color::Yellow).fg(Color::Black).bold(),
        )
    };
    let mut spans = vec![
        Span::styled(" bench51 ", Style::new().bg(ACCENT).fg(Color::Black).bold()),
        Span::raw(" "),
        Span::styled(mode, mode_style),
        Span::styled(state, state_style),
        Span::raw(format!("  {}", app.title())),
    ];
    if app.build.is_some() && app.stale() {
        spans.push(Span::styled(
            "  (source changed — F7 to rebuild)",
            Style::new().fg(Color::Yellow),
        ));
    }
    spans.push(Span::styled(
        format!("   speed {}/frame (+/-)", SPEEDS[app.speed]),
        DIM,
    ));
    f.render_widget(Line::from(spans), area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    if let Some(p) = &app.prompt {
        let label = match p.kind {
            PromptKind::Open => "Open file (.asm or .hex): ",
            PromptKind::SaveAs => "Save as: ",
            PromptKind::Poke => match app.mem_tab {
                MemTab::Iram => "Write IRAM — <addr> <value> (hex): ",
                MemTab::Sfr => "Write SFR — <addr> <value> (hex): ",
                MemTab::Xram => "Write XRAM — <addr> <value> (hex): ",
                MemTab::Code => "Write CODE — <addr> <value> (hex): ",
            },
        };
        let line = Line::from(vec![
            Span::styled(label, Style::new().fg(ACCENT).bold()),
            Span::raw(p.input.clone()),
            Span::styled("█", Style::new().fg(ACCENT)),
            Span::styled("   Enter confirm · Esc cancel", DIM),
        ]);
        f.render_widget(line, area);
        return;
    }
    let keys = match app.mode {
        Mode::Edit => "F7 build  F5 run  F11 step  F9 bp  F4 debug  ^G $EDITOR  F1 help",
        Mode::Debug => {
            "F5 run/pause  F11 step  F10 over  F12 out  F8 to cursor  F9 bp  F6 reset  Tab focus  F1 help"
        }
    };
    let width = area.width as usize;
    let status = format!(" {}", app.status);
    let pad = width.saturating_sub(status.chars().count() + keys.len() + 1);
    let line = Line::from(vec![
        Span::styled(status, Style::new().fg(Color::White)),
        Span::raw(" ".repeat(pad)),
        Span::styled(keys, DIM),
    ]);
    f.render_widget(line, area);
}

// ---------------------------------------------------------------- Edit mode

fn draw_edit(f: &mut Frame, app: &mut App, area: Rect) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).areas(area);

    let block = panel("Editor", true);
    let inner = block.inner(left);
    f.render_widget(block, left);
    let [gutter, text] =
        Layout::horizontal([Constraint::Length(2), Constraint::Min(1)]).areas(inner);
    f.render_widget(&app.editor, text);

    // Breakpoint / error markers aligned with the editor's scroll position.
    let top = app.editor.scroll_offset().0 as usize;
    let errors: Vec<usize> = app
        .last_asm
        .as_ref()
        .filter(|(src, _)| *src == app.text())
        .map(|(_, o)| o.diagnostics.iter().map(|d| d.line).collect())
        .unwrap_or_default();
    let marks: Vec<Line> = (top..top + gutter.height as usize)
        .map(|l| {
            if errors.contains(&l) {
                Line::styled("✗", Style::new().fg(Color::Red).bold())
            } else if app.breakpoints.contains(&l) {
                Line::styled("●", Style::new().fg(Color::Red))
            } else {
                Line::raw("")
            }
        })
        .collect();
    f.render_widget(Paragraph::new(marks), gutter);

    draw_output(f, app, right);
}

fn draw_output(f: &mut Frame, app: &mut App, area: Rect) {
    let n_err = app
        .last_asm
        .as_ref()
        .map_or(0, |(_, o)| o.diagnostics.len());
    let block = panel("Output", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [tabs_area, content] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
    let err_title = if n_err > 0 {
        format!("Errors ({n_err})")
    } else {
        "Errors".to_string()
    };
    let selected = match app.out_tab {
        OutTab::Listing => 0,
        OutTab::Hex => 1,
        OutTab::Errors => 2,
    };
    let tabs = Tabs::new(vec![
        "Listing [Alt-1]".to_string(),
        "HEX [Alt-2]".to_string(),
        format!("{err_title} [Alt-3]"),
    ])
    .select(selected)
    .style(DIM)
    .highlight_style(Style::new().fg(ACCENT).bold().underlined());
    f.render_widget(tabs, tabs_area);

    let lines: Vec<Line> = match (app.out_tab, &app.last_asm) {
        (_, None) => vec![Line::styled("Press F7 to assemble.", DIM)],
        (OutTab::Listing, Some((src, out))) => out
            .listing(src)
            .lines()
            .map(|l| {
                if l.starts_with("***") {
                    Line::styled(l.to_string(), Style::new().fg(Color::Red).bold())
                } else if l.len() > 25 && l.as_bytes()[0].is_ascii_hexdigit() {
                    // LOC OBJ LINE | source
                    let (head, src) = l.split_at(25);
                    let mut spans =
                        vec![Span::styled(head.to_string(), Style::new().fg(Color::Gray))];
                    spans.extend(highlight(src, Style::new()));
                    Line::from(spans)
                } else if l.len() > 25 && l.starts_with(' ') {
                    let (head, src) = l.split_at(25);
                    let mut spans = vec![Span::styled(head.to_string(), DIM)];
                    spans.extend(highlight(src, Style::new()));
                    Line::from(spans)
                } else {
                    Line::styled(l.to_string(), LABEL)
                }
            })
            .collect(),
        (OutTab::Hex, Some((_, out))) if out.ok() => {
            out.intel_hex().lines().map(hex_line).collect()
        }
        (OutTab::Hex, Some(_)) => vec![Line::styled("Fix the errors to generate HEX output.", DIM)],
        (OutTab::Errors, Some((_, out))) if out.ok() => {
            vec![Line::styled("No errors.", Style::new().fg(Color::Green))]
        }
        (OutTab::Errors, Some((_, out))) => out
            .diagnostics
            .iter()
            .map(|d| {
                Line::from(vec![
                    Span::styled(
                        format!("line {:>4}:{:<3} ", d.line + 1, d.col + 1),
                        Style::new().fg(Color::Red),
                    ),
                    Span::raw(d.msg.clone()),
                ])
            })
            .collect(),
    };
    app.out_scroll = app.out_scroll.min(lines.len().saturating_sub(1));
    f.render_widget(
        Paragraph::new(lines).scroll((app.out_scroll as u16, 0)),
        content,
    );
}

/// Colors the fields of an Intel HEX record.
fn hex_line(l: &str) -> Line<'static> {
    if l.len() < 11 {
        return Line::raw(l.to_string());
    }
    let n = l.len();
    Line::from(vec![
        Span::styled(":", DIM),
        Span::styled(l[1..3].to_string(), Style::new().fg(Color::Magenta)),
        Span::styled(l[3..7].to_string(), Style::new().fg(Color::Yellow)),
        Span::styled(l[7..9].to_string(), Style::new().fg(Color::Blue)),
        Span::raw(l[9..n - 2].to_string()),
        Span::styled(l[n - 2..].to_string(), DIM),
    ])
}

// --------------------------------------------------------------- Debug mode

fn draw_debug(f: &mut Frame, app: &mut App, area: Rect) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(area);
    draw_source(f, app, left);

    let [top, mid, bottom] = Layout::vertical([
        Constraint::Length(10),
        Constraint::Min(6),
        Constraint::Length(8),
    ])
    .areas(right);
    let [regs, periph] =
        Layout::horizontal([Constraint::Length(30), Constraint::Min(20)]).areas(top);
    let [ports, serial] =
        Layout::horizontal([Constraint::Length(26), Constraint::Min(20)]).areas(bottom);
    draw_registers(f, app, regs);
    draw_peripherals(f, app, periph);
    draw_memory(f, app, mid);
    draw_ports(f, app, ports);
    draw_serial(f, app, serial);
}

fn draw_source(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Source;
    let block = panel("Source", focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let Some(build) = &app.build else {
        f.render_widget(
            Paragraph::new("Nothing assembled yet — press F7.").style(DIM),
            inner,
        );
        return;
    };

    let h = inner.height as usize;
    if app.src_cursor < app.src_scroll {
        app.src_scroll = app.src_cursor;
    } else if app.src_cursor >= app.src_scroll + h {
        app.src_scroll = app.src_cursor + 1 - h;
    }
    let pc_line = build.addr_to_line.get(&app.sim.pc).copied();
    let lines: Vec<Line> = build
        .source
        .iter()
        .enumerate()
        .skip(app.src_scroll)
        .take(h)
        .map(|(i, text)| {
            let out = &build.out.lines[i];
            let is_pc = pc_line == Some(i);
            let bg = if is_pc {
                Style::new().bg(PC_BG)
            } else if i == app.src_cursor && focused {
                Style::new().bg(CURSOR_BG)
            } else {
                Style::new()
            };
            let bp = if app.breakpoints.contains(&i) {
                Span::styled("●", bg.fg(Color::Red))
            } else {
                Span::styled(" ", bg)
            };
            let arrow = if is_pc {
                Span::styled("▶", bg.fg(Color::Yellow).bold())
            } else {
                Span::styled(" ", bg)
            };
            let addr = match (out.addr, out.bytes.is_empty()) {
                (Some(a), false) => Span::styled(format!(" {a:04X}  "), bg.fg(Color::Gray)),
                _ => Span::styled("       ", bg),
            };
            let mut spans = vec![bp, arrow, addr];
            spans.extend(highlight(text, bg));
            let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
            spans.push(Span::styled(
                " ".repeat((inner.width as usize).saturating_sub(used)),
                bg,
            ));
            Line::from(spans)
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

fn hex2(v: u8, changed: bool) -> Span<'static> {
    Span::styled(
        format!("{v:02X}"),
        if changed {
            CHANGED
        } else {
            Style::new().fg(Color::White)
        },
    )
}

fn flags(byte: u8, names: [&'static str; 8]) -> Vec<Span<'static>> {
    names
        .iter()
        .enumerate()
        .filter(|(_, n)| !n.is_empty())
        .flat_map(|(i, &n)| {
            let on = byte & (0x80 >> i) != 0;
            [Span::styled(n, if on { ON } else { DIM }), Span::raw(" ")]
        })
        .collect()
}

fn draw_registers(f: &mut Frame, app: &App, area: Rect) {
    let s = &app.sim;
    let block = panel("Registers", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let ch = |addr: u8| s.sfr(addr) != app.prev_sfr[(addr & 0x7F) as usize];
    let reg = |n: u8| {
        let a = s.reg_addr(n) as usize;
        vec![
            Span::styled(format!("R{n} "), LABEL),
            hex2(s.iram[a], s.iram[a] != app.prev_iram[a]),
            Span::raw("  "),
        ]
    };
    let micros = s.cycles as f64 * 12.0 / app.clock_mhz;
    let time = if micros >= 1_000_000.0 {
        format!("{:.3} s", micros / 1e6)
    } else if micros >= 1000.0 {
        format!("{:.3} ms", micros / 1e3)
    } else {
        format!("{micros:.2} µs")
    };
    let lines = vec![
        Line::from(vec![
            Span::styled("PC ", LABEL),
            Span::styled(
                format!("{:04X}", s.pc),
                Style::new().fg(Color::Yellow).bold(),
            ),
            Span::styled("  DPTR ", LABEL),
            Span::styled(
                format!("{:04X}", s.dptr()),
                if ch(sfr::DPL) || ch(sfr::DPH) {
                    CHANGED
                } else {
                    Style::new()
                },
            ),
        ]),
        Line::from(vec![
            Span::styled("A  ", LABEL),
            hex2(s.acc(), ch(sfr::ACC)),
            Span::styled("    B ", LABEL),
            hex2(s.b(), ch(sfr::B)),
            Span::styled("  SP ", LABEL),
            hex2(s.sp(), ch(sfr::SP)),
        ]),
        Line::from([0, 1, 2, 3].into_iter().flat_map(reg).collect::<Vec<_>>()),
        Line::from([4, 5, 6, 7].into_iter().flat_map(reg).collect::<Vec<_>>()),
        Line::from(
            [
                Span::styled("PSW ", LABEL),
                hex2(s.psw(), ch(PSW)),
                Span::raw(" "),
            ]
            .into_iter()
            .chain(flags(s.psw(), ["C", "AC", "F0", "", "", "OV", "", "P"]))
            .collect::<Vec<_>>(),
        ),
        Line::from(vec![
            Span::styled("Bank ", LABEL),
            Span::raw(format!("{}", s.bank())),
            Span::styled("  Cycles ", LABEL),
            Span::raw(format!("{}", s.cycles)),
        ]),
        Line::from(vec![
            Span::styled("Time ", LABEL),
            Span::raw(time),
            Span::styled(format!(" @{}MHz", app.clock_mhz), DIM),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_peripherals(f: &mut Frame, app: &App, area: Rect) {
    let s = &app.sim;
    let block = panel("Peripherals", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let ch = |addr: u8| s.sfr(addr) != app.prev_sfr[(addr & 0x7F) as usize];
    let sfr_val = |name: &'static str, addr: u8| {
        vec![
            Span::styled(name, LABEL),
            hex2(s.sfr(addr), ch(addr)),
            Span::raw("  "),
        ]
    };

    let tmod = s.sfr(TMOD);
    let tcon = s.sfr(TCON);
    let timer = |n: u8| {
        let ctl = (tmod >> (4 * n)) & 0x0F;
        let (th, tl, tr) = if n == 0 {
            (TH0, TL0, 0x10)
        } else {
            (TH1, TL1, 0x40)
        };
        let kind = if ctl & 4 != 0 { "ctr" } else { "tmr" };
        let gate = if ctl & 8 != 0 { " gate" } else { "" };
        vec![
            Span::styled(format!("T{n} "), LABEL),
            Span::styled(
                format!("{:02X}{:02X}", s.sfr(th), s.sfr(tl)),
                if ch(th) || ch(tl) {
                    CHANGED
                } else {
                    Style::new()
                },
            ),
            Span::styled(format!(" m{} {kind}{gate} ", ctl & 3), DIM),
            if tcon & tr != 0 {
                Span::styled("run", ON)
            } else {
                Span::styled("stop", DIM)
            },
            Span::raw("   "),
        ]
    };

    let scon = s.sfr(SCON);
    let irq = if s.in_service.is_empty() {
        Span::styled("none", DIM)
    } else {
        Span::styled(
            s.in_service
                .iter()
                .map(|l| format!("L{l}"))
                .collect::<Vec<_>>()
                .join(">"),
            Style::new().fg(Color::Yellow),
        )
    };
    let last = s.last_irq.map_or("-".to_string(), |v| {
        format!("{v:04X}h ({})", vector_name(v))
    });
    let mut cpu_state = vec![];
    if s.idle {
        cpu_state.push(Span::styled(" IDLE", Style::new().fg(Color::Yellow).bold()));
    }
    if s.power_down {
        cpu_state.push(Span::styled(
            " POWER-DOWN",
            Style::new().fg(Color::Red).bold(),
        ));
    }

    let lines = vec![
        Line::from([sfr_val("TMOD ", TMOD), timer(0), timer(1)].concat()),
        Line::from(
            [
                sfr_val("TCON ", TCON),
                flags(
                    tcon,
                    ["TF1", "TR1", "TF0", "TR0", "IE1", "IT1", "IE0", "IT0"],
                ),
            ]
            .concat(),
        ),
        Line::from(
            [
                sfr_val("SCON ", SCON),
                vec![Span::styled(format!("mode{} ", scon >> 6), DIM)],
                flags(scon, ["", "", "SM2", "REN", "TB8", "RB8", "TI", "RI"]),
            ]
            .concat(),
        ),
        Line::from(vec![
            Span::styled("SBUF ", LABEL),
            Span::styled("rx ", DIM),
            hex2(s.uart.sbuf_rx, false),
            Span::styled(
                if s.uart.transmitting() {
                    "  tx busy"
                } else {
                    "  tx idle"
                },
                DIM,
            ),
            Span::styled(
                format!("  {} byte(s) queued", s.uart_rx_pending().len()),
                DIM,
            ),
        ]),
        Line::from(
            [
                sfr_val("IE   ", IE),
                flags(s.sfr(IE), ["EA", "", "", "ES", "ET1", "EX1", "ET0", "EX0"]),
            ]
            .concat(),
        ),
        Line::from(
            [
                sfr_val("IP   ", IP),
                flags(s.sfr(IP), ["", "", "", "PS", "PT1", "PX1", "PT0", "PX0"]),
                sfr_val(" PCON ", PCON),
                flags(s.sfr(PCON), ["SMOD", "", "", "", "", "", "PD", "IDL"]),
            ]
            .concat(),
        ),
        Line::from(
            [
                vec![
                    Span::styled("IRQ  ", LABEL),
                    Span::styled("active ", DIM),
                    irq,
                    Span::styled("  last ", DIM),
                ],
                vec![Span::raw(last)],
                cpu_state,
            ]
            .concat(),
        ),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn vector_name(v: u16) -> &'static str {
    match v {
        0x03 => "INT0",
        0x0B => "T0",
        0x13 => "INT1",
        0x1B => "T1",
        0x23 => "UART",
        _ => "?",
    }
}

fn draw_memory(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Memory;
    let block = panel("Memory", focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [tabs_area, content] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
    let selected = match app.mem_tab {
        MemTab::Iram => 0,
        MemTab::Sfr => 1,
        MemTab::Xram => 2,
        MemTab::Code => 3,
    };
    f.render_widget(
        Tabs::new(vec!["IRAM [i]", "SFR [f]", "XRAM [d]", "CODE [c]"])
            .select(selected)
            .style(DIM)
            .highlight_style(Style::new().fg(ACCENT).bold().underlined()),
        tabs_area,
    );

    if app.mem_tab == MemTab::Sfr {
        draw_sfr_table(f, app, content);
        return;
    }

    let s = &app.sim;
    let per_row: usize = if content.width >= 74 { 16 } else { 8 };
    let total = match app.mem_tab {
        MemTab::Iram => 256,
        _ => 0x10000,
    };
    let rows = total / per_row;
    let h = content.height as usize;
    app.mem_scroll = app.mem_scroll.min(rows.saturating_sub(h));
    let bps = app.bp_addrs();
    let pc = s.pc as usize;
    let pc_len = crate::isa::info(s.code[pc]).map_or(1, |i| i.size as usize);
    let bank = s.reg_addr(0) as usize;

    let lines: Vec<Line> = (app.mem_scroll..rows.min(app.mem_scroll + h))
        .map(|row| {
            let base = row * per_row;
            let mut spans = vec![Span::styled(
                if total > 256 {
                    format!("{base:04X} ")
                } else {
                    format!("{base:02X}  ")
                },
                LABEL,
            )];
            let mut ascii = String::new();
            for i in 0..per_row {
                let a = base + i;
                let (v, style) = match app.mem_tab {
                    MemTab::Iram => {
                        let v = s.iram[a];
                        let st = if a == s.sp() as usize {
                            Style::new().fg(Color::Black).bg(Color::LightMagenta)
                        } else if v != app.prev_iram[a] {
                            CHANGED
                        } else if (bank..bank + 8).contains(&a) {
                            Style::new().fg(Color::LightBlue)
                        } else if v == 0 {
                            DIM
                        } else {
                            Style::new()
                        };
                        (v, st)
                    }
                    MemTab::Xram => {
                        let v = s.xram[a];
                        (v, if v == 0 { DIM } else { Style::new() })
                    }
                    _ => {
                        let v = s.code[a];
                        let st = if (pc..pc + pc_len).contains(&a) {
                            Style::new().fg(Color::Black).bg(Color::Yellow)
                        } else if bps.contains(&(a as u16)) {
                            Style::new().fg(Color::White).bg(Color::Red)
                        } else if v == 0 {
                            DIM
                        } else {
                            Style::new()
                        };
                        (v, st)
                    }
                };
                spans.push(Span::styled(format!("{v:02X}"), style));
                spans.push(Span::raw(if i == 7 && per_row == 16 { "  " } else { " " }));
                ascii.push(if v.is_ascii_graphic() || v == b' ' {
                    v as char
                } else {
                    '·'
                });
            }
            spans.push(Span::styled(ascii, DIM));
            Line::from(spans)
        })
        .collect();
    f.render_widget(Paragraph::new(lines), content);
}

fn draw_sfr_table(f: &mut Frame, app: &App, area: Rect) {
    let s = &app.sim;
    let cols = if area.width >= 64 { 3 } else { 2 };
    let per_col = sfr::SFRS.len().div_ceil(cols);
    let lines: Vec<Line> = (0..per_col)
        .map(|r| {
            let mut spans = Vec::new();
            for c in 0..cols {
                if let Some(&(name, addr)) = sfr::SFRS.get(c * per_col + r) {
                    let v = s.read_direct(addr, true);
                    let changed = s.sfr(addr) != app.prev_sfr[(addr & 0x7F) as usize];
                    spans.push(Span::styled(format!("{name:<5}"), LABEL));
                    spans.push(Span::styled(format!("{addr:02X} "), DIM));
                    spans.push(hex2(v, changed));
                    spans.push(Span::styled(
                        format!(" {v:08b}   "),
                        if changed { CHANGED } else { DIM },
                    ));
                }
            }
            Line::from(spans)
        })
        .collect();
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_ports(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Ports;
    let block = panel("Ports", focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let s = &app.sim;
    let mut lines: Vec<Line> = (0..4)
        .map(|p| {
            let latch = s.sfr(sfr::P0 + 0x10 * p as u8);
            let pins = s.port_pins(p);
            let mut spans = vec![Span::styled(format!("P{p} "), LABEL)];
            for bit in (0..8u8).rev() {
                let m = 1 << bit;
                let (sym, mut st) = if pins & m != 0 {
                    ("●", Style::new().fg(Color::LightGreen))
                } else if s.pins[p] & m == 0 {
                    // Pulled low from outside (e.g. a pressed button).
                    ("○", Style::new().fg(Color::LightRed))
                } else {
                    ("○", Style::new().fg(Color::DarkGray))
                };
                if focused && app.port_sel == (p, bit) {
                    st = st.add_modifier(Modifier::REVERSED);
                }
                spans.push(Span::styled(sym, st));
                spans.push(Span::raw(if bit == 4 { "  " } else { " " }));
            }
            spans.push(Span::styled(format!("{latch:02X}"), DIM));
            Line::from(spans)
        })
        .collect();
    if focused {
        lines.push(Line::styled("←→↑↓ select · space: pull pin low", DIM));
    } else {
        lines.push(Line::styled("●=1 ○=0 red=pulled low", DIM));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_serial(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Serial;
    let block = panel("Serial (UART)", focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [out_area, in_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

    let mut text = String::new();
    for &b in &app.sim.uart.tx_log {
        match b {
            b'\n' => text.push('\n'),
            b'\r' => {}
            0x20..=0x7E => text.push(b as char),
            _ => text.push('·'),
        }
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let h = out_area.height as usize;
    let shown: Vec<Line> = lines[lines.len().saturating_sub(h)..]
        .iter()
        .map(|l| Line::raw(l.to_string()))
        .collect();
    f.render_widget(Paragraph::new(shown).wrap(Wrap { trim: false }), out_area);

    let input = if focused {
        Line::from(vec![
            Span::styled("> ", Style::new().fg(ACCENT)),
            Span::raw(app.serial_input.clone()),
            Span::styled("█", Style::new().fg(ACCENT)),
        ])
    } else {
        Line::styled("Tab here to type; Enter sends text + CR", DIM)
    };
    f.render_widget(input, in_area);
}

fn draw_help(f: &mut Frame) {
    let area = f.area();
    let w = 72.min(area.width);
    let h = 31.min(area.height);
    let rect = Rect::new((area.width - w) / 2, (area.height - h) / 2, w, h);
    f.render_widget(Clear, rect);
    let k = |key: &str, desc: &str| {
        Line::from(vec![
            Span::styled(format!("  {key:<16}"), Style::new().fg(ACCENT).bold()),
            Span::raw(desc.to_string()),
        ])
    };
    let head = |t: &str| Line::styled(t.to_string(), Style::new().fg(Color::Yellow).bold());
    let lines = vec![
        head("Global"),
        k("F1", "this help"),
        k(
            "F2 / Ctrl-S",
            "save (writes .asm; F7 also writes .lst + .hex)",
        ),
        k("F3 / Ctrl-O", "open .asm, or .hex (disassembled)"),
        k("Ctrl-G", "edit in $EDITOR; reloads + reassembles on exit"),
        k("F4", "switch Edit ⇄ Debug"),
        k("F7", "assemble (reset + load)"),
        k("F5", "run / pause (assembles if needed)"),
        k("F11  F10  F12", "step into · step over · step out"),
        k("F8", "run to cursor line"),
        k("F9", "toggle breakpoint on cursor line"),
        k("F6", "reset CPU"),
        k("Ctrl-Q", "quit"),
        head("Edit mode"),
        k("Alt-1/2/3", "Listing / HEX / Errors tab"),
        k("Alt-↑↓ PgUp/Dn", "scroll output"),
        k("Ctrl-U / Ctrl-R", "undo / redo (Emacs-style editing keys)"),
        head("Debug mode"),
        k("Tab / Shift-Tab", "focus Source → Memory → Ports → Serial"),
        k(
            "r s n o g b x",
            "run · step · next · out · to cursor · bp · reset",
        ),
        k("i f d c", "memory: IRAM · SFR · XRAM · CODE"),
        k("p", "poke a byte into the shown memory"),
        k("+ / -", "simulation speed"),
        k(".", "jump source cursor to PC"),
        k("Esc", "pause / leave serial / back to editor"),
        Line::raw(""),
        Line::styled("  press any key to close", DIM),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .block(panel("Help", true))
            .style(Style::new().bg(Color::Black)),
        rect,
    );
}
