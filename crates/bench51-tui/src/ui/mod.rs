mod app;
pub mod draw;
mod syntax;

use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use app::App;

const FRAME: Duration = Duration::from_millis(33);

pub fn run(path: Option<PathBuf>, clock_mhz: f64) -> io::Result<()> {
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    let mut app = App::new(path, clock_mhz);
    let result = event_loop(&mut terminal, &mut app);
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    let mut next_frame = Instant::now();
    while !app.quit {
        terminal.draw(|f| draw::draw(f, app))?;
        // Wait for input until the next frame is due; while running, the
        // simulation advances once per frame.
        let timeout = next_frame.saturating_duration_since(Instant::now());
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(k) if k.kind != KeyEventKind::Release => app.on_key(k),
                Event::Mouse(m) => app.on_mouse(m),
                _ => {}
            }
        }
        if std::mem::take(&mut app.external_edit) {
            suspend()?;
            app.edit_externally();
            resume(terminal)?;
        }
        if Instant::now() >= next_frame {
            app.tick();
            next_frame = Instant::now() + FRAME;
        }
    }
    Ok(())
}

/// Hands the terminal back to the shell (for `$EDITOR`).
fn suspend() -> io::Result<()> {
    execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen)?;
    disable_raw_mode()
}

fn resume(terminal: &mut ratatui::DefaultTerminal) -> io::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    terminal.clear()
}
