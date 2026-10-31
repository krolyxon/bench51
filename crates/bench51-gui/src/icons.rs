//! Monoline toolbar icons drawn with the painter: 1.5 px strokes on a
//! 16-unit grid, at most six strokes each, no fills.

use std::f32::consts::TAU;

use eframe::egui::{Color32, IconData, Painter, Pos2, Rect, Shape, Stroke, vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    New,
    Open,
    Save,
    Build,
    Debug,
    Reset,
    Run,
    Pause,
    StepInto,
    StepOver,
    StepOut,
    RunToCursor,
    Breakpoint,
}

pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height()) / 16.0;
    let o = rect.center() - vec2(8.0, 8.0) * s;
    let p = |x: f32, y: f32| o + vec2(x, y) * s;
    let stroke = Stroke::new(1.5, color);
    let line = |pts: &[(f32, f32)]| {
        painter.add(Shape::line(
            pts.iter().map(|&(x, y)| p(x, y)).collect(),
            stroke,
        ));
    };
    let closed = |pts: &[(f32, f32)]| {
        painter.add(Shape::closed_line(
            pts.iter().map(|&(x, y)| p(x, y)).collect(),
            stroke,
        ));
    };
    // Arc around (cx, cy) from angle a0 to a1 (radians, clockwise on screen).
    let arc = |cx: f32, cy: f32, r: f32, a0: f32, a1: f32| -> Vec<Pos2> {
        let n = 24;
        (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                p(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    };
    let dot = |x: f32, y: f32| painter.circle_stroke(p(x, y), 1.5 * s, stroke);

    match icon {
        Icon::New => {
            closed(&[
                (3.0, 1.5),
                (9.5, 1.5),
                (13.0, 5.0),
                (13.0, 14.5),
                (3.0, 14.5),
            ]);
            line(&[(9.5, 1.5), (9.5, 5.0), (13.0, 5.0)]);
        }
        Icon::Open => {
            closed(&[
                (1.5, 3.0),
                (6.0, 3.0),
                (7.5, 4.5),
                (14.5, 4.5),
                (14.5, 13.0),
                (1.5, 13.0),
            ]);
            line(&[(1.5, 7.0), (14.5, 7.0)]);
        }
        Icon::Save => {
            line(&[(8.0, 1.5), (8.0, 10.0)]);
            line(&[(4.5, 6.5), (8.0, 10.0), (11.5, 6.5)]);
            line(&[(2.0, 10.0), (2.0, 14.0), (14.0, 14.0), (14.0, 10.0)]);
        }
        Icon::Build => {
            // A DIP chip: body and four pins.
            closed(&[(4.0, 3.5), (12.0, 3.5), (12.0, 12.5), (4.0, 12.5)]);
            for y in [6.0, 10.0] {
                line(&[(1.5, y), (4.0, y)]);
                line(&[(12.0, y), (14.5, y)]);
            }
        }
        Icon::Debug => {
            // Power switch: starts and stops the debug session.
            painter.add(Shape::line(
                arc(8.0, 9.0, 5.5, -TAU * 0.17, TAU * 0.67),
                stroke,
            ));
            line(&[(8.0, 1.5), (8.0, 8.0)]);
        }
        Icon::Reset => {
            painter.add(Shape::line(
                arc(8.0, 8.5, 5.5, -TAU * 0.2, TAU * 0.6),
                stroke,
            ));
            let tip = (
                8.0 + 5.5 * (TAU * 0.6f32).cos(),
                8.5 + 5.5 * (TAU * 0.6f32).sin(),
            );
            line(&[(tip.0 - 3.5, tip.1 - 0.5), tip, (tip.0 + 0.5, tip.1 - 3.5)]);
        }
        Icon::Run => closed(&[(4.5, 2.5), (13.0, 8.0), (4.5, 13.5)]),
        Icon::Pause => {
            line(&[(5.5, 3.0), (5.5, 13.0)]);
            line(&[(10.5, 3.0), (10.5, 13.0)]);
        }
        Icon::StepInto => {
            line(&[(8.0, 1.5), (8.0, 9.5)]);
            line(&[(4.5, 6.0), (8.0, 9.5), (11.5, 6.0)]);
            dot(8.0, 13.5);
        }
        Icon::StepOver => {
            painter.add(Shape::line(arc(8.0, 9.0, 5.5, TAU * 0.5, TAU), stroke));
            line(&[(10.5, 8.0), (13.5, 9.0), (14.5, 6.0)]);
            dot(8.0, 13.5);
        }
        Icon::StepOut => {
            line(&[(8.0, 10.5), (8.0, 2.0)]);
            line(&[(4.5, 5.5), (8.0, 2.0), (11.5, 5.5)]);
            dot(8.0, 13.5);
        }
        Icon::RunToCursor => {
            line(&[(1.5, 8.0), (11.0, 8.0)]);
            line(&[(7.5, 4.5), (11.0, 8.0), (7.5, 11.5)]);
            line(&[(14.5, 3.0), (14.5, 13.0)]);
        }
        Icon::Breakpoint => {
            painter.circle_stroke(p(8.0, 8.0), 5.0 * s, stroke);
        }
    }
}

/// Window icon: a dot-matrix "51" in white on black with one red dot.
pub fn app_icon() -> IconData {
    const N: usize = 64;
    // 5×7 glyphs.
    const FIVE: [&str; 7] = [
        "#####", "#....", "####.", "....#", "....#", "#...#", ".###.",
    ];
    const ONE: [&str; 7] = [
        "..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###.",
    ];
    let mut rgba = vec![0u8; N * N * 4];
    // Rounded black tile.
    let r = 12.0;
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let dx = (r - fx).max(fx - (N as f32 - r)).max(0.0);
            let dy = (r - fy).max(fy - (N as f32 - r)).max(0.0);
            let a = (r + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
            rgba[(y * N + x) * 4 + 3] = (a * 255.0) as u8;
        }
    }
    let mut put_disc = |cx: f32, cy: f32, r: f32, c: [u8; 3]| {
        for y in 0..N {
            for x in 0..N {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let a = (r + 0.5 - d).clamp(0.0, 1.0);
                if a > 0.0 {
                    let i = (y * N + x) * 4;
                    for k in 0..3 {
                        rgba[i + k] = (rgba[i + k] as f32 * (1.0 - a) + c[k] as f32 * a) as u8;
                    }
                    rgba[i + 3] = 255;
                }
            }
        }
    };
    let pitch = 4.0;
    let (x0, y0) = (11.0, 19.0);
    for (gi, glyph) in [FIVE, ONE].iter().enumerate() {
        for (row, bits) in glyph.iter().enumerate() {
            for (col, b) in bits.bytes().enumerate() {
                if b == b'#' {
                    let cx = x0 + (gi * 6 + col) as f32 * pitch + pitch / 2.0;
                    let cy = y0 + row as f32 * pitch + pitch / 2.0;
                    put_disc(cx, cy, 1.5, [255, 255, 255]);
                }
            }
        }
    }
    put_disc(52.0, 12.0, 3.0, [0xD7, 0x19, 0x21]);
    IconData {
        rgba,
        width: N as u32,
        height: N as u32,
    }
}

/// Square box for an icon of size `size` centred in `rect`.
pub fn icon_rect(rect: Rect, size: f32) -> Rect {
    Rect::from_center_size(rect.center(), vec2(size, size))
}
