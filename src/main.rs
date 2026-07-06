mod app;
mod basecamp;
mod config;
mod models;
mod storage;
mod ui;

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::App;

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {e}");
    }
    Ok(())
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new();

    loop {
        terminal.draw(|f| ui::render(f, &app))?;

        // Poll for input with 100ms timeout (needed for timer refresh + notification decay)
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Ignore key-release events on Windows
                if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat {
                    app.handle_key(key);
                }
            }
        }

        // Drain background messages
        while let Ok(msg) = app.rx.try_recv() {
            app.handle_msg(msg);
        }

        app.tick_notification();

        // Poll chat lines periodically when in chat screen
        {
            use app::Screen;
            if let Screen::Chat(ref mut s) = app.screen {
                if s.last_poll.elapsed().as_secs() >= 10 {
                    s.last_poll = std::time::Instant::now();
                    if let Some(ref room) = s.current.clone() {
                        app.spawn_load_chat_lines(room.bucket_id, room.id);
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
