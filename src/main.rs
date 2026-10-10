mod app;
mod calendar;
mod config;
mod input;
mod integrations;
mod jira;
mod ui;

use app::{App, Response};
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyEventKind,
    },
    execute,
};
use std::{io, sync::mpsc, time::Duration};

struct Restore;
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableMouseCapture, DisableBracketedPaste);
        ratatui::restore();
    }
}

fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("herdr-jira-workspace\n\nNative Jira Kanban, comments and calendar worklogs.\n\n--demo         Explore offline; no credentials or Jira mutations\n--check        Validate configuration without resolving credentials\n--dump-demo    Print an offline board snapshot\n\nInside the UI, press ? for shortcuts.");
        return Ok(());
    }
    if args
        .iter()
        .any(|a| !["--demo", "--check", "--dump-demo"].contains(&a.as_str()))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unknown option; use --help",
        ));
    }
    if args.iter().any(|a| a == "--check") {
        match config::Config::load() {
            Ok(_) => {
                println!("Configuration valid (own config or existing herdr-jira credentials)")
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    let (tx, rx) = mpsc::channel::<Response>();
    let demo = args.iter().any(|a| a == "--demo" || a == "--dump-demo");
    let mut app = App::new(tx, demo);
    if args.iter().any(|a| a == "--dump-demo") {
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 36))?;
        terminal.draw(|f| {
            ui::draw(f, &app);
        })?;
        for row in terminal.backend().buffer().content.chunks(120) {
            println!(
                "{}",
                row.iter()
                    .map(|c| c.symbol())
                    .collect::<String>()
                    .trim_end()
            );
        }
        return Ok(());
    }
    let mut terminal = ratatui::init();
    let _restore = Restore;
    execute!(io::stdout(), EnableMouseCapture, EnableBracketedPaste)?;
    loop {
        while let Ok(response) = rx.try_recv() {
            app.on_message(response);
        }
        terminal.draw(|f| {
            let area = f.area();
            app.dimensions = (area.width, area.height);
            app.hits = ui::draw(f, &app);
        })?;
        if event::poll(Duration::from_millis(80))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key),
                Event::Mouse(mouse) => app.on_mouse(mouse),
                Event::Paste(value) => app.on_paste(&value),
                _ => {}
            }
        }
        if app.should_quit {
            break;
        }
    }
    if app.view == app::View::Image {
        let _ = integrations::clear_graphics();
    }
    Ok(())
}
