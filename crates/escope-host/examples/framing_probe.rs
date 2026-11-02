//! Exercise fragmented, corrupt, and oversized frames over a physical CDC port.
use escope_protocol::{
    decode_frame, encode_frame, Envelope, FrameDecoder, Kind, Operation, Status, MAX_FRAME, VERSION,
};
use std::{error::Error, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::SerialPortBuilderExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 1 {
        return Err("usage: framing_probe SERIAL_PORT".into());
    }
    let mut port = tokio_serial::new(&args[0], 115200)
        .dtr_on_open(true)
        .open_native_async()?;
    let mut decoder = FrameDecoder::new();
    let mut scratch = [0; MAX_FRAME];
    let mut tx = [0; MAX_FRAME];
    let hello = Envelope {
        version: VERSION,
        kind: Kind::Request,
        session: 0,
        id: 17,
        op: Operation::Hello.into(),
        status: Status::Ok,
        payload: &[],
    };
    let len = encode_frame(&hello, &mut tx)?;
    let corrupt = (1..len - 1)
        .find_map(|index| {
            let mut candidate = tx[..len].to_vec();
            candidate[index] ^= 0x40;
            matches!(
                decode_frame(&candidate[..len - 1], &mut scratch),
                Err(escope_protocol::Error::Checksum)
            )
            .then_some(candidate)
        })
        .ok_or("unable to construct a frame with an invalid CRC")?;
    port.write_all(&[4, 0x11, 0x22, 0]).await?;
    port.write_all(&vec![0xaa; MAX_FRAME + 64]).await?;
    port.write_all(&[0]).await?;
    port.write_all(&corrupt).await?;
    for part in tx[..len].chunks(2) {
        port.write_all(part).await?;
        tokio::time::sleep(Duration::from_millis(3)).await;
    }
    let mut rx = [0; 512];
    let mut session = 0;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let n = port.read(&mut rx).await?;
            if n == 0 {
                return Err::<(), Box<dyn Error>>("CDC closed".into());
            }
            for byte in &rx[..n] {
                if let Some(Ok(frame)) = decoder.push(*byte) {
                    let response = decode_frame(frame, &mut scratch)?;
                    if response.id != 17 || response.status != Status::Ok || response.session == 0 {
                        return Err("unexpected handshake response".into());
                    }
                    session = response.session;
                    return Ok(());
                }
            }
        }
    })
    .await??;
    let stale = Envelope {
        version: VERSION,
        kind: Kind::Request,
        session: session.wrapping_add(1),
        id: 18,
        op: Operation::Metrics.into(),
        status: Status::Ok,
        payload: &[],
    };
    let len = encode_frame(&stale, &mut tx)?;
    port.write_all(&tx[..len]).await?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let n = port.read(&mut rx).await?;
            if n == 0 {
                return Err::<(), Box<dyn Error>>("CDC closed".into());
            }
            for byte in &rx[..n] {
                if let Some(Ok(frame)) = decoder.push(*byte) {
                    let response = decode_frame(frame, &mut scratch)?;
                    if response.id == 18 && response.status == Status::Session {
                        return Ok(());
                    }
                    return Err("stale session was not rejected".into());
                }
            }
        }
    })
    .await??;
    println!(
        "{}",
        serde_json::json!({ "passed": true, "physical_device": true, "endpoint": args[0], "session": session, "checks": ["fragmented hello", "malformed COBS recovery", "oversized frame recovery", "corrupt CRC recovery", "stale session rejection"] })
    );
    Ok(())
}
