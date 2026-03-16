mod ambient;
mod app;
mod auth;
mod config;
mod fade;
mod gallery;
mod menu;
mod overlay;
mod power;
mod registry;
mod screensaver;
mod screensavers;
mod theme;
mod transition;
mod watch;

use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};

use config::Config;

#[derive(Parser)]
#[command(name = "termsaver", about = "Terminal screensaver", version)]
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
        }) => {
            config.merge_cli(screensaver.as_deref(), fps, no_fade);
            if let Err(e) = app::run(&config, None) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }

        Some(Command::List) => {
            println!("Available screensavers:\n");
            for info in registry::SCREENSAVERS {
                println!("  {:<20} {}", info.name, info.description);
            }
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
                r#"# termsaver shell integration for bash
# Add to your .bashrc: eval "$(termsaver init bash)"

# Guard: only run in interactive shells
[[ $- != *i* ]] && return

# Guard: don't activate inside nested shells
[[ -n "$TERMSAVER_ACTIVE" ]] && return

# Kill existing watcher from previous source
[[ -n "$_TS_WATCHER_PID" ]] && kill "$_TS_WATCHER_PID" 2>/dev/null

# Heartbeat file — mtime tracks when the user last ran a command
_TS_HEARTBEAT="/tmp/.ts_heartbeat_$$"
: > "$_TS_HEARTBEAT"

# Start background watcher (monitors heartbeat, takes over terminal when idle)
command termsaver watch --shell-pid $$ --timeout {timeout} &
_TS_WATCHER_PID=$!
disown "$_TS_WATCHER_PID" 2>/dev/null

# Update heartbeat on each prompt
PROMPT_COMMAND=": > $_TS_HEARTBEAT${{PROMPT_COMMAND:+;$PROMPT_COMMAND}}"
"#
            );
        }
        ShellType::Zsh => {
            print!(
                r#"# termsaver shell integration for zsh
# Add to your .zshrc: eval "$(termsaver init zsh)"

# Guard: only run in interactive shells
[[ -o interactive ]] || return

# Guard: don't activate inside nested shells if already active
[[ -n "$TERMSAVER_ACTIVE" ]] && return

# Set TMOUT to trigger ALRM after idle period
TMOUT={timeout}

# Trap ALRM signal
TRAPALRM() {{
    [[ ! -t 0 ]] && return
    [[ -n "$TERMSAVER_ACTIVE" ]] && return

    unset TMOUT

    export TERMSAVER_ACTIVE=1
    termsaver run
    unset TERMSAVER_ACTIVE

    TMOUT={timeout}
}}
"#
            );
        }
    }
}
