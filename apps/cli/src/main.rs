//! `navori` — command-line client for the running Navori app.
//!
//! Exit codes: 0 success · 1 command failed · 2 usage error · 3 app not running.

use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use navori_config::{Paths, Profile};
use navori_ipc::{Client, IpcError};
use navori_protocol::rpc::method;
use navori_protocol::{ActionSpec, Check, CoreStatus, Event, ModuleState};
use serde_json::{Value, json};

#[derive(Debug, Parser)]
#[command(name = "navori", version, about = "Talk to the running Navori app")]
struct Cli {
    /// Which install to talk to.
    #[arg(long, env = "NAVORI_PROFILE", default_value = "release", global = true, value_parser = ["release", "dev"])]
    profile: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show version, uptime and module health.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Run health checks.
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// Show recent events from the journal (newest first).
    Events {
        #[arg(long, default_value_t = 20)]
        limit: u32,
        #[arg(long)]
        json: bool,
    },
    /// List every action modules accept.
    Actions {
        #[arg(long)]
        json: bool,
    },
    /// Send an action to a module, e.g. `navori dispatch system '"ping"'`.
    Dispatch {
        module: String,
        /// The action as JSON: `"name"` or `{"name": {...}}`.
        action: String,
    },
    /// Keep this Mac awake.
    Awake {
        #[command(subcommand)]
        command: AwakeCommand,
    },
}

#[derive(Debug, Subcommand)]
enum AwakeCommand {
    /// Keep this Mac awake for a duration, or until turned off.
    Hold {
        /// How long to hold, as Ns, Nm or Nh. Omit to hold until turned off.
        #[arg(long = "for", value_name = "DURATION")]
        duration: Option<String>,
    },
    /// End every lease Navori is holding.
    ReleaseAll,
    /// Show the leases that are keeping this Mac awake.
    Status {
        #[arg(long)]
        json: bool,
    },
}

enum Failure {
    Usage(String),
    NotRunning(String),
    Failed(String),
}

impl From<IpcError> for Failure {
    fn from(e: IpcError) -> Self {
        match e {
            IpcError::NotRunning(_) => Failure::NotRunning(e.to_string()),
            other => Failure::Failed(other.to_string()),
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("navori: cannot start runtime: {e}");
            return ExitCode::from(1);
        }
    };
    match runtime.block_on(run(cli)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(m)) => {
            eprintln!("navori: {m}");
            ExitCode::from(2)
        }
        Err(Failure::NotRunning(m)) => {
            eprintln!("navori: {m}");
            ExitCode::from(3)
        }
        Err(Failure::Failed(m)) => {
            eprintln!("navori: {m}");
            ExitCode::from(1)
        }
    }
}

fn socket_path(profile: &str) -> Result<PathBuf, Failure> {
    let profile = Profile::parse(profile)
        .ok_or_else(|| Failure::Usage(format!("unknown profile {profile:?}")))?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| Failure::Usage("HOME is not set".into()))?;
    let uid = std::fs::metadata(&home)
        .map(|m| m.uid())
        .map_err(|e| Failure::Failed(format!("{}: {e}", home.display())))?;
    Ok(Paths::new(profile, &home, uid).socket)
}

async fn run(cli: Cli) -> Result<(), Failure> {
    let socket = socket_path(&cli.profile)?;

    if let Command::Doctor { json } = cli.command {
        return doctor(&socket, json).await;
    }
    if let Command::Dispatch { action, .. } = &cli.command
        && let Err(e) = serde_json::from_str::<Value>(action)
    {
        return Err(Failure::Usage(format!("action is not valid JSON: {e}")));
    }
    // A bad --for is usage (exit 2) even when the app is down.
    if let Command::Awake {
        command: AwakeCommand::Hold { duration },
    } = &cli.command
    {
        let _ = awake_hold_action(duration.as_deref())?;
    }

    let mut client = Client::connect(&socket).await?;
    match cli.command {
        Command::Status { json } => {
            let v = client.call(method::STATUS, Value::Null).await?;
            if json {
                print_json(&v);
            } else {
                print_status(&decode::<CoreStatus>(v)?);
            }
        }
        Command::Events { limit, json } => {
            let v = client
                .call(method::EVENTS, json!({ "limit": limit }))
                .await?;
            if json {
                print_json(&v);
            } else {
                for e in decode::<Vec<Event>>(v)? {
                    println!(
                        "{}  {:<5}  {:<8} {}",
                        e.id,
                        format!("{:?}", e.severity).to_lowercase(),
                        e.module,
                        e.summary
                    );
                }
            }
        }
        Command::Actions { json } => {
            let v = client.call(method::ACTIONS, Value::Null).await?;
            if json {
                print_json(&v);
            } else {
                for a in decode::<Vec<ActionSpec>>(v)? {
                    let name = format!("{}.{}", a.module, a.name);
                    println!(
                        "{name:<28} {:<12} {}",
                        format!("{:?}", a.risk).to_lowercase(),
                        a.description
                    );
                }
            }
        }
        Command::Dispatch { module, action } => {
            let action: Value =
                serde_json::from_str(&action).map_err(|e| Failure::Usage(e.to_string()))?;
            let v = client
                .call(
                    method::DISPATCH,
                    json!({ "module": module, "action": action }),
                )
                .await?;
            println!("{v}");
        }
        Command::Awake { command } => match command {
            AwakeCommand::Hold { duration } => {
                let action = awake_hold_action(duration.as_deref())?;
                dispatch_awake(&mut client, action).await?;
            }
            AwakeCommand::ReleaseAll => {
                dispatch_awake(&mut client, json!("release_all")).await?;
            }
            AwakeCommand::Status { json } => {
                let v = dispatch_awake(&mut client, json!("status")).await?;
                if json {
                    print_json(&v);
                } else {
                    print_awake_status(&v);
                }
            }
        },
        Command::Doctor { .. } => unreachable!("handled above"),
    }
    Ok(())
}

