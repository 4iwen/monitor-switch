use std::path::PathBuf;

use clap::{Parser, Subcommand};
use monitor_switch::{
    backends::{
        get_input_source, get_supported_input_sources, set_input_source, ReadRequest, SwitchRequest,
    },
    config::{default_config_path, BackendKind, Config, SourceProfile},
    error::{AppError, Result},
    sources::{format_input_value, parse_input_value, COMMON_INPUT_CANDIDATES},
};

#[derive(Debug, Parser)]
#[command(name = "monitor-switch")]
#[command(about = "Switch monitor inputs through DDC/CI")]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the config path that monitor-switch will use.
    ConfigPath,

    /// Create the initial config file.
    Init {
        #[arg(long)]
        force: bool,
    },

    /// List common DDC/CI input source candidates.
    Candidates,

    /// List displays available through the built-in DDC/CI backend.
    Displays,

    /// List configured source profiles.
    List,

    /// Read the current monitor input source.
    Current {
        #[arg(long)]
        display: Option<String>,

        #[arg(long, default_value = "auto")]
        backend: String,
    },

    /// List input source values reported by the monitor.
    Supported {
        #[arg(long)]
        display: Option<String>,

        #[arg(long, default_value = "auto")]
        backend: String,
    },

    /// Add a source profile.
    Add {
        name: String,
        value: String,

        #[arg(long)]
        label: Option<String>,

        #[arg(long)]
        display: Option<String>,

        #[arg(long)]
        backend: Option<String>,
    },

    /// Remove a source profile.
    Remove { name: String },

    /// Update a source profile.
    Update {
        name: String,

        #[arg(long)]
        value: Option<String>,

        #[arg(long)]
        label: Option<String>,

        #[arg(long)]
        display: Option<String>,

        #[arg(long)]
        backend: Option<String>,
    },

    /// Switch to a configured source profile.
    Switch {
        name: String,

        #[arg(long)]
        dry_run: bool,
    },

    /// Send a raw input source value without saving it as a profile.
    Raw {
        value: String,

        #[arg(long)]
        display: Option<String>,

        #[arg(long, default_value = "auto")]
        backend: String,

        #[arg(long)]
        dry_run: bool,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let config_path = cli.config.unwrap_or_else(default_config_path);

    match cli.command {
        Command::ConfigPath => {
            println!("{}", config_path.display());
        }
        Command::Init { force } => {
            if config_path.exists() && !force {
                return Err(AppError::ConfigAlreadyExists(
                    config_path.display().to_string(),
                ));
            }

            Config::default().save(&config_path)?;
            println!("created {}", config_path.display());
        }
        Command::Candidates => print_candidates(),
        Command::Displays => {
            let displays = monitor_switch::backends::list_displays();
            if displays.is_empty() {
                println!("no DDC/CI-capable displays detected");
            } else {
                for (index, display) in displays.iter().enumerate() {
                    println!(
                        "{}\t{}\t{}\t{}",
                        index + 1,
                        display.name,
                        display.selector,
                        display.detail
                    );
                }
            }
        }
        Command::List => {
            let config = Config::load(&config_path)?;
            print_profiles(&config);
        }
        Command::Current { display, backend } => {
            let value = get_input_source(ReadRequest {
                backend: BackendKind::parse(&backend)?,
                display: display.as_deref(),
            })?;
            println!("current input source: {}", format_input_value(value));
        }
        Command::Supported { display, backend } => {
            let values = get_supported_input_sources(ReadRequest {
                backend: BackendKind::parse(&backend)?,
                display: display.as_deref(),
            })?;

            if values.is_empty() {
                println!("no supported input source values reported by this backend");
            } else {
                println!("supported input sources:");
                for value in values {
                    println!("{}", format_input_value(value));
                }
            }
        }
        Command::Add {
            name,
            value,
            label,
            display,
            backend,
        } => {
            let mut config = Config::load(&config_path)?;
            if config.find_profile(&name).is_some() {
                return Err(AppError::ProfileExists(name));
            }

            let profile = SourceProfile {
                name,
                value: parse_input_value(&value)?,
                label,
                display,
                backend: backend.as_deref().map(BackendKind::parse).transpose()?,
                show_in_menu: true,
            };

            config.profiles.push(profile);
            config.save(&config_path)?;
            println!("saved {}", config_path.display());
        }
        Command::Remove { name } => {
            let mut config = Config::load(&config_path)?;
            let before = config.profiles.len();
            config.profiles.retain(|profile| profile.name != name);

            if before == config.profiles.len() {
                return Err(AppError::ProfileMissing(name));
            }

            config.save(&config_path)?;
            println!("removed profile");
        }
        Command::Update {
            name,
            value,
            label,
            display,
            backend,
        } => {
            let mut config = Config::load(&config_path)?;
            let profile = config
                .profiles
                .iter_mut()
                .find(|profile| profile.name == name)
                .ok_or_else(|| AppError::ProfileMissing(name.clone()))?;

            if let Some(value) = value {
                profile.value = parse_input_value(&value)?;
            }
            if let Some(label) = label {
                profile.label = Some(label);
            }
            if let Some(display) = display {
                profile.display = Some(display);
            }
            if let Some(backend) = backend {
                profile.backend = Some(BackendKind::parse(&backend)?);
            }

            config.save(&config_path)?;
            println!("updated profile `{name}`");
        }
        Command::Switch { name, dry_run } => {
            let config = Config::load(&config_path)?;
            let profile = config
                .find_profile(&name)
                .ok_or_else(|| AppError::ProfileMissing(name.clone()))?;

            let action = set_input_source(SwitchRequest {
                backend: config.effective_backend(Some(profile)),
                display: config.effective_display(Some(profile)),
                value: profile.value,
                dry_run,
            })?;

            print_action(
                "switched",
                action.backend,
                &action.program,
                &action.args,
                action.dry_run,
            );
        }
        Command::Raw {
            value,
            display,
            backend,
            dry_run,
        } => {
            let action = set_input_source(SwitchRequest {
                backend: BackendKind::parse(&backend)?,
                display: display.as_deref(),
                value: parse_input_value(&value)?,
                dry_run,
            })?;

            print_action(
                "sent",
                action.backend,
                &action.program,
                &action.args,
                action.dry_run,
            );
        }
    }

    Ok(())
}

fn print_candidates() {
    println!("{:<8} {:<18} note", "value", "candidate");
    for candidate in COMMON_INPUT_CANDIDATES {
        println!(
            "{:<8} {:<18} {}",
            format_input_value(candidate.value),
            candidate.name,
            candidate.note
        );
    }
}

fn print_profiles(config: &Config) {
    if config.profiles.is_empty() {
        println!("no profiles configured");
        return;
    }

    println!(
        "{:<14} {:<8} {:<20} {:<14} backend",
        "name", "value", "label", "display"
    );

    for profile in &config.profiles {
        println!(
            "{:<14} {:<8} {:<20} {:<14} {}",
            profile.name,
            format_input_value(profile.value),
            profile.label.as_deref().unwrap_or("-"),
            profile.display.as_deref().unwrap_or("-"),
            profile.backend.unwrap_or(config.default_backend)
        );
    }
}

fn print_action(prefix: &str, backend: BackendKind, program: &str, args: &[String], dry_run: bool) {
    let marker = if dry_run { "dry-run: " } else { "" };
    println!(
        "{marker}{prefix} via {backend} backend: `{program} {}`",
        args.join(" ")
    );
}
