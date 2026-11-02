use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use escope_host::{capability_names, protocol::capability, Session};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Parser)]
#[command(
    name = "escope",
    version,
    about = "Embedded diagnostics & control",
    long_about = "EmbedScope connects to a device Agent over serial or TCP.\nRun `escope simulate` for a software reference device; it does not represent physical hardware."
)]
struct Cli {
    #[arg(
        short = 'd',
        long,
        global = true,
        help = "Serial path or tcp://host:port; auto-selects exactly one serial port"
    )]
    device: Option<String>,
    #[arg(long, global = true, default_value_t = 115200)]
    baud: u32,
    #[arg(long, global = true, value_enum, default_value_t = Dtr::Preserve, help = "Serial DTR line policy; some USB CDC consoles require on")]
    dtr: Dtr,
    #[arg(long, global = true, default_value_t = 3000, value_parser = clap::value_parser!(u64).range(1..))]
    timeout_ms: u64,
    #[arg(
        long,
        global = true,
        help = "Emit JSON (one object per streaming record)"
    )]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum Dtr {
    Preserve,
    On,
    Off,
}

#[derive(Subcommand)]
enum Command {
    /// List serial ports without opening them.
    Devices,
    /// Start the software reference device (synthetic pixels and metrics).
    Simulate {
        #[arg(long, default_value = "127.0.0.1:9337")]
        listen: String,
    },
    /// Show build identity, device information and capabilities.
    Info,
    /// Read structured log events.
    Logs {
        #[arg(long)]
        follow: bool,
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        count: Option<u64>,
        #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u8).range(1..=5))]
        level: u8,
        #[arg(long)]
        target: Option<String>,
    },
    /// Read system metrics; unavailable measurements are null.
    Metrics {
        #[arg(long)]
        watch: bool,
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        count: Option<u64>,
        #[arg(long, default_value_t = 1000, value_parser = clap::value_parser!(u64).range(1..))]
        interval_ms: u64,
    },
    /// Inspect the device UI tree.
    Ui {
        #[command(subcommand)]
        command: UiCommand,
    },
    /// Capture the device's display output.
    Display {
        #[command(subcommand)]
        command: DisplayCommand,
    },
    /// Inject input through the platform's input adapter.
    Input {
        #[command(subcommand)]
        command: InputCommand,
    },
    /// Read a platform-provided crash summary.
    Crash {
        #[command(subcommand)]
        command: CrashCommand,
    },
    /// Export identity, logs, metrics, UI and screenshot to a new directory.
    Diagnose(DiagnoseArgs),
}
#[derive(Subcommand)]
enum UiCommand {
    Tree,
}
#[derive(Subcommand)]
enum DisplayCommand {
    Capture { output: PathBuf },
}
#[derive(Subcommand)]
enum InputCommand {
    Tap {
        x: u32,
        y: u32,
        #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u32).range(1..=5000))]
        hold_ms: u32,
    },
}
#[derive(Subcommand)]
enum CrashCommand {
    Show,
}
#[derive(Args)]
struct DiagnoseArgs {
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value_t = 8, value_parser = clap::value_parser!(u64).range(1..=1000))]
    logs: u64,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(error) = run(&cli).await {
        if cli.json {
            eprintln!(
                "{}",
                serde_json::json!({ "ok": false, "error": error.to_string() })
            );
        } else {
            eprintln!("escope: {error:#}");
        }
        std::process::exit(1);
    }
}

fn emit<T: Serialize>(value: &T, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(value)?);
    } else {
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    Ok(())
}

