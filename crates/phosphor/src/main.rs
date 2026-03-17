mod ambient;
mod app;
mod auth;
mod compositor;
mod config;
mod fade;
mod gallery;
mod menu;
mod overlay;
mod plugin_host;
mod power;
mod registry;
mod screensaver;
mod stats;
mod theme;
mod transition;
mod watch;

use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};

use config::Config;

#[derive(Parser)]
#[command(name = "phosphor", about = "Terminal screensaver", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Launch a screensaver (default if no subcommand given)
    Run {
        /// Which screensaver to run
        #[arg(short, long)]
        screensaver: Option<String>,

        /// Target FPS
        #[arg(long)]
        fps: Option<u32>,

        /// Disable fade transition
        #[arg(long)]
        no_fade: bool,

        /// Run two screensavers in split-screen (e.g., "starfield|matrix_rain" for vertical, "starfield/matrix_rain" for horizontal)
        #[arg(long)]
        split: Option<String>,
    },

    /// Open interactive TUI configuration
    Config,

    /// List available screensavers
    List,

    /// Preview a screensaver for a few seconds
    Preview {
        /// Which screensaver to preview
        name: String,

        /// Preview duration in seconds
        #[arg(short, long, default_value = "5")]
        duration: u64,
    },

    /// Print shell integration script to stdout
    Init {
        /// Shell type
        #[arg(value_enum, default_value = "bash")]
        shell: ShellType,
    },

    /// Print the configured timeout value (for shell integration)
    PrintTimeout,

    /// Show all screensavers in a live split-screen gallery
    Gallery,

    /// Show usage statistics
    Stats,

    /// Background idle watcher (used by shell integration)
    Watch {
        /// PID of the parent shell
        #[arg(long)]
        shell_pid: u32,

        /// Idle timeout in seconds
        #[arg(long)]
        timeout: u64,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum ShellType {
    Bash,
    Zsh,
}

fn main() {
    let cli = Cli::parse();
    let mut config = Config::load();

    match cli.command {
        None | Some(Command::Config) => {
            // No subcommand or `config`: open interactive menu
            loop {
                match menu::run(&config, menu::MenuContext::Standalone) {
                    Ok(menu::MenuResult::Launch(name)) => {
                        config.general.default_screensaver = name;
                        config.general.fade_enabled = false;
                        let _ = app::run(&config, None);
                        config = Config::load(); // Reload any saved changes
                    }
                    _ => break,
                }
            }
        }

        Some(Command::Run {
            screensaver,
            fps,
            no_fade,
            split,
        }) => {
            config.merge_cli(screensaver.as_deref(), fps, no_fade);
            if let Some(split_str) = split {
                // Parse split string
                let (left, right, orientation) = if let Some(pos) = split_str.find('|') {
                    (
                        &split_str[..pos],
                        &split_str[pos + 1..],
                        compositor::SplitOrientation::Vertical,
                    )
                } else if let Some(pos) = split_str.find('/') {
                    (
                        &split_str[..pos],
                        &split_str[pos + 1..],
                        compositor::SplitOrientation::Horizontal,
                    )
                } else {
                    eprintln!("Invalid split format. Use 'name1|name2' for vertical or 'name1/name2' for horizontal");
                    std::process::exit(1);
                };
                if let Err(e) = compositor::run(&config, left, right, orientation, None) {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            } else if let Err(e) = app::run(&config, None) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }

        Some(Command::List) => {
            println!("Available screensavers:\n");
            for plugin in registry::plugins() {
                println!("  {:<20} {}", plugin.name, plugin.info.description);
            }
            println!("  {:<20} Randomly selects a screensaver each time", "random");
        }

        Some(Command::Preview { name, duration }) => {
            config.general.default_screensaver = name;
            config.general.fade_enabled = false;
            if let Err(e) = app::run(&config, Some(Duration::from_secs(duration))) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }

        Some(Command::Init { shell }) => {
            print_shell_init(&shell, &config);
        }

        Some(Command::PrintTimeout) => {
            println!("{}", config.general.timeout);
        }

        Some(Command::Gallery) => {
            if let Err(e) = gallery::run(&config) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }

        Some(Command::Stats) => {
            let stats = stats::Stats::load();
            println!("{}", stats.format_summary());
        }

        Some(Command::Watch { shell_pid, timeout }) => {
            if let Err(e) = watch::run(shell_pid, timeout, &config) {
                eprintln!("Watch error: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn print_shell_init(shell: &ShellType, config: &Config) {
    let timeout = config.general.timeout;
    match shell {
        ShellType::Bash => {
            print!(
                r#"# phosphor shell integration for bash
# Add to your .bashrc: eval "$(phosphor init bash)"

# Guard: only run in interactive shells
[[ $- != *i* ]] && return

# Guard: don't activate inside nested shells
[[ -n "$PHOSPHOR_ACTIVE" ]] && return

# Kill existing watcher from previous source
[[ -n "$_PH_WATCHER_PID" ]] && kill "$_PH_WATCHER_PID" 2>/dev/null

# Heartbeat file — mtime tracks when the user last ran a command
_PH_HEARTBEAT="/tmp/.ts_heartbeat_$$"
: > "$_PH_HEARTBEAT"

# Start background watcher (monitors heartbeat, takes over terminal when idle)
command phosphor watch --shell-pid $$ --timeout {timeout} &
_PH_WATCHER_PID=$!
disown "$_PH_WATCHER_PID" 2>/dev/null

# Update heartbeat on each prompt
PROMPT_COMMAND=": > $_PH_HEARTBEAT${{PROMPT_COMMAND:+;$PROMPT_COMMAND}}"
"#
            );
        }
        ShellType::Zsh => {
            print!(
                r#"# phosphor shell integration for zsh
# Add to your .zshrc: eval "$(phosphor init zsh)"

# Guard: only run in interactive shells
[[ -o interactive ]] || return

# Guard: don't activate inside nested shells if already active
[[ -n "$PHOSPHOR_ACTIVE" ]] && return

# Set TMOUT to trigger ALRM after idle period
TMOUT={timeout}

# Trap ALRM signal
TRAPALRM() {{
    [[ ! -t 0 ]] && return
    [[ -n "$PHOSPHOR_ACTIVE" ]] && return

    unset TMOUT

    export PHOSPHOR_ACTIVE=1
    phosphor run
    unset PHOSPHOR_ACTIVE

    TMOUT={timeout}
}}
"#
            );
        }
    }
}