async fn dispatch_awake(client: &mut Client, action: Value) -> Result<Value, Failure> {
    client
        .call(
            method::DISPATCH,
            json!({ "module": "awake", "action": action }),
        )
        .await
        .map_err(Into::into)
}

fn awake_hold_action(duration: Option<&str>) -> Result<Value, Failure> {
    match duration {
        Some(raw) => Ok(json!({ "hold_for": { "seconds": parse_duration(raw)? } })),
        None => Ok(json!("hold_until_off")),
    }
}

fn parse_duration(raw: &str) -> Result<u64, Failure> {
    let split = raw.find(|c: char| !c.is_ascii_digit()).unwrap_or(raw.len());
    let (digits, unit) = raw.split_at(split);
    let n: u64 = digits.parse().map_err(|_| duration_usage(raw))?;
    if n == 0 || unit.len() != 1 {
        return Err(duration_usage(raw));
    }
    match unit {
        "s" => Ok(n),
        "m" => n.checked_mul(60).ok_or_else(|| duration_usage(raw)),
        "h" => n.checked_mul(3_600).ok_or_else(|| duration_usage(raw)),
        _ => Err(duration_usage(raw)),
    }
}

fn duration_usage(raw: &str) -> Failure {
    Failure::Usage(format!(
        "duration {raw:?} is not Ns, Nm or Nh (for example 90s, 30m, 1h)"
    ))
}

async fn doctor(socket: &std::path::Path, json: bool) -> Result<(), Failure> {
    let mut checks = Vec::new();
    let mut not_running = false;
    match Client::connect(socket).await {
        Ok(mut client) => {
            checks.push(Check {
                name: "app".into(),
                ok: true,
                detail: format!("reachable at {}", socket.display()),
            });
            let v = client.call(method::DOCTOR, Value::Null).await?;
            checks.extend(decode::<Vec<Check>>(v)?);
        }
        Err(IpcError::NotRunning(_)) => {
            not_running = true;
            checks.push(Check {
                name: "app".into(),
                ok: false,
                detail: format!("not running (no socket at {})", socket.display()),
            });
        }
        Err(e) => return Err(e.into()),
    }

    if json {
        print_json(&serde_json::to_value(&checks).unwrap_or(Value::Null));
    } else {
        for c in &checks {
            println!(
                "{} {:<10} {}",
                if c.ok { "✓" } else { "✗" },
                c.name,
                c.detail
            );
        }
    }
    if not_running {
        Err(Failure::NotRunning("Navori is not running".into()))
    } else if checks.iter().all(|c| c.ok) {
        Ok(())
    } else {
        Err(Failure::Failed("some checks failed".into()))
    }
}

fn decode<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, Failure> {
    serde_json::from_value(v)
        .map_err(|e| Failure::Failed(format!("unexpected response from the app: {e}")))
}

fn print_json(v: &Value) {
    match serde_json::to_string_pretty(v) {
        Ok(s) => println!("{s}"),
        Err(_) => println!("{v}"),
    }
}

fn print_awake_status(v: &Value) {
    let leases = v["leases"].as_array();
    let count = leases.map(Vec::len).unwrap_or(0);
    let noun = if count == 1 { "Lease" } else { "Leases" };
    println!("{count} {noun}");
    if let Some(leases) = leases {
        for lease in leases {
            let line = lease["label"]
                .as_str()
                .filter(|label| !label.is_empty())
                .or_else(|| lease["kind"].as_str())
                .unwrap_or("lease");
            println!("  {line}");
        }
    }
    let system_held = v["system_held"].as_bool().unwrap_or(false);
    if count > 0 && !system_held {
        println!("not held");
    }
}

fn print_status(s: &CoreStatus) {
    let secs = s.uptime_ms / 1000;
    println!(
        "Navori {} ({}) — pid {}, up {}h {}m {}s",
        s.version,
        s.profile,
        s.pid,
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    );
    println!("data     {}", s.data_dir);
    println!("socket   {}", s.socket_path);
    println!("modules");
    for m in &s.modules {
        let state = match &m.state {
            ModuleState::Starting => "starting".to_owned(),
            ModuleState::Running => "running".to_owned(),
            ModuleState::Restarting {
                attempt,
                last_error,
            } => format!("restarting (attempt {attempt}): {last_error}"),
            ModuleState::Failed { last_error } => format!("FAILED: {last_error}"),
            ModuleState::Stopped => "stopped".to_owned(),
        };
        let restarts = if m.restarts > 0 {
            format!(" · {} restarts", m.restarts)
        } else {
            String::new()
        };
        println!("  {:<10} {state}{restarts}", m.id);
    }
}
