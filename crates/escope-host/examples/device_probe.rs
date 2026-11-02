//! Device acceptance through the public SDK. Supply a dedicated serial endpoint
//! and a new output directory. Optional x/y arguments exercise one authorized tap.
use escope_host::{protocol as p, ConnectOptions, Error, Session};
use serde::Serialize;
use serde_json::json;
use std::{error::Error as StdError, path::Path, time::Duration};

type Result<T> = std::result::Result<T, Box<dyn StdError>>;

fn check(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn save(path: &Path, value: &impl Serialize) -> Result<()> {
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn payload(value: &impl p::minicbor::Encode<()>) -> Result<Vec<u8>> {
    let mut bytes = [0; p::MAX_PAYLOAD];
    let n = p::encode_payload(value, &mut bytes)?;
    Ok(bytes[..n].to_vec())
}

async fn connect(endpoint: &str) -> Result<Session> {
    Ok(Session::connect_with_options(
        endpoint,
        ConnectOptions {
            dtr: Some(true),
            timeout: Duration::from_secs(5),
            ..ConnectOptions::default()
        },
    )
    .await?)
}

async fn probe(endpoint: &str, out: &Path, tap: Option<(u32, u32)>) -> Result<()> {
    let session = connect(endpoint).await?;
    let identity = session.info().await?;
    save(&out.join("identity.json"), &identity)?;
    save(&out.join("hello.json"), session.hello())?;
    check(
        identity.model != "software / synthetic",
        "expected a physical device",
    )?;
    check(
        session.supports(63),
        "all six diagnostic capabilities are required",
    )?;

    let mut events = session.subscribe();
    session.start_logs(5).await?;
    let log_task = tokio::spawn(async move {
        let mut logs = Vec::new();
        let mut gaps = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        loop {
            match tokio::time::timeout_at(deadline, events.recv()).await {
                Ok(Ok(event)) => {
                    if let Ok(log) = event.log() {
                        logs.push(log);
                    }
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => gaps.push(n),
                _ => break,
            }
        }
        json!({ "records": logs, "host_gaps": gaps })
    });

    let (metrics, tree, crash) =
        tokio::join!(session.metrics(), session.ui_tree(), session.crash());
    save(&out.join("metrics-before.json"), &metrics?)?;
    let tree = tree?;
    check(!tree.is_empty(), "empty UI tree")?;
    save(&out.join("tree-before.json"), &tree)?;
    save(&out.join("crash.json"), &crash?)?;

    let start = session
        .request_raw(p::Operation::CaptureStart.into(), &[])
        .await?;
    let meta: p::CaptureMeta = p::decode_payload(&start)?;
    check(
        meta.width == 480 && meta.height == 480,
        "unexpected panel dimensions",
    )?;
    let query = payload(&p::CaptureRead {
        capture_id: meta.capture_id,
        offset: 0,
        length: 768,
    })?;
    let first = session
        .request_raw(p::Operation::CaptureRead.into(), &query)
        .await?;
    let rejected = session
        .request_raw(p::Operation::CaptureStart.into(), &[])
        .await;
    check(
        matches!(rejected, Err(Error::Device(p::Status::Busy))),
        "second capture must return Busy",
    )?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    let second = session
        .request_raw(p::Operation::CaptureRead.into(), &query)
        .await?;
    let first: p::CaptureChunk<'_> = p::decode_payload(&first)?;
    let second: p::CaptureChunk<'_> = p::decode_payload(&second)?;
    check(
        first.data == second.data,
        "capture pixels changed while leased",
    )?;
    session
        .request_raw(
            p::Operation::CaptureEnd.into(),
            &payload(&p::CaptureEnd {
                capture_id: meta.capture_id,
            })?,
        )
        .await?;
    save(
        &out.join("fixed-frame.json"),
        &json!({
            "capture_id": meta.capture_id, "frame_id": meta.frame_id,
            "width": meta.width, "height": meta.height, "bytes": meta.total_bytes,
            "repeated_chunk_identical": true, "second_start_busy": true,
        }),
    )?;

    let capture = session.capture().await?;
    capture.save_png(out.join("before.png"))?;
    if let Some((x, y)) = tap {
        session.tap(x, y, 80).await?;
        tokio::time::sleep(Duration::from_millis(400)).await;
        let after_tree = session.ui_tree().await?;
        save(&out.join("tree-after.json"), &after_tree)?;
        let after = session.capture().await?;
        after.save_png(out.join("after.png"))?;
        let changed = capture
            .rgb
            .as_chunks::<3>()
            .0
            .iter()
            .zip(after.rgb.as_chunks::<3>().0)
            .filter(|(a, b)| a != b)
            .count();
        save(
            &out.join("input.json"),
            &json!({
                "x": x, "y": y, "hold_ms": 80, "changed_pixels": changed,
                "before_revision": tree[0].revision,
                "after_revision": after_tree.first().map(|node| node.revision),
                "before_frame": capture.meta.frame_id, "after_frame": after.meta.frame_id,
            }),
        )?;
    }

    check(
        matches!(
            session.tap(u32::MAX, 0, 60).await,
            Err(Error::Device(p::Status::InvalidArgument))
        ),
        "invalid coordinate was accepted",
    )?;
    check(
        matches!(
            session.request_raw(u16::MAX, &[]).await,
            Err(Error::Device(p::Status::Unsupported))
        ),
        "unknown operation was accepted",
    )?;
    let logs = log_task.await?;
    save(&out.join("logs.json"), &logs)?;
    check(
        logs["records"]
            .as_array()
            .is_some_and(|records| !records.is_empty()),
        "no structured logs received from the device",
    )?;
    session.stop_logs().await?;
    save(&out.join("metrics-after.json"), &session.metrics().await?)?;

    // Disconnect with an active frame and unfinished hold to exercise reset.
    session
        .request_raw(p::Operation::CaptureStart.into(), &[])
        .await?;
    session.tap(0, 0, 5000).await?;
    let first_session = session.id();
    session.close();
    drop(session);
    tokio::time::sleep(Duration::from_millis(250)).await;
    let reconnected = connect(endpoint).await?;
    let next_identity = reconnected.info().await?;
    check(
        next_identity.boot_id == identity.boot_id,
        "device rebooted during probe",
    )?;
    check(
        reconnected.id() != first_session,
        "session was reused after reconnect",
    )?;
    // A stale pending hold would reject this tap as Busy for another 5 seconds.
    reconnected.tap(0, 0, 60).await?;
    reconnected
        .capture()
        .await?
        .save_png(out.join("reconnected.png"))?;
    save(
        &out.join("reconnect.json"),
        &json!({
            "old_session": first_session, "new_session": reconnected.id(),
            "same_boot": true, "capture_released_by_reset": true,
            "unfinished_hold_released_by_reset": true,
            "corrupt_frames_seen_by_host": reconnected.corrupt_frames(),
        }),
    )?;
    reconnected.close();
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    check(
        args.len() == 2 || args.len() == 4,
        "usage: device_probe ENDPOINT NEW_OUTPUT_DIRECTORY [TAP_X TAP_Y]",
    )?;
    let out = Path::new(&args[1]);
    std::fs::create_dir(out)?;
    let tap = if args.len() == 4 {
        Some((args[2].parse()?, args[3].parse()?))
    } else {
        None
    };
    let result = probe(&args[0], out, tap).await;
    save(
        &out.join("receipt.json"),
        &json!({
            "physical_device": true, "endpoint": args[0], "passed": result.is_ok(),
            "error": result.as_ref().err().map(ToString::to_string),
        }),
    )?;
    result
}
