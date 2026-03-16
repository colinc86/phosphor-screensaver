use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::config::Config;
use crate::registry;
use crate::theme::Theme;

pub enum MenuResult {
    Resume,
    Launch(String),
    Quit,
}

pub enum MenuContext {
    Standalone,
    DuringScreensaver { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Screensavers,
    Settings,
    Help,
    About,
}

const TABS: &[(Tab, &str)] = &[
    (Tab::Screensavers, "Screensavers"),
    (Tab::Settings, "Settings"),
    (Tab::Help, "Help"),
    (Tab::About, "About"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditKind {
    Bool,
    U32,
    U64,
    USize,
    F64,
    Text,
}

struct MenuUI {
    config: Config,
    context: MenuContext,
    tab: Tab,
    // Screensavers tab
    saver_selected: usize,
    saver_config_selected: usize,
    saver_config_editing: bool,
    saver_config_buffer: String,
    saver_focus_right: bool,
    // Settings tab
    setting_cursor: usize,
    setting_editing: bool,
    setting_buffer: String,
    // Help tab
    help_scroll: u16,
    // General
    message: Option<String>,
    result: Option<MenuResult>,
}

impl MenuUI {
    fn new(config: Config, context: MenuContext) -> Self {
        Self {
            config,
            context,
            tab: Tab::Screensavers,
            saver_selected: 0,
            saver_config_selected: 0,
            saver_config_editing: false,
            saver_config_buffer: String::new(),
            saver_focus_right: false,
            setting_cursor: 0,
            setting_editing: false,
            setting_buffer: String::new(),
            help_scroll: 0,
            message: None,
            result: None,
        }
    }

    fn next_tab(&mut self) {
        self.tab = match self.tab {
            Tab::Screensavers => Tab::Settings,
            Tab::Settings => Tab::Help,
            Tab::Help => Tab::About,
            Tab::About => Tab::Screensavers,
        };
    }

    fn prev_tab(&mut self) {
        self.tab = match self.tab {
            Tab::Screensavers => Tab::About,
            Tab::Settings => Tab::Screensavers,
            Tab::Help => Tab::Settings,
            Tab::About => Tab::Help,
        };
    }

    // --- Settings tab data ---

    fn setting_entries(&self) -> Vec<SettingEntry> {
        let c = &self.config;
        let saver_names: Vec<String> = registry::SCREENSAVERS.iter().map(|s| s.name.to_string()).collect();
        let theme_names: Vec<String> = Theme::available().iter().map(|s| s.to_string()).collect();
        let positions = vec!["top-left".into(), "top-right".into(), "bottom-left".into(), "bottom-right".into()];
        let transitions = vec!["dissolve".into(), "slide".into(), "melt".into(), "none".into()];

        vec![
            SettingEntry::Header("General"),
            SettingEntry::Setting { label: "Timeout (seconds)", value: c.general.timeout.to_string(), kind: EditKind::U64, choices: None },
            SettingEntry::Setting { label: "FPS", value: c.general.fps.to_string(), kind: EditKind::U32, choices: None },
            SettingEntry::Setting { label: "Default Screensaver", value: c.general.default_screensaver.clone(), kind: EditKind::Text, choices: Some(saver_names.clone()) },
            SettingEntry::Setting { label: "Fade Enabled", value: c.general.fade_enabled.to_string(), kind: EditKind::Bool, choices: None },
            SettingEntry::Setting { label: "Fade Duration (ms)", value: c.general.fade_duration_ms.to_string(), kind: EditKind::U64, choices: None },
            SettingEntry::Setting { label: "Ambient Enabled", value: c.general.ambient_enabled.to_string(), kind: EditKind::Bool, choices: None },
            SettingEntry::Setting { label: "Ambient Intensity", value: format!("{:.2}", c.general.ambient_intensity), kind: EditKind::F64, choices: None },

            SettingEntry::Header("Clock Overlay"),
            SettingEntry::Setting { label: "Enabled", value: c.overlay.clock.enabled.to_string(), kind: EditKind::Bool, choices: None },
            SettingEntry::Setting { label: "Position", value: c.overlay.clock.position.clone(), kind: EditKind::Text, choices: Some(positions) },
            SettingEntry::Setting { label: "Format", value: c.overlay.clock.format.clone(), kind: EditKind::Text, choices: None },
            SettingEntry::Setting { label: "Show Seconds", value: c.overlay.clock.show_seconds.to_string(), kind: EditKind::Bool, choices: None },

            SettingEntry::Header("Lock"),
            SettingEntry::Setting { label: "Enabled", value: c.overlay.lock.enabled.to_string(), kind: EditKind::Bool, choices: None },
            SettingEntry::Setting { label: "Message", value: c.overlay.lock.message.clone(), kind: EditKind::Text, choices: None },
            SettingEntry::Setting { label: "Password Required", value: c.overlay.lock.password_required.to_string(), kind: EditKind::Bool, choices: None },

            SettingEntry::Header("Screen Blanking"),
            SettingEntry::Setting { label: "Enabled", value: c.blanking.enabled.to_string(), kind: EditKind::Bool, choices: None },
            SettingEntry::Setting { label: "Delay (seconds)", value: c.blanking.delay_secs.to_string(), kind: EditKind::U64, choices: None },

            SettingEntry::Header("Auto-Rotate"),
            SettingEntry::Setting { label: "Enabled", value: c.rotate.enabled.to_string(), kind: EditKind::Bool, choices: None },
            SettingEntry::Setting { label: "Interval (seconds)", value: c.rotate.interval_secs.to_string(), kind: EditKind::U64, choices: None },
            SettingEntry::Setting { label: "Transition Style", value: c.rotate.transition_style.clone(), kind: EditKind::Text, choices: Some(transitions) },
            SettingEntry::Setting { label: "Transition Duration (ms)", value: c.rotate.transition_duration_ms.to_string(), kind: EditKind::U64, choices: None },

            SettingEntry::Header("Power"),
            SettingEntry::Setting { label: "Battery Aware", value: c.power.battery_aware.to_string(), kind: EditKind::Bool, choices: None },
            SettingEntry::Setting { label: "Battery FPS", value: c.power.battery_fps.to_string(), kind: EditKind::U32, choices: None },
            SettingEntry::Setting { label: "AC FPS", value: c.power.ac_fps.to_string(), kind: EditKind::U32, choices: None },

            SettingEntry::Header("Schedule"),
            SettingEntry::Setting { label: "Enabled", value: c.schedule.enabled.to_string(), kind: EditKind::Bool, choices: None },
            SettingEntry::Setting { label: "Morning (6am-12pm)", value: c.schedule.morning.clone(), kind: EditKind::Text, choices: Some(saver_names.clone()) },
            SettingEntry::Setting { label: "Afternoon (12pm-6pm)", value: c.schedule.afternoon.clone(), kind: EditKind::Text, choices: Some(saver_names.clone()) },
            SettingEntry::Setting { label: "Evening (6pm-10pm)", value: c.schedule.evening.clone(), kind: EditKind::Text, choices: Some(saver_names.clone()) },
            SettingEntry::Setting { label: "Night (10pm-6am)", value: c.schedule.night.clone(), kind: EditKind::Text, choices: Some(saver_names) },

            SettingEntry::Header("Theme"),
            SettingEntry::Setting { label: "Name", value: c.theme.name.clone(), kind: EditKind::Text, choices: Some(theme_names) },
        ]
    }

    /// Map flat setting index (skipping headers) to config field and apply value.
    fn apply_setting(&mut self, flat_idx: usize, value: &str) {
        let c = &mut self.config;
        match flat_idx {
            // General
            0 => { if let Ok(v) = value.parse() { c.general.timeout = v; } }
            1 => { if let Ok(v) = value.parse() { c.general.fps = v; } }
            2 => c.general.default_screensaver = value.to_string(),
            3 => c.general.fade_enabled = !c.general.fade_enabled,
            4 => { if let Ok(v) = value.parse() { c.general.fade_duration_ms = v; } }
            5 => c.general.ambient_enabled = !c.general.ambient_enabled,
            6 => { if let Ok(v) = value.parse() { c.general.ambient_intensity = v; } }
            // Clock Overlay
            7 => c.overlay.clock.enabled = !c.overlay.clock.enabled,
            8 => c.overlay.clock.position = value.to_string(),
            9 => c.overlay.clock.format = value.to_string(),
            10 => c.overlay.clock.show_seconds = !c.overlay.clock.show_seconds,
            // Lock
            11 => c.overlay.lock.enabled = !c.overlay.lock.enabled,
            12 => c.overlay.lock.message = value.to_string(),
            13 => c.overlay.lock.password_required = !c.overlay.lock.password_required,
            // Screen Blanking
            14 => c.blanking.enabled = !c.blanking.enabled,
            15 => { if let Ok(v) = value.parse() { c.blanking.delay_secs = v; } }
            // Auto-Rotate
            16 => c.rotate.enabled = !c.rotate.enabled,
            17 => { if let Ok(v) = value.parse() { c.rotate.interval_secs = v; } }
            18 => c.rotate.transition_style = value.to_string(),
            19 => { if let Ok(v) = value.parse() { c.rotate.transition_duration_ms = v; } }
            // Power
            20 => c.power.battery_aware = !c.power.battery_aware,
            21 => { if let Ok(v) = value.parse() { c.power.battery_fps = v; } }
            22 => { if let Ok(v) = value.parse() { c.power.ac_fps = v; } }
            // Schedule
            23 => c.schedule.enabled = !c.schedule.enabled,
            24 => c.schedule.morning = value.to_string(),
            25 => c.schedule.afternoon = value.to_string(),
            26 => c.schedule.evening = value.to_string(),
            27 => c.schedule.night = value.to_string(),
            // Theme
            28 => c.theme.name = value.to_string(),
            _ => {}
        }
    }

    /// Get the flat setting index and entry for the current cursor position.
    fn setting_at_cursor(&self) -> Option<(usize, SettingEntry)> {
        let entries = self.setting_entries();
        let mut flat_idx = 0usize;
        let mut cursor = 0usize;
        for entry in &entries {
            match entry {
                SettingEntry::Header(_) => {
                    if cursor == self.setting_cursor {
                        return None; // cursor on header
                    }
                    cursor += 1;
                }
                SettingEntry::Setting { .. } => {
                    if cursor == self.setting_cursor {
                        return Some((flat_idx, entry.clone()));
                    }
                    cursor += 1;
                    flat_idx += 1;
                }
            }
        }
        None
    }

    fn setting_entry_count(&self) -> usize {
        self.setting_entries().len()
    }

    /// Move setting cursor, skipping headers.
    fn setting_move(&mut self, delta: isize) {
        let count = self.setting_entry_count();
        if count == 0 { return; }
        let entries = self.setting_entries();
        let mut pos = self.setting_cursor as isize;
        loop {
            pos += delta;
            if pos < 0 { pos = count as isize - 1; }
            if pos >= count as isize { pos = 0; }
            if matches!(entries[pos as usize], SettingEntry::Setting { .. }) {
                break;
            }
            // Safety: if all are headers (impossible), break
            if pos as usize == self.setting_cursor { break; }
        }
        self.setting_cursor = pos as usize;
    }

    // --- Screensaver config items ---

    fn saver_config_items(&self) -> Vec<(String, String, EditKind, Option<Vec<String>>)> {
        let name = self.selected_saver_name();
        let c = &self.config.screensavers;
        match name {
            "bouncing_ball" => vec![
                ("Trail Length".into(), c.bouncing_ball.trail_length.to_string(), EditKind::USize, None),
                ("Ball Character".into(), c.bouncing_ball.ball_char.clone(), EditKind::Text, None),
                ("Color".into(), c.bouncing_ball.color.clone(), EditKind::Text, None),
                ("Ball Count".into(), c.bouncing_ball.ball_count.to_string(), EditKind::USize, None),
                ("Gravity".into(), c.bouncing_ball.gravity.clone(), EditKind::Text, Some(vec!["none".into(), "down".into(), "center".into(), "wave".into()])),
                ("Trail Style".into(), c.bouncing_ball.trail_style.clone(), EditKind::Text, Some(vec!["dots".into(), "fire".into(), "rainbow".into()])),
                ("Collisions".into(), c.bouncing_ball.collisions.to_string(), EditKind::Bool, None),
            ],
            "starfield" => vec![
                ("Star Count".into(), c.starfield.star_count.to_string(), EditKind::USize, None),
                ("Speed".into(), format!("{:.1}", c.starfield.speed), EditKind::F64, None),
                ("Streaks Enabled".into(), c.starfield.streak_enabled.to_string(), EditKind::Bool, None),
                ("Warp Enabled".into(), c.starfield.warp_enabled.to_string(), EditKind::Bool, None),
            ],
            "matrix_rain" => vec![
                ("Density".into(), format!("{:.2}", c.matrix_rain.density), EditKind::F64, None),
                ("Speed".into(), format!("{:.1}", c.matrix_rain.speed), EditKind::F64, None),
                ("Charset".into(), c.matrix_rain.charset.clone(), EditKind::Text, Some(vec!["katakana".into(), "ascii".into(), "binary".into()])),
                ("Depth Layers".into(), c.matrix_rain.depth_layers.to_string(), EditKind::Bool, None),
                ("Glitch Wave".into(), c.matrix_rain.glitch_wave.to_string(), EditKind::Bool, None),
                ("Glitch Interval (s)".into(), c.matrix_rain.glitch_interval_secs.to_string(), EditKind::U64, None),
                ("Interactive".into(), c.matrix_rain.interactive.to_string(), EditKind::Bool, None),
            ],
            "pipes" => vec![
                ("Palette".into(), c.pipes.palette.clone(), EditKind::Text, Some(vec!["rainbow".into(), "theme".into(), "mono".into()])),
                ("Turn Probability".into(), format!("{:.2}", c.pipes.turn_probability), EditKind::F64, None),
                ("Junctions".into(), c.pipes.junctions.to_string(), EditKind::Bool, None),
                ("Flow Enabled".into(), c.pipes.flow_enabled.to_string(), EditKind::Bool, None),
                ("Flow Speed".into(), format!("{:.1}", c.pipes.flow_speed), EditKind::F64, None),
            ],
            "digital_clock" => vec![
                ("Style".into(), c.digital_clock.style.clone(), EditKind::Text, Some(vec!["block".into(), "thin".into(), "segment".into()])),
                ("Show Date".into(), c.digital_clock.show_date.to_string(), EditKind::Bool, None),
                ("Color".into(), c.digital_clock.color.clone(), EditKind::Text, None),
                ("Color Cycle".into(), c.digital_clock.color_cycle.to_string(), EditKind::Bool, None),
                ("Color Cycle Speed".into(), format!("{:.1}", c.digital_clock.color_cycle_speed), EditKind::F64, None),
                ("Mode".into(), c.digital_clock.mode.clone(), EditKind::Text, Some(vec!["clock".into(), "stopwatch".into(), "countdown".into()])),
                ("Countdown Minutes".into(), c.digital_clock.countdown_minutes.to_string(), EditKind::U64, None),
            ],
            "game_of_life" => vec![
                ("Initial Density".into(), format!("{:.2}", c.game_of_life.initial_density), EditKind::F64, None),
                ("Cell Character".into(), c.game_of_life.cell_char.clone(), EditKind::Text, None),
                ("Auto Reset (secs)".into(), c.game_of_life.auto_reset_secs.to_string(), EditKind::U64, None),
                ("Color By Age".into(), c.game_of_life.color_by_age.to_string(), EditKind::Bool, None),
            ],
            "analog_clock" => vec![
                ("Show Seconds".into(), c.analog_clock.show_seconds.to_string(), EditKind::Bool, None),
                ("Show Date".into(), c.analog_clock.show_date.to_string(), EditKind::Bool, None),
                ("Color".into(), c.analog_clock.color.clone(), EditKind::Text, None),
                ("Timezone".into(), c.analog_clock.timezone.clone(), EditKind::Text, None),
            ],
            "lissajous" => vec![
                ("Speed".into(), format!("{:.1}", c.lissajous.speed), EditKind::F64, None),
                ("Trail Length".into(), c.lissajous.trail_length.to_string(), EditKind::USize, None),
                ("Damping".into(), format!("{:.4}", c.lissajous.damping), EditKind::F64, None),
                ("Color Mode".into(), c.lissajous.color_mode.clone(), EditKind::Text, Some(vec!["rainbow".into(), "theme".into()])),
                ("Morph Interval (secs)".into(), format!("{:.1}", c.lissajous.morph_interval_secs), EditKind::F64, None),
            ],
            "fluid" => vec![
                ("Viscosity".into(), format!("{:.5}", c.fluid.viscosity), EditKind::F64, None),
                ("Diffusion".into(), format!("{:.6}", c.fluid.diffusion), EditKind::F64, None),
                ("Force Strength".into(), format!("{:.1}", c.fluid.force_strength), EditKind::F64, None),
                ("Interactive".into(), c.fluid.interactive.to_string(), EditKind::Bool, None),
                ("Color Mode".into(), c.fluid.color_mode.clone(), EditKind::Text, Some(vec!["rainbow".into(), "theme".into()])),
                ("Solver Iterations".into(), c.fluid.solver_iterations.to_string(), EditKind::USize, None),
            ],
            "terrain" => vec![
                ("Speed".into(), format!("{:.1}", c.terrain.speed), EditKind::F64, None),
                ("Altitude".into(), format!("{:.1}", c.terrain.altitude), EditKind::F64, None),
                ("Draw Distance".into(), format!("{:.1}", c.terrain.draw_distance), EditKind::F64, None),
                ("Palette".into(), c.terrain.palette.clone(), EditKind::Text, Some(vec!["earth".into(), "alien".into(), "desert".into()])),
                ("Fog".into(), c.terrain.fog.to_string(), EditKind::Bool, None),
            ],
            "aurora" => vec![
                ("Ribbon Count".into(), c.aurora.ribbon_count.to_string(), EditKind::USize, None),
                ("Speed".into(), format!("{:.1}", c.aurora.speed), EditKind::F64, None),
                ("Intensity".into(), format!("{:.1}", c.aurora.intensity), EditKind::F64, None),
                ("Palette".into(), c.aurora.palette.clone(), EditKind::Text, Some(vec!["classic".into(), "warm".into(), "mono".into()])),
            ],
            "plasma" => vec![
                ("Speed".into(), format!("{:.1}", c.plasma.speed), EditKind::F64, None),
                ("Scale".into(), format!("{:.1}", c.plasma.scale), EditKind::F64, None),
                ("Mode".into(), c.plasma.mode.clone(), EditKind::Text, Some(vec!["fill".into(), "braille".into()])),
                ("Color Mode".into(), c.plasma.color_mode.clone(), EditKind::Text, Some(vec!["rainbow".into(), "fire".into(), "ocean".into(), "mono".into()])),
            ],
            "maze" => vec![
                ("Generation Speed".into(), format!("{:.1}", c.maze.generation_speed), EditKind::F64, None),
                ("Solve Speed".into(), format!("{:.1}", c.maze.solve_speed), EditKind::F64, None),
                ("Wall Style".into(), c.maze.wall_style.clone(), EditKind::Text, Some(vec!["box".into(), "thick".into(), "ascii".into()])),
                ("Show Exploration".into(), c.maze.show_exploration.to_string(), EditKind::Bool, None),
            ],
            _ => vec![],
        }
    }

    fn apply_saver_config(&mut self, idx: usize, value: &str) {
        let name = self.selected_saver_name().to_string();
        let c = &mut self.config.screensavers;
        match name.as_str() {
            "bouncing_ball" => match idx {
                0 => { if let Ok(v) = value.parse() { c.bouncing_ball.trail_length = v; } }
                1 => c.bouncing_ball.ball_char = value.to_string(),
                2 => c.bouncing_ball.color = value.to_string(),
                3 => { if let Ok(v) = value.parse::<usize>() { c.bouncing_ball.ball_count = v.max(1); } }
                4 => c.bouncing_ball.gravity = value.to_string(),
                5 => c.bouncing_ball.trail_style = value.to_string(),
                6 => { if let Ok(v) = value.parse() { c.bouncing_ball.collisions = v; } }
                _ => {}
            },
            "starfield" => match idx {
                0 => { if let Ok(v) = value.parse() { c.starfield.star_count = v; } }
                1 => { if let Ok(v) = value.parse() { c.starfield.speed = v; } }
                2 => c.starfield.streak_enabled = !c.starfield.streak_enabled,
                3 => c.starfield.warp_enabled = !c.starfield.warp_enabled,
                _ => {}
            },
            "matrix_rain" => match idx {
                0 => { if let Ok(v) = value.parse() { c.matrix_rain.density = v; } }
                1 => { if let Ok(v) = value.parse() { c.matrix_rain.speed = v; } }
                2 => c.matrix_rain.charset = value.to_string(),
                3 => c.matrix_rain.depth_layers = !c.matrix_rain.depth_layers,
                4 => c.matrix_rain.glitch_wave = !c.matrix_rain.glitch_wave,
                5 => { if let Ok(v) = value.parse() { c.matrix_rain.glitch_interval_secs = v; } }
                6 => c.matrix_rain.interactive = !c.matrix_rain.interactive,
                _ => {}
            },
            "pipes" => match idx {
                0 => c.pipes.palette = value.to_string(),
                1 => { if let Ok(v) = value.parse() { c.pipes.turn_probability = v; } }
                2 => c.pipes.junctions = !c.pipes.junctions,
                3 => c.pipes.flow_enabled = !c.pipes.flow_enabled,
                4 => { if let Ok(v) = value.parse() { c.pipes.flow_speed = v; } }
                _ => {}
            },
            "digital_clock" => match idx {
                0 => c.digital_clock.style = value.to_string(),
                1 => c.digital_clock.show_date = !c.digital_clock.show_date,
                2 => c.digital_clock.color = value.to_string(),
                3 => c.digital_clock.color_cycle = !c.digital_clock.color_cycle,
                4 => { if let Ok(v) = value.parse() { c.digital_clock.color_cycle_speed = v; } }
                5 => c.digital_clock.mode = value.to_string(),
                6 => { if let Ok(v) = value.parse() { c.digital_clock.countdown_minutes = v; } }
                _ => {}
            },
            "game_of_life" => match idx {
                0 => { if let Ok(v) = value.parse() { c.game_of_life.initial_density = v; } }
                1 => c.game_of_life.cell_char = value.to_string(),
                2 => { if let Ok(v) = value.parse() { c.game_of_life.auto_reset_secs = v; } }
                3 => c.game_of_life.color_by_age = !c.game_of_life.color_by_age,
                _ => {}
            },
            "analog_clock" => match idx {
                0 => c.analog_clock.show_seconds = !c.analog_clock.show_seconds,
                1 => c.analog_clock.show_date = !c.analog_clock.show_date,
                2 => c.analog_clock.color = value.to_string(),
                3 => c.analog_clock.timezone = value.to_string(),
                _ => {}
            },
            "lissajous" => match idx {
                0 => { if let Ok(v) = value.parse() { c.lissajous.speed = v; } }
                1 => { if let Ok(v) = value.parse() { c.lissajous.trail_length = v; } }
                2 => { if let Ok(v) = value.parse() { c.lissajous.damping = v; } }
                3 => c.lissajous.color_mode = value.to_string(),
                4 => { if let Ok(v) = value.parse() { c.lissajous.morph_interval_secs = v; } }
                _ => {}
            },
            "fluid" => match idx {
                0 => { if let Ok(v) = value.parse() { c.fluid.viscosity = v; } }
                1 => { if let Ok(v) = value.parse() { c.fluid.diffusion = v; } }
                2 => { if let Ok(v) = value.parse() { c.fluid.force_strength = v; } }
                3 => c.fluid.interactive = !c.fluid.interactive,
                4 => c.fluid.color_mode = value.to_string(),
                5 => { if let Ok(v) = value.parse() { c.fluid.solver_iterations = v; } }
                _ => {}
            },
            "terrain" => match idx {
                0 => { if let Ok(v) = value.parse() { c.terrain.speed = v; } }
                1 => { if let Ok(v) = value.parse() { c.terrain.altitude = v; } }
                2 => { if let Ok(v) = value.parse() { c.terrain.draw_distance = v; } }
                3 => c.terrain.palette = value.to_string(),
                4 => c.terrain.fog = !c.terrain.fog,
                _ => {}
            },
            "aurora" => match idx {
                0 => { if let Ok(v) = value.parse() { c.aurora.ribbon_count = v; } }
                1 => { if let Ok(v) = value.parse() { c.aurora.speed = v; } }
                2 => { if let Ok(v) = value.parse() { c.aurora.intensity = v; } }
                3 => c.aurora.palette = value.to_string(),
                _ => {}
            },
            "plasma" => match idx {
                0 => { if let Ok(v) = value.parse() { c.plasma.speed = v; } }
                1 => { if let Ok(v) = value.parse() { c.plasma.scale = v; } }
                2 => c.plasma.mode = value.to_string(),
                3 => c.plasma.color_mode = value.to_string(),
                _ => {}
            },
            "maze" => match idx {
                0 => { if let Ok(v) = value.parse() { c.maze.generation_speed = v; } }
                1 => { if let Ok(v) = value.parse() { c.maze.solve_speed = v; } }
                2 => c.maze.wall_style = value.to_string(),
                3 => c.maze.show_exploration = !c.maze.show_exploration,
                _ => {}
            },
            _ => {}
        }
    }

    fn selected_saver_name(&self) -> &str {
        let savers: Vec<&registry::ScreensaverInfo> = registry::SCREENSAVERS.iter().filter(|s| s.name != "random").collect();
        savers.get(self.saver_selected).map(|s| s.name).unwrap_or("starfield")
    }

    fn real_savers() -> Vec<&'static registry::ScreensaverInfo> {
        registry::SCREENSAVERS.iter().filter(|s| s.name != "random").collect()
    }
}

#[derive(Clone)]
enum SettingEntry {
    Header(&'static str),
    Setting {
        label: &'static str,
        value: String,
        kind: EditKind,
        choices: Option<Vec<String>>,
    },
}

pub fn run(config: &Config, context: MenuContext) -> io::Result<MenuResult> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::cursor::Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut ui = MenuUI::new(config.clone(), context);

    // Initialize setting_cursor to first non-header
    ui.setting_move(1);

    loop {
        terminal.draw(|frame| render(&ui, frame))?;

        if let Some(_) = &ui.result {
            break;
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? {
                ui.message = None;
                handle_input(&mut ui, code, modifiers, &mut terminal)?;
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

    Ok(ui.result.unwrap_or(MenuResult::Quit))
}

fn handle_input(
    ui: &mut MenuUI,
    code: KeyCode,
    modifiers: KeyModifiers,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> io::Result<()> {
    // Handle edit mode first
    if ui.setting_editing || ui.saver_config_editing {
        let buf = if ui.setting_editing { &mut ui.setting_buffer } else { &mut ui.saver_config_buffer };
        match code {
            KeyCode::Enter => {
                let val = buf.clone();
                if ui.setting_editing {
                    if let Some((flat_idx, _)) = ui.setting_at_cursor() {
                        ui.apply_setting(flat_idx, &val);
                    }
                    ui.setting_editing = false;
                } else {
                    ui.apply_saver_config(ui.saver_config_selected, &val);
                    ui.saver_config_editing = false;
                }
            }
            KeyCode::Esc => {
                ui.setting_editing = false;
                ui.saver_config_editing = false;
            }
            KeyCode::Backspace => { buf.pop(); }
            KeyCode::Char(c) => buf.push(c),
            _ => {}
        }
        return Ok(());
    }

    // Global keys
    match code {
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
            ui.result = Some(MenuResult::Quit);
            return Ok(());
        }
        _ => {}
    }

    match ui.tab {
        Tab::Screensavers => handle_screensavers_input(ui, code, terminal),
        Tab::Settings => handle_settings_input(ui, code),
        Tab::Help => handle_help_input(ui, code),
        Tab::About => handle_about_input(ui, code),
    }
}

fn handle_screensavers_input(
    ui: &mut MenuUI,
    code: KeyCode,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> io::Result<()> {
    let saver_count = MenuUI::real_savers().len();
    match code {
        KeyCode::Esc | KeyCode::Char('q') => {
            if ui.saver_focus_right {
                ui.saver_focus_right = false;
            } else {
                ui.result = Some(match &ui.context {
                    MenuContext::DuringScreensaver { .. } => MenuResult::Resume,
                    MenuContext::Standalone => MenuResult::Quit,
                });
            }
        }
        KeyCode::Tab => ui.next_tab(),
        KeyCode::BackTab => ui.prev_tab(),
        KeyCode::Up | KeyCode::Char('k') => {
            if ui.saver_focus_right {
                let items = ui.saver_config_items();
                if !items.is_empty() {
                    ui.saver_config_selected = ui.saver_config_selected.checked_sub(1).unwrap_or(items.len() - 1);
                }
            } else if saver_count > 0 {
                ui.saver_selected = ui.saver_selected.checked_sub(1).unwrap_or(saver_count - 1);
                ui.saver_config_selected = 0;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if ui.saver_focus_right {
                let items = ui.saver_config_items();
                if !items.is_empty() {
                    ui.saver_config_selected = (ui.saver_config_selected + 1) % items.len();
                }
            } else if saver_count > 0 {
                ui.saver_selected = (ui.saver_selected + 1) % saver_count;
                ui.saver_config_selected = 0;
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            ui.saver_focus_right = true;
        }
        KeyCode::Left | KeyCode::Char('h') if ui.saver_focus_right => {
            ui.saver_focus_right = false;
        }
        KeyCode::Enter => {
            if ui.saver_focus_right {
                // Edit selected saver config item
                let items = ui.saver_config_items();
                if let Some((_, value, kind, choices)) = items.get(ui.saver_config_selected) {
                    if *kind == EditKind::Bool {
                        ui.apply_saver_config(ui.saver_config_selected, "");
                    } else if let Some(opts) = choices {
                        // Cycle through choices
                        let current = value.as_str();
                        let next_idx = opts.iter().position(|o| o == current).map(|i| (i + 1) % opts.len()).unwrap_or(0);
                        ui.apply_saver_config(ui.saver_config_selected, &opts[next_idx]);
                    } else {
                        ui.saver_config_buffer = value.clone();
                        ui.saver_config_editing = true;
                    }
                }
            } else {
                // Launch selected screensaver
                let name = ui.selected_saver_name().to_string();
                ui.result = Some(MenuResult::Launch(name));
            }
        }
        KeyCode::Char('d') if !ui.saver_focus_right => {
            let name = ui.selected_saver_name().to_string();
            ui.config.general.default_screensaver = name.clone();
            ui.message = Some(format!("Default set to: {name}"));
        }
        KeyCode::Char('p') if !ui.saver_focus_right => {
            // Preview
            let name = ui.selected_saver_name().to_string();
            disable_raw_mode()?;
            execute!(terminal.backend_mut(), LeaveAlternateScreen, crossterm::cursor::Show)?;

            let mut preview_config = ui.config.clone();
            preview_config.general.default_screensaver = name;
            preview_config.general.fade_enabled = false;
            let _ = crate::app::run(&preview_config, Some(Duration::from_secs(5)));

            enable_raw_mode()?;
            execute!(io::stdout(), EnterAlternateScreen, crossterm::cursor::Hide)?;
            *terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
            ui.message = Some("Preview finished".into());
        }
        KeyCode::Char('s') => {
            save_config(ui);
        }
        KeyCode::Char('r') => {
            if matches!(ui.context, MenuContext::DuringScreensaver { .. }) {
                ui.result = Some(MenuResult::Resume);
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_settings_input(ui: &mut MenuUI, code: KeyCode) -> io::Result<()> {
    match code {
        KeyCode::Esc | KeyCode::Char('q') => {
            ui.result = Some(match &ui.context {
                MenuContext::DuringScreensaver { .. } => MenuResult::Resume,
                MenuContext::Standalone => MenuResult::Quit,
            });
        }
        KeyCode::Tab => ui.next_tab(),
        KeyCode::BackTab => ui.prev_tab(),
        KeyCode::Up | KeyCode::Char('k') => ui.setting_move(-1),
        KeyCode::Down | KeyCode::Char('j') => ui.setting_move(1),
        KeyCode::Enter => {
            if let Some((flat_idx, entry)) = ui.setting_at_cursor() {
                if let SettingEntry::Setting { kind, value, choices, .. } = entry {
                    match kind {
                        EditKind::Bool => {
                            ui.apply_setting(flat_idx, "");
                        }
                        _ => {
                            if let Some(opts) = choices {
                                // Cycle through choices
                                let next_idx = opts.iter().position(|o| o == &value).map(|i| (i + 1) % opts.len()).unwrap_or(0);
                                ui.apply_setting(flat_idx, &opts[next_idx]);
                            } else {
                                ui.setting_buffer = value;
                                ui.setting_editing = true;
                            }
                        }
                    }
                }
            }
        }
        KeyCode::Char('s') => {
            save_config(ui);
        }
        _ => {}
    }
    Ok(())
}

fn handle_help_input(ui: &mut MenuUI, code: KeyCode) -> io::Result<()> {
    match code {
        KeyCode::Esc | KeyCode::Char('q') => {
            ui.result = Some(match &ui.context {
                MenuContext::DuringScreensaver { .. } => MenuResult::Resume,
                MenuContext::Standalone => MenuResult::Quit,
            });
        }
        KeyCode::Tab => ui.next_tab(),
        KeyCode::BackTab => ui.prev_tab(),
        KeyCode::Up | KeyCode::Char('k') => {
            ui.help_scroll = ui.help_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            ui.help_scroll = ui.help_scroll.saturating_add(1);
        }
        _ => {}
    }
    Ok(())
}

fn handle_about_input(ui: &mut MenuUI, code: KeyCode) -> io::Result<()> {
    match code {
        KeyCode::Esc | KeyCode::Char('q') => {
            ui.result = Some(match &ui.context {
                MenuContext::DuringScreensaver { .. } => MenuResult::Resume,
                MenuContext::Standalone => MenuResult::Quit,
            });
        }
        KeyCode::Tab => ui.next_tab(),
        KeyCode::BackTab => ui.prev_tab(),
        _ => {}
    }
    Ok(())
}

fn save_config(ui: &mut MenuUI) {
    match ui.config.save() {
        Ok(()) => ui.message = Some("Configuration saved!".into()),
        Err(e) => ui.message = Some(format!("Save failed: {e}")),
    }
}

// --- Rendering ---

fn render(ui: &MenuUI, frame: &mut ratatui::Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title + tab bar
            Constraint::Min(10),  // Content
            Constraint::Length(3), // Status
        ])
        .split(frame.area());

    render_tab_bar(ui, frame, chunks[0]);

    match ui.tab {
        Tab::Screensavers => render_screensavers(ui, frame, chunks[1]),
        Tab::Settings => render_settings(ui, frame, chunks[1]),
        Tab::Help => render_help(ui, frame, chunks[1]),
        Tab::About => render_about(frame, chunks[1]),
    }

    render_status_bar(ui, frame, chunks[2]);
}

fn render_tab_bar(ui: &MenuUI, frame: &mut ratatui::Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(2)])
        .split(area);

    let title = Paragraph::new("  Termsaver")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    frame.render_widget(title, chunks[0]);

    let tabs: Vec<Span> = TABS.iter().map(|(tab, name)| {
        if *tab == ui.tab {
            Span::styled(format!(" [{name}] "), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        } else {
            Span::styled(format!("  {name}  "), Style::default().fg(Color::DarkGray))
        }
    }).collect();
    let tab_line = Paragraph::new(Line::from(tabs));
    frame.render_widget(tab_line, chunks[1]);
}

fn render_screensavers(ui: &MenuUI, frame: &mut ratatui::Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    // Left panel: screensaver list
    let savers = MenuUI::real_savers();
    let running_name = match &ui.context {
        MenuContext::DuringScreensaver { name } => Some(name.as_str()),
        _ => None,
    };
    let list_items: Vec<ListItem> = savers.iter().enumerate().map(|(i, info)| {
        let is_default = info.name == ui.config.general.default_screensaver;
        let is_running = running_name == Some(info.name);
        let marker = if is_running { " ▶" } else if is_default { " *" } else { "" };
        let style = if i == ui.saver_selected && !ui.saver_focus_right {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if i == ui.saver_selected {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        ListItem::new(format!("  {}{marker}", info.name)).style(style)
    }).collect();

    let mut list_state = ListState::default();
    list_state.select(Some(ui.saver_selected));
    let list = List::new(list_items)
        .block(Block::default().borders(Borders::ALL).title("Screensavers")
            .border_style(if !ui.saver_focus_right { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::DarkGray) }));
    frame.render_stateful_widget(list, chunks[0], &mut list_state);

    // Right panel: detail + config
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(6)])
        .split(chunks[1]);

    // Description
    let info = savers.get(ui.saver_selected);
    let desc = info.map(|i| i.description).unwrap_or("");
    let desc_widget = Paragraph::new(format!("  {desc}"))
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Description"));
    frame.render_widget(desc_widget, right_chunks[0]);

    // Per-screensaver config
    let config_items = ui.saver_config_items();
    let config_list_items: Vec<ListItem> = config_items.iter().enumerate().map(|(i, (label, value, kind, _choices))| {
        let display = if ui.saver_config_editing && i == ui.saver_config_selected {
            format!("  {label}: {}_", ui.saver_config_buffer)
        } else {
            let val_display = if *kind == EditKind::Bool {
                if value == "true" { "yes" } else { "no" }
            } else {
                value.as_str()
            };
            format!("  {label}: {val_display}")
        };
        let style = if i == ui.saver_config_selected && ui.saver_focus_right {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        ListItem::new(display).style(style)
    }).collect();

    let mut config_state = ListState::default();
    config_state.select(Some(ui.saver_config_selected));
    let config_list = List::new(config_list_items)
        .block(Block::default().borders(Borders::ALL).title("Configuration")
            .border_style(if ui.saver_focus_right { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::DarkGray) }));
    frame.render_stateful_widget(config_list, right_chunks[1], &mut config_state);
}

fn render_settings(ui: &MenuUI, frame: &mut ratatui::Frame, area: Rect) {
    let entries = ui.setting_entries();
    let list_items: Vec<ListItem> = entries.iter().enumerate().map(|(i, entry)| {
        match entry {
            SettingEntry::Header(name) => {
                ListItem::new(format!("  ── {name} ──"))
                    .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            }
            SettingEntry::Setting { label, value, kind, .. } => {
                let display = if ui.setting_editing && i == ui.setting_cursor {
                    format!("    {label}: {}_", ui.setting_buffer)
                } else {
                    let val_display = if *kind == EditKind::Bool {
                        if value == "true" { "yes" } else { "no" }
                    } else {
                        value.as_str()
                    };
                    format!("    {label}: {val_display}")
                };
                let style = if i == ui.setting_cursor {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(display).style(style)
            }
        }
    }).collect();

    let mut list_state = ListState::default();
    list_state.select(Some(ui.setting_cursor));
    let list = List::new(list_items)
        .block(Block::default().borders(Borders::ALL).title("Settings")
            .border_style(Style::default().fg(Color::Cyan)));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_help(ui: &MenuUI, frame: &mut ratatui::Frame, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    // Global controls
    lines.push(Line::from(Span::styled("  Global Controls", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))));
    lines.push(Line::from(""));
    lines.push(Line::from("    q/Esc        Exit screensaver"));
    lines.push(Line::from("    m            Open this menu (during screensaver)"));
    lines.push(Line::from("    Tab/BackTab  Switch menu tabs"));
    lines.push(Line::from("    ↑/↓ or j/k   Navigate lists"));
    lines.push(Line::from("    Enter        Select / edit / launch"));
    lines.push(Line::from("    s            Save configuration"));
    lines.push(Line::from(""));

    // Per-screensaver keys
    lines.push(Line::from(Span::styled("  Screensaver Controls", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))));
    lines.push(Line::from(""));

    for info in registry::SCREENSAVERS.iter().filter(|s| s.name != "random") {
        let has_custom_keys = info.key_bindings.len() > 1
            || (info.key_bindings.len() == 1 && info.key_bindings[0].0 != "Any key");
        if has_custom_keys || !info.features.is_empty() {
            lines.push(Line::from(Span::styled(format!("    {}", info.name), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))));
            if has_custom_keys {
                for (key, desc) in info.key_bindings {
                    lines.push(Line::from(format!("      {key:<14} {desc}")));
                }
            }
            if !info.features.is_empty() {
                for feat in info.features {
                    lines.push(Line::from(format!("      • {feat}")));
                }
            }
            lines.push(Line::from(""));
        }
    }

    // Features
    lines.push(Line::from(Span::styled("  Features", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))));
    lines.push(Line::from(""));
    lines.push(Line::from("    Auto-Rotate     Cycle through screensavers on a timer"));
    lines.push(Line::from("    Transitions     Dissolve, slide, or melt between screensavers"));
    lines.push(Line::from("    Clock Overlay   Show time over any screensaver"));
    lines.push(Line::from("    Screen Lock     Require password to exit (PAM)"));
    lines.push(Line::from("    Screen Blank    Go dark after inactivity"));
    lines.push(Line::from("    Battery Aware   Reduce FPS on battery power"));
    lines.push(Line::from("    Ambient Mode    Subtle background animation"));
    lines.push(Line::from("    Themes          default, neon, pastel, monochrome, dracula"));

    let para = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Help"))
        .scroll((ui.help_scroll, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}

fn render_about(frame: &mut ratatui::Frame, area: Rect) {
    let version = env!("CARGO_PKG_VERSION");
    let config_path = Config::config_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "unknown".into());

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled("  Termsaver", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from(format!("  Version: {version}")),
        Line::from(""),
        Line::from(format!("  Config: {config_path}")),
        Line::from(""),
        Line::from("  Shell integration:"),
        Line::from(Span::styled("    eval \"$(termsaver init bash)\"", Style::default().fg(Color::Green))),
        Line::from(""),
        Line::from("  Commands:"),
        Line::from("    termsaver              Open this menu"),
        Line::from("    termsaver run          Launch screensaver directly"),
        Line::from("    termsaver list         List all screensavers"),
        Line::from("    termsaver preview <n>  Preview a screensaver"),
        Line::from("    termsaver config       Open this menu"),
        Line::from("    termsaver gallery      Live split-screen gallery"),
    ];

    let para = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("About"))
        .wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}

fn render_status_bar(ui: &MenuUI, frame: &mut ratatui::Frame, area: Rect) {
    let status = if let Some(ref msg) = ui.message {
        msg.clone()
    } else if ui.setting_editing || ui.saver_config_editing {
        "Type value, Enter to confirm, Esc to cancel".into()
    } else {
        let resume_hint = if matches!(ui.context, MenuContext::DuringScreensaver { .. }) { "  r: Resume" } else { "" };
        match ui.tab {
            Tab::Screensavers => format!("Enter: Launch  p: Preview  d: Set default  s: Save  ←→: Config{resume_hint}  q: Quit"),
            Tab::Settings => format!("Enter: Edit/toggle  s: Save{resume_hint}  q: Quit"),
            Tab::Help => format!("↑↓: Scroll{resume_hint}  q: Quit"),
            Tab::About => format!("{resume_hint}  q: Quit").trim_start().to_string(),
        }
    };
    let status_bar = Paragraph::new(format!("  {status}"))
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(status_bar, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_entries_count() {
        let ui = MenuUI::new(Config::default(), MenuContext::Standalone);
        let entries = ui.setting_entries();
        let setting_count = entries.iter().filter(|e| matches!(e, SettingEntry::Setting { .. })).count();
        assert_eq!(setting_count, 29);
        let header_count = entries.iter().filter(|e| matches!(e, SettingEntry::Header(_))).count();
        assert_eq!(header_count, 8);
    }

    #[test]
    fn apply_setting_round_trip() {
        let mut ui = MenuUI::new(Config::default(), MenuContext::Standalone);
        assert_eq!(ui.config.general.fps, 30);
        ui.apply_setting(1, "60");
        assert_eq!(ui.config.general.fps, 60);
    }

    #[test]
    fn apply_setting_bool_toggle() {
        let mut ui = MenuUI::new(Config::default(), MenuContext::Standalone);
        assert!(ui.config.general.fade_enabled);
        ui.apply_setting(3, "");
        assert!(!ui.config.general.fade_enabled);
        ui.apply_setting(3, "");
        assert!(ui.config.general.fade_enabled);
    }

    #[test]
    fn saver_config_items_all_screensavers() {
        let ui = MenuUI::new(Config::default(), MenuContext::Standalone);
        let savers = MenuUI::real_savers();
        for (i, info) in savers.iter().enumerate() {
            let mut test_ui = MenuUI::new(Config::default(), MenuContext::Standalone);
            test_ui.saver_selected = i;
            let items = test_ui.saver_config_items();
            assert!(!items.is_empty(), "No config items for {}", info.name);
        }
    }

    #[test]
    fn apply_saver_config_round_trip() {
        let mut ui = MenuUI::new(Config::default(), MenuContext::Standalone);
        // starfield is index 1
        ui.saver_selected = 1;
        assert_eq!(ui.config.screensavers.starfield.star_count, 200);
        ui.apply_saver_config(0, "500");
        assert_eq!(ui.config.screensavers.starfield.star_count, 500);
    }

    #[test]
    fn setting_move_skips_headers() {
        let mut ui = MenuUI::new(Config::default(), MenuContext::Standalone);
        ui.setting_cursor = 0;
        ui.setting_move(1); // Should skip header at 0 if starting there
        let entries = ui.setting_entries();
        assert!(matches!(entries[ui.setting_cursor], SettingEntry::Setting { .. }));
    }
}
