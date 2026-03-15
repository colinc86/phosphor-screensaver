use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::config::Config;
use crate::registry;
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    General,
    Theme,
    Screensavers,
}

struct ConfigUI {
    config: Config,
    section: Section,
    selected: usize,
    editing: bool,
    edit_buffer: String,
    saved: bool,
    message: Option<String>,
}

impl ConfigUI {
    fn new(config: Config) -> Self {
        Self {
            config,
            section: Section::General,
            selected: 0,
            editing: false,
            edit_buffer: String::new(),
            saved: false,
            message: None,
        }
    }

    fn items_for_section(&self) -> Vec<(String, String)> {
        match self.section {
            Section::General => vec![
                ("Timeout (seconds)".into(), self.config.general.timeout.to_string()),
                ("FPS".into(), self.config.general.fps.to_string()),
                ("Default Screensaver".into(), self.config.general.default_screensaver.clone()),
                ("Fade Enabled".into(), self.config.general.fade_enabled.to_string()),
                ("Fade Duration (ms)".into(), self.config.general.fade_duration_ms.to_string()),
                ("Clock Overlay".into(), self.config.overlay.clock.enabled.to_string()),
                ("Clock Position".into(), self.config.overlay.clock.position.clone()),
                ("Lock Mode".into(), self.config.overlay.lock.enabled.to_string()),
                ("Battery Aware".into(), self.config.power.battery_aware.to_string()),
                ("Screen Blanking (secs)".into(), self.config.blanking.delay_secs.to_string()),
                ("Auto-Rotate".into(), self.config.rotate.enabled.to_string()),
                ("Rotate Interval (secs)".into(), self.config.rotate.interval_secs.to_string()),
            ],
            Section::Theme => {
                Theme::available()
                    .iter()
                    .map(|name| {
                        let marker = if *name == self.config.theme.name {
                            " [active]"
                        } else {
                            ""
                        };
                        (format!("{name}{marker}"), String::new())
                    })
                    .collect()
            }
            Section::Screensavers => {
                registry::SCREENSAVERS
                    .iter()
                    .map(|info| (info.name.to_string(), info.description.to_string()))
                    .collect()
            }
        }
    }

    fn apply_edit(&mut self) {
        if !self.editing {
            return;
        }
        self.editing = false;
        let val = self.edit_buffer.clone();

        match self.section {
            Section::General => match self.selected {
                0 => {
                    if let Ok(v) = val.parse() {
                        self.config.general.timeout = v;
                    }
                }
                1 => {
                    if let Ok(v) = val.parse() {
                        self.config.general.fps = v;
                    }
                }
                2 => self.config.general.default_screensaver = val,
                3 => {
                    self.config.general.fade_enabled = !self.config.general.fade_enabled;
                }
                4 => {
                    if let Ok(v) = val.parse() {
                        self.config.general.fade_duration_ms = v;
                    }
                }
                5 => {
                    self.config.overlay.clock.enabled = !self.config.overlay.clock.enabled;
                }
                6 => self.config.overlay.clock.position = val,
                7 => {
                    self.config.overlay.lock.enabled = !self.config.overlay.lock.enabled;
                }
                8 => {
                    self.config.power.battery_aware = !self.config.power.battery_aware;
                }
                9 => {
                    if let Ok(v) = val.parse() {
                        self.config.blanking.delay_secs = v;
                    }
                }
                10 => {
                    self.config.rotate.enabled = !self.config.rotate.enabled;
                }
                11 => {
                    if let Ok(v) = val.parse() {
                        self.config.rotate.interval_secs = v;
                    }
                }
                _ => {}
            },
            Section::Theme => {
                if let Some(name) = Theme::available().get(self.selected) {
                    self.config.theme.name = name.to_string();
                }
            }
            Section::Screensavers => {
                // Selecting a screensaver sets it as default
                if let Some(info) = registry::SCREENSAVERS.get(self.selected) {
                    self.config.general.default_screensaver = info.name.to_string();
                    self.message = Some(format!("Default set to: {}", info.name));
                }
            }
        }
    }

    fn save(&mut self) {
        match self.config.save() {
            Ok(()) => {
                self.saved = true;
                self.message = Some("Configuration saved!".into());
            }
            Err(e) => {
                self.message = Some(format!("Save failed: {e}"));
            }
        }
    }
}

