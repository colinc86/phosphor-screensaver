use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent};
use crossterm::execute;
use crossterm::terminal::{
    BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen, LeaveAlternateScreen,
    disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use chrono::Timelike;

use crate::ambient::AmbientState;
use crate::auth;
use crate::config::Config;
use crate::fade;
use crate::filter::FilterState;
use crate::overlay;
use crate::plugin_host::PluginRenderer;
use crate::power;
use crate::registry;
use crate::screensaver::EventResponse;
use crate::transition::{Transition, TransitionStyle};

/// Create a PluginRenderer for the given screensaver name.
/// Tries subprocess discovery first, falls back to in-process.
fn create_plugin(name: &str, config: &Config) -> Box<dyn PluginRenderer> {
    registry::create_plugin(name, config).unwrap_or_else(|| {
        eprintln!("Unknown screensaver '{name}', using random");
        registry::create_random_plugin(config)
    })
}

/// Guard that restores the terminal on drop, even if the function exits
/// via an error or panic.  Declared *after* `saver` so it is dropped
/// *before* the SubprocessPlugin (Rust drops in reverse declaration order),
/// ensuring the user gets their terminal back immediately.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
    }
}

/// Run the screensaver application.
pub fn run(config: &Config, preview_duration: Option<Duration>) -> io::Result<()> {
    let mut config = config.clone();
    // Select screensaver, checking schedule first
    let screensaver_name = if config.schedule.enabled {
        scheduled_screensaver(&config.schedule).unwrap_or(&config.general.default_screensaver)
    } else {
        &config.general.default_screensaver
    };
    let mut saver = create_plugin(screensaver_name, &config);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::cursor::Hide)?;
    // Terminal cleanup guard — dropped before `saver`, so the terminal is
    // restored even if SubprocessPlugin::drop() blocks briefly.
    let _terminal_guard = TerminalGuard;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Run fade-out transition
    if config.general.fade_enabled {
        fade::fade_out(&mut terminal, config.general.fade_duration_ms)?;
    }

    // Initialize screensaver
    let size = terminal.size()?;
    let mut saver_w = size.width;
    let mut saver_h = size.height;
    saver.init(saver_w, saver_h)?;

    // Determine FPS
    let base_fps = if config.power.battery_aware {
        let state = power::detect_power_state();
        power::fps_for_power_state(state, config.power.battery_fps, config.power.ac_fps, true)
    } else {
        config.general.fps
    };
    let mut frame_duration = Duration::from_millis(1000 / base_fps.max(1) as u64);

    let mut last_frame = Instant::now();
    let start_time = Instant::now();
    let mut blanked = false;
    let mut blank_delay = Duration::from_secs(config.blanking.delay_secs);
    // Separate timer for blanking — resets on activity (auto-rotate, unblank)
    // so the screen doesn't permanently blank while content is still changing.
    let mut blank_timer = Instant::now();

    // Auto-rotate state
    let mut rotate_timer = Instant::now();
    let mut rotate_interval = Duration::from_secs(config.rotate.interval_secs);
    let mut rotate_index = 0usize;

    // Transition state
    let mut transition_style = TransitionStyle::from_name(&config.rotate.transition_style);
    let mut transition_duration_ms = config.rotate.transition_duration_ms;
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

    // Filter state
    let mut filter_state = FilterState::new(saver_w, saver_h);

    // OSD state (on-screen display for +/- cycling)
    let mut osd_message: Option<(String, Instant)> = None;

    'outer: loop {
        // Check for preview timeout
        if let Some(dur) = preview_duration
            && start_time.elapsed() >= dur
        {
            break;
        }

        // Process all pending events within the frame time budget.
        // Compensate poll timeout for time already elapsed since last render
        // to maintain consistent frame intervals.
        loop {
            let remaining = frame_duration.saturating_sub(last_frame.elapsed());
            // poll/read can fail transiently (e.g. EINTR from signals when the
            // display sleeps/wakes).  Treat errors as "no event" rather than
            // crashing the screensaver.
            match event::poll(remaining) {
                Ok(false) => break, // Frame time elapsed, proceed to render
                Ok(true) => {}      // Event available, read it below
                Err(_) => break,    // Transient error, proceed to render
            }
            let ev = match event::read() {
                Ok(ev) => ev,
                Err(_) => break, // Transient read error, proceed to render
            };

            // Handle resize — clear ratatui's buffers so the next draw does
            // a full repaint (fixes stale content after display sleep/wake,
            // which many terminals signal via Resize).
            if let Event::Resize(w, h) = ev {
                saver_w = w;
                saver_h = h;
                saver.init(w, h)?;
                let _ = terminal.clear();
                break; // Render immediately after resize
            }

            // Forward mouse events to screensaver
            if let Event::Mouse(_) = &ev {
                let _ = saver.handle_event(&ev);
                continue; // Keep draining events
            }

            // Handle key events
            if let Event::Key(key_event) = &ev {
                // Password mode: capture typing
                if password_mode {
                    match key_event.code {
                        KeyCode::Enter => {
                            // Verify password
                            if auth::verify_password(&username, &password_buffer) {
                                break 'outer; // Authenticated — exit screensaver
                            } else {
                                auth_message = Some(("Incorrect password".into(), Instant::now()));
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
                    if let Some((_, t)) = &auth_message
                        && t.elapsed() > Duration::from_secs(2)
                    {
                        auth_message = None;
                    }
                    continue; // Keep draining events
                }

                // If blanked, unblank on any key
                if blanked {
                    blanked = false;
                    blank_timer = Instant::now();
                    let size = terminal.size()?;
                    saver_w = size.width;
                    saver_h = size.height;
                    saver.init(saver_w, saver_h)?;
                    if password_lock {
                        password_mode = true;
                        password_buffer.clear();
                        auth_message = None;
                    }
                    break; // Render immediately to show unblanked state
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
                        &config,
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
                            config = Config::load();
                            frame_duration =
                                Duration::from_millis(1000 / config.general.fps.max(1) as u64);
                            rotate_interval = Duration::from_secs(config.rotate.interval_secs);
                            transition_style =
                                TransitionStyle::from_name(&config.rotate.transition_style);
                            transition_duration_ms = config.rotate.transition_duration_ms;
                            blank_delay = Duration::from_secs(config.blanking.delay_secs);

                            let size = terminal.size()?;
                            saver_w = size.width;
                            saver_h = size.height;
                            saver.init(saver_w, saver_h)?;
                        }
                        crate::menu::MenuResult::Launch(name) => {
                            config = Config::load();
                            frame_duration =
                                Duration::from_millis(1000 / config.general.fps.max(1) as u64);
                            rotate_interval = Duration::from_secs(config.rotate.interval_secs);
                            transition_style =
                                TransitionStyle::from_name(&config.rotate.transition_style);
                            transition_duration_ms = config.rotate.transition_duration_ms;
                            blank_delay = Duration::from_secs(config.blanking.delay_secs);

                            saver = create_plugin(&name, &config);
                            let size = terminal.size()?;
                            saver_w = size.width;
                            saver_h = size.height;
                            saver.init(saver_w, saver_h)?;
                        }
                        crate::menu::MenuResult::Quit => break 'outer,
                    }
                    last_frame = Instant::now();
                    blank_timer = Instant::now();
                    break; // Render immediately after menu
                }

                // +/= cycle to next screensaver, - cycle to previous
                if matches!(key_event.code, KeyCode::Char('+') | KeyCode::Char('=')) {
                    let plugins = registry::plugins();
                    if plugins.len() > 1 {
                        let current_name = saver.name();
                        let current_idx = plugins
                            .iter()
                            .position(|p| p.name == current_name)
                            .unwrap_or(0);
                        let next_idx = (current_idx + 1) % plugins.len();
                        // Capture old buffer for transition
                        if transition_style != TransitionStyle::None {
                            let buf = terminal.current_buffer_mut();
                            active_transition = Some(Transition::new(
                                transition_style,
                                transition_duration_ms,
                                buf,
                            ));
                        }
                        saver = create_plugin(&plugins[next_idx].name, &config);
                        let size = terminal.size()?;
                        saver_w = size.width;
                        saver_h = size.height;
                        saver.init(saver_w, saver_h)?;
                        // Pre-warm: ensure the subprocess has produced its first frame
                        // before the transition blends into it (avoids black flash).
                        let _ = saver.update(Duration::ZERO);
                        let _ = saver.render_to_buffer();
                        rotate_timer = Instant::now();
                        blank_timer = Instant::now();
                        osd_message = Some((plugins[next_idx].name.clone(), Instant::now()));
                    }
                    break; // Render immediately to show new screensaver
                }
                if matches!(key_event.code, KeyCode::Char('-')) {
                    let plugins = registry::plugins();
                    if plugins.len() > 1 {
                        let current_name = saver.name();
                        let current_idx = plugins
                            .iter()
                            .position(|p| p.name == current_name)
                            .unwrap_or(0);
                        let prev_idx = if current_idx == 0 {
                            plugins.len() - 1
                        } else {
                            current_idx - 1
                        };
                        if transition_style != TransitionStyle::None {
                            let buf = terminal.current_buffer_mut();
                            active_transition = Some(Transition::new(
                                transition_style,
                                transition_duration_ms,
                                buf,
                            ));
                        }
                        saver = create_plugin(&plugins[prev_idx].name, &config);
                        let size = terminal.size()?;
                        saver_w = size.width;
                        saver_h = size.height;
                        saver.init(saver_w, saver_h)?;
                        // Pre-warm: ensure the subprocess has produced its first frame
                        // before the transition blends into it (avoids black flash).
                        let _ = saver.update(Duration::ZERO);
                        let _ = saver.render_to_buffer();
                        rotate_timer = Instant::now();
                        blank_timer = Instant::now();
                        osd_message = Some((plugins[prev_idx].name.clone(), Instant::now()));
                    }
                    break; // Render immediately to show new screensaver
                }

                // Non-locked mode: Escape and Enter always exit
                if matches!(
                    key_event,
                    KeyEvent {
                        code: KeyCode::Esc,
                        ..
                    } | KeyEvent {
                        code: KeyCode::Enter,
                        ..
                    }
                ) {
                    break 'outer;
                }

                // Let the screensaver handle the event
                match saver.handle_event(&ev)? {
                    EventResponse::Exit => break 'outer,
                    EventResponse::Continue | EventResponse::Ignored => {}
                }
            }
        }

        // Calculate delta time
        let now = Instant::now();
        let dt = now - last_frame;
        last_frame = now;

        // Detect display sleep/wake: if more than 2 seconds elapsed since the
        // last frame, the display was probably asleep.  Force a full repaint
        // because ratatui's double-buffer is now out of sync with the screen.
        if dt > Duration::from_secs(2) {
            let _ = terminal.clear();
            let size = terminal.size()?;
            if size.width != saver_w || size.height != saver_h {
                saver_w = size.width;
                saver_h = size.height;
                saver.init(saver_w, saver_h)?;
            }
        }

        // Screen blanking
        if config.blanking.enabled && !blanked && blank_timer.elapsed() >= blank_delay {
            blanked = true;
            let _ = terminal.draw(|_frame| {});
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
        saver.update(effective_dt)?;

        // Update active transition
        if let Some(ref mut trans) = active_transition
            && !trans.update(dt.as_secs_f64())
        {
            active_transition = None;
        }

        // Auto-rotate
        if config.rotate.enabled && rotate_timer.elapsed() >= rotate_interval {
            rotate_timer = Instant::now();
            blank_timer = Instant::now();
            let rotate_list = &config.rotate.screensavers;
            if !rotate_list.is_empty() {
                rotate_index = (rotate_index + 1) % rotate_list.len();
                // Capture old buffer for transition
                if transition_style != TransitionStyle::None {
                    let buf = terminal.current_buffer_mut();
                    active_transition = Some(Transition::new(
                        transition_style,
                        transition_duration_ms,
                        buf,
                    ));
                }

                saver = create_plugin(&rotate_list[rotate_index], &config);
                let size = terminal.size()?;
                saver_w = size.width;
                saver_h = size.height;
                saver.init(saver_w, saver_h)?;
                // Pre-warm: ensure the subprocess has produced its first frame
                // before the transition blends into it (avoids black flash).
                let _ = saver.update(Duration::ZERO);
                let _ = saver.render_to_buffer();
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
        if let Some((_, t)) = &auth_message
            && t.elapsed() > Duration::from_secs(2)
        {
            auth_message = None;
        }

        // Clear expired OSD message
        if let Some((_, t)) = &osd_message
            && t.elapsed() > Duration::from_secs(2)
        {
            osd_message = None;
        }

        // Catch terminal size changes that bypass Event::Resize (e.g. ratatui autoresize).
        // Without this, the frame buffer can silently grow larger than the plugin buffer,
        // leaving the right/bottom edge cells unwritten and causing them to flash.
        if let Ok((term_w, term_h)) = crossterm::terminal::size()
            && (term_w != saver_w || term_h != saver_h)
        {
            saver_w = term_w;
            saver_h = term_h;
            saver.init(saver_w, saver_h)?;
        }

        // Render (synchronized output prevents tearing from partial frame writes).
        // Swallow transient IO errors from the terminal — they can happen after
        // display sleep/wake and are not fatal.
        let _ = execute!(terminal.backend_mut(), BeginSynchronizedUpdate);
        let pw_mode = password_mode;
        let pw_len = password_buffer.len();
        let auth_msg = auth_message.as_ref().map(|(m, _)| m.as_str());
        let lock_active = lock_first_key;
        let trans_ref = &active_transition;
        let ambient_ref = &ambient;
        let saver_ref = &mut saver;
        let filter_ref = &mut filter_state;
        let osd_ref = &osd_message;
        let _ = terminal.draw(|frame| {
            let _ = saver_ref.render_frame(frame);

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

            // Apply visual filters (after ambient, before overlays)
            filter_ref.apply(frame.buffer_mut(), &config.filter);

            overlay::render_clock(frame, &config.overlay);
            overlay::render_info(frame, &config.overlay);
            if let Some((name, started)) = osd_ref {
                overlay::render_osd(frame, name, started.elapsed());
            }
            if config.overlay.lock.enabled && lock_active && !pw_mode {
                overlay::render_lock(frame, &config.overlay.lock.message);
            }
            if pw_mode {
                overlay::render_password_prompt(frame, pw_len, auth_msg);
            }

            // Reset the bottom-right cell to prevent terminal scroll.
            // Writing a character at the last column of the last row causes the
            // terminal cursor to wrap past the screen, triggering a scroll.
            let area = frame.area();
            let last_x = area.width.saturating_sub(1);
            let last_y = area.height.saturating_sub(1);
            if let Some(c) = frame.buffer_mut().cell_mut((last_x, last_y)) {
                c.reset();
            }
        });
        let _ = execute!(terminal.backend_mut(), EndSynchronizedUpdate);
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
