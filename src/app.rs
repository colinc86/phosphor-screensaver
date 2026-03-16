use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use chrono::Timelike;

use crate::ambient::AmbientState;
use crate::auth;
use crate::config::Config;
use crate::fade;
use crate::overlay;
use crate::power;
use crate::registry;
use crate::screensaver::{EventResponse, Screensaver};
use crate::transition::{Transition, TransitionStyle};

/// Run the screensaver application.
pub fn run(config: &Config, preview_duration: Option<Duration>) -> io::Result<()> {
    // Select screensaver, checking schedule first
    let screensaver_name = if config.schedule.enabled {
        scheduled_screensaver(&config.schedule)
            .unwrap_or(&config.general.default_screensaver)
    } else {
        &config.general.default_screensaver
    };
    let mut saver: Box<dyn Screensaver> = registry::create(screensaver_name, config)
        .unwrap_or_else(|| {
            eprintln!(
                "Unknown screensaver '{}', using random",
                screensaver_name
            );
            registry::create_random(config)
        });

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

    // Transition state
    let transition_style = TransitionStyle::from_name(&config.rotate.transition_style);
    let transition_duration_ms = config.rotate.transition_duration_ms;
    let mut active_transition: Option<Transition> = None;

    // Lock mode state
    let password_lock = config.overlay.lock.enabled && config.overlay.lock.password_required;
    let mut lock_first_key = config.overlay.lock.enabled;
    let mut password_mode = false;
    let mut password_buffer = String::new();
    let mut auth_message: Option<(String, Instant)> = None;
    let username = auth::current_username().unwrap_or_default();

    // Power check timer
    let mut power_check = Instant::now();
    let power_check_interval = Duration::from_secs(30);

    // Ambient state
    let mut ambient = if config.general.ambient_enabled {
        AmbientState::from_now(config.general.ambient_intensity)
    } else {
        AmbientState::neutral()
    };
    let mut ambient_check = Instant::now();
    let ambient_check_interval = Duration::from_secs(60);

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

            // Forward mouse events to screensaver
            if let Event::Mouse(_) = &ev {
                let _ = saver.handle_event(&ev);
                continue;
            }

            // Handle key events
            if let Event::Key(key_event) = &ev {
                // Password mode: capture typing
                if password_mode {
                    match key_event.code {
                        KeyCode::Enter => {
                            // Verify password
                            if auth::verify_password(&username, &password_buffer) {
                                break; // Authenticated — exit screensaver
                            } else {
                                auth_message =
                                    Some(("Incorrect password".into(), Instant::now()));
                                password_buffer.clear();
                            }
                        }
                        KeyCode::Esc => {
                            // Cancel password entry, return to screensaver
                            password_mode = false;
                            password_buffer.clear();
                            auth_message = None;
                        }
                        KeyCode::Backspace => {
                            password_buffer.pop();
                        }
                        KeyCode::Char(c) => {
                            password_buffer.push(c);
                        }
                        _ => {}
                    }
                    // Clear auth message after 2 seconds
                    if let Some((_, t)) = &auth_message {
                        if t.elapsed() > Duration::from_secs(2) {
                            auth_message = None;
                        }
                    }
                    continue;
                }

                // If blanked, unblank on any key
                if blanked {
                    blanked = false;
                    saver.init(terminal.size()?.width, terminal.size()?.height);
                    if password_lock {
                        password_mode = true;
                        password_buffer.clear();
                        auth_message = None;
                    }
                    continue;
                }

                // Lock mode with password: enter password mode on any key
                if lock_first_key && password_lock {
                    lock_first_key = false;
                    password_mode = true;
                    password_buffer.clear();
                    continue;
                }

                // Lock mode without password: first keypress dismisses the lock
                if lock_first_key {
                    lock_first_key = false;
                    continue;
                }

                // Open menu
                if matches!(key_event.code, KeyCode::Char('m')) {
                    disable_raw_mode()?;
                    execute!(
                        terminal.backend_mut(),
                        LeaveAlternateScreen,
                        crossterm::cursor::Show
                    )?;
                    drop(terminal);

                    let menu_result = crate::menu::run(
                        config,
                        crate::menu::MenuContext::DuringScreensaver {
                            name: saver.name().to_string(),
                        },
                    )?;

                    enable_raw_mode()?;
                    let mut stdout = io::stdout();
                    execute!(stdout, EnterAlternateScreen, crossterm::cursor::Hide)?;
                    terminal = Terminal::new(CrosstermBackend::new(stdout))?;

                    match menu_result {
                        crate::menu::MenuResult::Resume => {
                            let size = terminal.size()?;
                            saver.init(size.width, size.height);
                        }
                        crate::menu::MenuResult::Launch(name) => {
                            if let Some(new) = registry::create(&name, config) {
                                saver = new;
                            }
                            let size = terminal.size()?;
                            saver.init(size.width, size.height);
                        }
                        crate::menu::MenuResult::Quit => break,
                    }
                    last_frame = Instant::now();
                    continue;
                }

                // Non-locked mode: Escape and 'q' always exit
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

        // Update screensaver (scale dt by ambient energy)
        let effective_dt = if config.general.ambient_enabled {
            Duration::from_secs_f64(dt.as_secs_f64() * ambient.energy)
        } else {
            dt
        };
        saver.update(effective_dt);

        // Update active transition
        if let Some(ref mut trans) = active_transition {
            if !trans.update(dt.as_secs_f64()) {
                active_transition = None;
            }
        }

        // Auto-rotate
        if config.rotate.enabled && rotate_timer.elapsed() >= rotate_interval {
            rotate_timer = Instant::now();
            let rotate_list = &config.rotate.screensavers;
            if !rotate_list.is_empty() {
                rotate_index = (rotate_index + 1) % rotate_list.len();
                if let Some(new_saver) = registry::create(&rotate_list[rotate_index], config) {
                    // Capture old buffer for transition
                    if transition_style != TransitionStyle::None {
                        let buf = terminal.current_buffer_mut();
                        active_transition = Some(Transition::new(
                            transition_style,
                            transition_duration_ms,
                            buf,
                        ));
                    }

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

        // Refresh ambient state periodically
        if config.general.ambient_enabled && ambient_check.elapsed() >= ambient_check_interval {
            ambient_check = Instant::now();
            ambient = AmbientState::from_now(config.general.ambient_intensity);
        }

        // Clear expired auth message
        if let Some((_, t)) = &auth_message {
            if t.elapsed() > Duration::from_secs(2) {
                auth_message = None;
            }
        }

        // Render
        let pw_mode = password_mode;
        let pw_len = password_buffer.len();
        let auth_msg = auth_message.as_ref().map(|(m, _)| m.as_str());
        let lock_active = lock_first_key;
        let trans_ref = &active_transition;
        let ambient_ref = &ambient;
        terminal.draw(|frame| {
            saver.render(frame);

            // Apply transition effect if active
            if let Some(trans) = trans_ref {
                trans.apply(frame.buffer_mut());
            }

            // Apply ambient color shift (before overlays so they stay full brightness)
            if config.general.ambient_enabled && !ambient_ref.is_neutral() {
                for cell in &mut frame.buffer_mut().content {
                    cell.fg = ambient_ref.apply_ratatui_color(cell.fg);
                    cell.bg = ambient_ref.apply_ratatui_color(cell.bg);
                }
            }

            overlay::render_clock(frame, &config.overlay);
            if config.overlay.lock.enabled && lock_active && !pw_mode {
                overlay::render_lock(frame, &config.overlay.lock.message);
            }
            if pw_mode {
                overlay::render_password_prompt(frame, pw_len, auth_msg);
            }
        })?;
    }

    // Record session stats
    {
        let session_duration = start_time.elapsed();
        let saver_name = saver.name().to_string();
        let mut stats = crate::stats::Stats::load();
        stats.record_session(&saver_name, session_duration);
        let _ = stats.save();
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

fn scheduled_screensaver(schedule: &crate::config::ScheduleConfig) -> Option<&str> {
    let hour = chrono::Local::now().hour();
    let name = match hour {
        6..=11 => &schedule.morning,
        12..=17 => &schedule.afternoon,
        18..=21 => &schedule.evening,
        _ => &schedule.night, // 22-23, 0-5
    };
    if name.is_empty() {
        None
    } else {
        Some(name.as_str())
    }
}