async fn run(cli: &Cli) -> Result<()> {
    match &cli.command {
        Command::Devices => return emit(&escope_host::devices()?, cli.json),
        Command::Simulate { listen } => {
            let listener = tokio::net::TcpListener::bind(listen).await?;
            emit(
                &serde_json::json!({"source":"software-reference", "endpoint":format!("tcp://{}",listener.local_addr()?),"physical_device":false}),
                cli.json,
            )?;
            let (stop, receiver) = tokio::sync::watch::channel(false);
            let server = escope_host::simulator::serve(listener, receiver);
            tokio::pin!(server);
            tokio::select! {
                result = &mut server => result?,
                _ = tokio::signal::ctrl_c() => { let _ = stop.send(true); server.await?; }
            }
            return Ok(());
        }
        _ => {}
    }
    let endpoint = select_device(cli.device.as_deref())?;
    let session = Session::connect_with_options(
        &endpoint,
        escope_host::ConnectOptions {
            baud: cli.baud,
            timeout: Duration::from_millis(cli.timeout_ms),
            dtr: match cli.dtr {
                Dtr::Preserve => None,
                Dtr::On => Some(true),
                Dtr::Off => Some(false),
            },
        },
    )
    .await?;
    match &cli.command {
        Command::Info => emit(
            &serde_json::json!({"device": session.info().await?, "session": session.hello(), "capabilities":capability_names(session.hello().capabilities)}),
            cli.json,
        )?,
        Command::Logs {
            follow,
            count,
            level,
            target,
        } => {
            let mut events = session.subscribe();
            session.start_logs(*level).await?;
            let limit = count.unwrap_or(if *follow { u64::MAX } else { 1 });
            let mut printed = 0;
            let mut deadline = tokio::time::Instant::now() + Duration::from_millis(cli.timeout_ms);
            while printed < limit {
                let next = async {
                    match tokio::time::timeout_at(deadline, events.recv()).await {
                        Ok(Ok(event)) => Ok(Some(event.log()?)),
                        Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                            emit(
                                &serde_json::json!({"type":"host_log_gap","dropped":n}),
                                cli.json,
                            )?;
                            Ok(None)
                        }
                        Ok(Err(_)) => bail!("event stream closed"),
                        Err(_) if session.is_closed() => bail!("device disconnected"),
                        Err(_) if *follow => Ok(None),
                        Err(_) => bail!("no matching log received before timeout"),
                    }
                };
                tokio::select! {
                    result = next => {
                        if let Some(log) = result? { if target.as_ref().is_none_or(|t| log.target.contains(t)) {
                            emit(&log, cli.json)?; printed += 1;
                            deadline = tokio::time::Instant::now() + Duration::from_millis(cli.timeout_ms);
                        } }
                        if *follow && tokio::time::Instant::now() >= deadline { deadline = tokio::time::Instant::now() + Duration::from_millis(cli.timeout_ms); }
                    }
                    _ = tokio::signal::ctrl_c() => break,
                }
            }
            session.stop_logs().await?;
        }
        Command::Metrics {
            watch,
            count,
            interval_ms,
        } => {
            let limit = count.unwrap_or(if *watch { u64::MAX } else { 1 });
            for index in 0..limit {
                emit(&session.metrics().await?, cli.json)?;
                if index + 1 < limit {
                    tokio::select! { _ = tokio::time::sleep(Duration::from_millis(*interval_ms)) => {}, _ = tokio::signal::ctrl_c() => break }
                }
            }
        }
        Command::Ui {
            command: UiCommand::Tree,
        } => {
            let nodes = session.ui_tree().await?;
            if cli.json {
                emit(&nodes, true)?;
            } else {
                for node in &nodes {
                    println!(
                        "#{:<5} parent={:<5} {:<12} [{},{},{}×{}] {}",
                        node.id,
                        node.parent,
                        node.kind,
                        node.x,
                        node.y,
                        node.width,
                        node.height,
                        node.name
                    );
                }
            }
        }
        Command::Display {
            command: DisplayCommand::Capture { output },
        } => {
            let capture = session.capture().await?;
            let file = new_file_path(output)?;
            capture.save_png(file)?;
            emit(
                &serde_json::json!({"path":output,"width":capture.meta.width,"height":capture.meta.height,"frame_id":capture.meta.frame_id,"session":session.id()}),
                cli.json,
            )?;
        }
        Command::Input {
            command: InputCommand::Tap { x, y, hold_ms },
        } => {
            session.tap(*x, *y, *hold_ms).await?;
            emit(
                &serde_json::json!({"accepted":true,"x":x,"y":y,"hold_ms":hold_ms,"session":session.id()}),
                cli.json,
            )?;
        }
        Command::Crash {
            command: CrashCommand::Show,
        } => emit(&session.crash().await?, cli.json)?,
        Command::Diagnose(args) => diagnose(&session, &endpoint, args, cli.json).await?,
        _ => unreachable!(),
    }
    session.close();
    Ok(())
}

fn select_device(requested: Option<&str>) -> Result<String> {
    if let Some(device) = requested {
        return Ok(device.into());
    }
    let ports = escope_host::devices()?;
    match ports.as_slice() {
        [one] => Ok(one.port.clone()),
        [] => bail!(
            "no serial device found; pass --device tcp://127.0.0.1:9337 for the reference device"
        ),
        _ => bail!("multiple serial devices found; select one with --device (see escope devices)"),
    }
}

fn new_file_path(path: &Path) -> Result<&Path> {
    if path.exists() {
        bail!("output already exists: {}", path.display());
    }
    Ok(path)
}

async fn diagnose(
    session: &Session,
    endpoint: &str,
    args: &DiagnoseArgs,
    json: bool,
) -> Result<()> {
    std::fs::create_dir(&args.output)
        .context("diagnostic output must be a new directory with an existing parent")?;
    let mut report = serde_json::json!({
        "schema":1, "endpoint":endpoint, "captured_at_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
        "hello":session.hello(), "capabilities":capability_names(session.hello().capabilities), "info":session.info().await?,
    });
    let mut issues = Vec::<String>::new();
    if session.supports(capability::METRICS) {
        match session.metrics().await {
            Ok(value) => report["metrics"] = serde_json::to_value(value)?,
            Err(e) => issues.push(format!("metrics: {e}")),
        }
    }
    if session.supports(capability::UI_TREE) {
        match session.ui_tree().await {
            Ok(value) => report["ui_tree"] = serde_json::to_value(value)?,
            Err(e) => issues.push(format!("ui_tree: {e}")),
        }
    }
    if session.supports(capability::CRASH) {
        match session.crash().await {
            Ok(value) => report["crash"] = serde_json::to_value(value)?,
            Err(e) => issues.push(format!("crash: {e}")),
        }
    }
    if session.supports(capability::CAPTURE) {
        match session.capture().await {
            Ok(capture) => {
                capture.save_png(args.output.join("screen.png"))?;
                report["display"] = serde_json::json!({"path":"screen.png","width":capture.meta.width,"height":capture.meta.height,"frame_id":capture.meta.frame_id});
            }
            Err(e) => issues.push(format!("display: {e}")),
        }
    }
    if session.supports(capability::LOGS) {
        let mut events = session.subscribe();
        session.start_logs(5).await?;
        let mut logs = Vec::new();
        for _ in 0..args.logs {
            match tokio::time::timeout(Duration::from_secs(3), events.recv()).await {
                Ok(Ok(event)) => logs.push(event.log()?),
                result => {
                    issues.push(format!("logs: {result:?}"));
                    break;
                }
            }
        }
        session.stop_logs().await?;
        report["logs"] = serde_json::to_value(logs)?;
    }
    report["corrupt_frames"] = session.corrupt_frames().into();
    report["complete"] = issues.is_empty().into();
    report["issues"] = serde_json::to_value(&issues)?;
    std::fs::write(
        args.output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    emit(
        &serde_json::json!({"path":args.output,"complete":issues.is_empty(),"issues":issues}),
        json,
    )
}
