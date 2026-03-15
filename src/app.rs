use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::config::Config;
use crate::fade;
use crate::overlay;
use crate::power;
use crate::registry;
use crate::screensaver::{EventResponse, Screensaver};

/// Run the screensaver application.
pub fn run(config: &Config, preview_duration: Option<Duration>) -> io::Result<()> {
    // Select screensaver
    let screensaver_name = &config.general.default_screensaver;
    let mut saver: Box<dyn Screensaver> = if screensaver_name == "random" {
        registry::create_random(config)
    } else {
        registry::create(screensaver_name, config).unwrap_or_else(|| {
            eprintln!(
                "Unknown screensaver '{}', using random",
                screensaver_name
            );
            registry::create_random(config)
        })
    };

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::cursor::Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Run fade-out transition
    if config.general.fade_enabled {
        fade::fade_out(&mut terminal, config.general.fade_duration_ms)?;
    }

    // Initialize screensaver
    let size = terminal.size()?;
    saver.init(size.width, size.height);

    // Determine FPS
    let base_fps = if config.power.battery_aware {
        let state = power::detect_power_state();
        power::fps_for_power_state(
            state,
            config.power.battery_fps,
            config.power.ac_fps,
            true,
        )
    } else {
        config.general.fps
    };
    let mut frame_duration = Duration::from_millis(1000 / base_fps.max(1) as u64);

    let mut last_frame = Instant::now();
    let start_time = Instant::now();
    let mut blanked = false;
    let blank_delay = Duration::from_secs(config.blanking.delay_secs);

    // Auto-rotate state
    let mut rotate_timer = Instant::now();
    let rotate_interval = Duration::from_secs(config.rotate.interval_secs);
    let mut rotate_index = 0usize;

    // Lock mode state
    let mut lock_first_key = config.overlay.lock.enabled;

    // Power check timer
    let mut power_check = Instant::now();
    let power_check_interval = Duration::from_secs(30);

    loop {
        // Check for preview timeout
        if let Some(dur) = preview_duration {
            if start_time.elapsed() >= dur {
                break;
            }
        }

        // Poll for events
        if event::poll(frame_duration)? {
            let ev = event::read()?;

            // Handle resize
            if let Event::Resize(w, h) = ev {
                saver.init(w, h);
                continue;
            }

            // Handle key events
            if let Event::Key(key_event) = &ev {
                // Escape and 'q' always exit
                if matches!(
                    key_event,
                    KeyEvent {
                        code: KeyCode::Esc,
                        ..
                    } | KeyEvent {
                        code: KeyCode::Char('q'),
                        ..
                    }
                ) {
                    break;
                }

                // If blanked, unblank on any key
                if blanked {
                    blanked = false;
                    saver.init(terminal.size()?.width, terminal.size()?.height);
                    continue;
                }

                // Lock mode: first keypress just dismisses the lock message
                if lock_first_key {
                    lock_first_key = false;
                    continue;
                }

                // Let the screensaver handle the event
                match saver.handle_event(&ev) {
                    EventResponse::Exit => break,
                    EventResponse::Continue => {}
                    EventResponse::Ignored => break, // Any unhandled key exits
                }
            }
        }

        // Calculate delta time
        let now = Instant::now();
        let dt = now - last_frame;
        last_frame = now;

        // Screen blanking
        if config.blanking.enabled && !blanked && start_time.elapsed() >= blank_delay {
            blanked = true;
            terminal.draw(|_frame| {})?;
            continue;
        }

        if blanked {
            continue;
        }

        // Update screensaver
        saver.update(dt);

        // Auto-rotate
        if config.rotate.enabled && rotate_timer.elapsed() >= rotate_interval {
            rotate_timer = Instant::now();
            let rotate_list = &config.rotate.screensavers;
            if !rotate_list.is_empty() {
                rotate_index = (rotate_index + 1) % rotate_list.len();
                if let Some(new_saver) = registry::create(&rotate_list[rotate_index], config) {
                    saver = new_saver;
                    let size = terminal.size()?;
                    saver.init(size.width, size.height);
                }
            }
        }

        // Battery-aware FPS adjustment
        if config.power.battery_aware && power_check.elapsed() >= power_check_interval {
            power_check = Instant::now();
            let state = power::detect_power_state();
            let new_fps = power::fps_for_power_state(
                state,
                config.power.battery_fps,
                config.power.ac_fps,
                true,
            );
            frame_duration = Duration::from_millis(1000 / new_fps.max(1) as u64);
        }

        // Render
        terminal.draw(|frame| {
            saver.render(frame);
            overlay::render_clock(frame, &config.overlay);
            if config.overlay.lock.enabled && lock_first_key {
                overlay::render_lock(frame, &config.overlay.lock.message);
            }
        })?;
    }

    // Fade in (reverse)
    if config.general.fade_enabled {
        fade::fade_in(&mut terminal, config.general.fade_duration_ms / 2)?;
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