pub fn run(config: Config) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::cursor::Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut ui = ConfigUI::new(config);

    loop {
        terminal.draw(|frame| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Title
                    Constraint::Length(3), // Tab bar
                    Constraint::Min(10),   // Content
                    Constraint::Length(3), // Status/help
                ])
                .split(frame.area());

            // Title
            let title = Paragraph::new("Termsaver Configuration")
                .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                .block(Block::default().borders(Borders::BOTTOM));
            frame.render_widget(title, chunks[0]);

            // Tab bar
            let tabs: Vec<Span> = [
                ("General", Section::General),
                ("Theme", Section::Theme),
                ("Screensavers", Section::Screensavers),
            ]
            .iter()
            .map(|(name, sec)| {
                if *sec == ui.section {
                    Span::styled(
                        format!(" [{name}] "),
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::styled(format!("  {name}  "), Style::default().fg(Color::DarkGray))
                }
            })
            .collect();
            let tab_line = Paragraph::new(Line::from(tabs));
            frame.render_widget(tab_line, chunks[1]);

            // Content
            let items = ui.items_for_section();
            let list_items: Vec<ListItem> = items
                .iter()
                .enumerate()
                .map(|(i, (key, val))| {
                    let content = if val.is_empty() {
                        key.clone()
                    } else if ui.editing && i == ui.selected {
                        format!("{key}: {}_", ui.edit_buffer)
                    } else {
                        format!("{key}: {val}")
                    };
                    let style = if i == ui.selected {
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    ListItem::new(format!("  {content}")).style(style)
                })
                .collect();

            let mut list_state = ListState::default();
            list_state.select(Some(ui.selected));
            let list = List::new(list_items)
                .block(Block::default().borders(Borders::ALL).title("Settings"));
            frame.render_stateful_widget(list, chunks[2], &mut list_state);

            // Status bar
            let status = if let Some(ref msg) = ui.message {
                msg.clone()
            } else if ui.editing {
                "Type value, Enter to confirm, Esc to cancel".into()
            } else {
                "↑↓ Navigate  Tab: Section  Enter: Edit  s: Save  q: Quit  p: Preview".into()
            };
            let status_bar = Paragraph::new(status)
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::TOP));
            frame.render_widget(status_bar, chunks[3]);
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(KeyEvent {
                code, modifiers, ..
            }) = event::read()?
            {
                ui.message = None;

                if ui.editing {
                    match code {
                        KeyCode::Enter => ui.apply_edit(),
                        KeyCode::Esc => ui.editing = false,
                        KeyCode::Backspace => {
                            ui.edit_buffer.pop();
                        }
                        KeyCode::Char(c) => ui.edit_buffer.push(c),
                        _ => {}
                    }
                    continue;
                }

                match code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Up | KeyCode::Char('k') => {
                        let items = ui.items_for_section();
                        if ui.selected > 0 {
                            ui.selected -= 1;
                        } else {
                            ui.selected = items.len().saturating_sub(1);
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let items = ui.items_for_section();
                        ui.selected = (ui.selected + 1) % items.len().max(1);
                    }
                    KeyCode::Tab => {
                        ui.section = match ui.section {
                            Section::General => Section::Theme,
                            Section::Theme => Section::Screensavers,
                            Section::Screensavers => Section::General,
                        };
                        ui.selected = 0;
                    }
                    KeyCode::BackTab => {
                        ui.section = match ui.section {
                            Section::General => Section::Screensavers,
                            Section::Theme => Section::General,
                            Section::Screensavers => Section::Theme,
                        };
                        ui.selected = 0;
                    }
                    KeyCode::Enter => {
                        match ui.section {
                            Section::General => {
                                // Toggle booleans directly, edit text values
                                match ui.selected {
                                    3 | 5 | 7 | 8 | 10 => {
                                        ui.editing = true;
                                        ui.apply_edit();
                                    }
                                    _ => {
                                        let items = ui.items_for_section();
                                        ui.edit_buffer = items[ui.selected].1.clone();
                                        ui.editing = true;
                                    }
                                }
                            }
                            Section::Theme => {
                                ui.apply_edit();
                            }
                            Section::Screensavers => {
                                ui.apply_edit();
                            }
                        }
                    }
                    KeyCode::Char('s') => ui.save(),
                    KeyCode::Char('p') => {
                        // Preview: exit TUI temporarily, run screensaver for 5 seconds
                        disable_raw_mode()?;
                        execute!(
                            terminal.backend_mut(),
                            LeaveAlternateScreen,
                            crossterm::cursor::Show
                        )?;

                        let preview_config = ui.config.clone();
                        crate::app::run(&preview_config, Some(Duration::from_secs(5)))?;

                        enable_raw_mode()?;
                        execute!(
                            io::stdout(),
                            EnterAlternateScreen,
                            crossterm::cursor::Hide
                        )?;
                        terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
                        ui.message = Some("Preview finished".into());
                    }
                    _ => {}
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::cursor::Show
    )?;

    Ok(())
}
