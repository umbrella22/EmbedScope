//! Capture a device while a command on its separate text console changes a page.
//! The console line is supplied explicitly; this example is for device acceptance.
use escope_host::{ConnectOptions, Session};
use serde_json::json;
use std::{error::Error, path::Path, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::SerialPortBuilderExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 5 {
        return Err("usage: console_capture AGENT_PORT CONSOLE_PORT NEW_OUTPUT_DIR CONSOLE_LINE CAPTURE_DELAY_MS".into());
    }
    let delay = args[4].parse::<u64>()?;
    let out = Path::new(&args[2]);
    std::fs::create_dir(out)?;
    let session = Session::connect_with_options(
        &args[0],
        ConnectOptions {
            dtr: Some(true),
            timeout: Duration::from_secs(5),
            ..Default::default()
        },
    )
    .await?;
    let identity = session.info().await?;
    let mut events = session.subscribe();
    session.start_logs(5).await?;
    let logs = tokio::spawn(async move {
        let mut records = Vec::new();
        let until = tokio::time::Instant::now() + Duration::from_secs(12);
        loop {
            match tokio::time::timeout_at(until, events.recv()).await {
                Ok(Ok(event)) => {
                    if let Ok(log) = event.log() {
                        records.push(log);
                    }
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
                _ => break,
            }
        }
        records
    });
    let mut console = tokio_serial::new(&args[1], 115200)
        .dtr_on_open(true)
        .open_native_async()?;
    let start = tokio::time::Instant::now();
    console.write_all(args[3].as_bytes()).await?;
    console.write_all(b"\n").await?;
    let console_rx = tokio::spawn(async move {
        let mut bytes = Vec::new();
        let mut buf = [0; 1024];
        let until = tokio::time::Instant::now() + Duration::from_secs(2);
        while bytes.len() < 64 * 1024 {
            match tokio::time::timeout_at(until, console.read(&mut buf)).await {
                Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
                Ok(Ok(n)) => bytes.extend_from_slice(&buf[..n]),
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    });
    tokio::time::sleep(Duration::from_millis(delay)).await;
    let requested_at_ms = start.elapsed().as_millis();
    let frame = session.capture().await?;
    frame.save_png(out.join("screen.png"))?;
    let tree = session.ui_tree().await?;
    std::fs::write(out.join("tree.json"), serde_json::to_vec_pretty(&tree)?)?;
    std::fs::write(out.join("console.log"), console_rx.await?)?;
    std::fs::write(
        out.join("logs.json"),
        serde_json::to_vec_pretty(&logs.await?)?,
    )?;
    session.stop_logs().await?;
    let receipt = json!({
        "physical_device": identity.model != "software / synthetic",
        "identity": identity, "session": session.hello(),
        "console_line": args[3], "requested_at_ms": requested_at_ms,
        "capture": {
            "capture_id": frame.meta.capture_id, "frame_id": frame.meta.frame_id,
            "width": frame.meta.width, "height": frame.meta.height,
            "stride": frame.meta.stride, "total_bytes": frame.meta.total_bytes,
        },
        "completed_at_ms": start.elapsed().as_millis(),
    });
    std::fs::write(
        out.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    println!("{}", serde_json::to_string(&receipt)?);
    session.close();
    Ok(())
}
