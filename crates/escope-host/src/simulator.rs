//! A deterministic software reference device. Its pixels, metrics and crash
//! summary are synthetic; it exercises the same Agent and framing as hardware.
use crate::{Error, Result};
use escope_agent::{Agent, Backend};
use escope_protocol::{self as p, capability, decode_payload, encode_payload, Operation, Status};
use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinSet,
};

pub const WIDTH: u32 = 320;
pub const HEIGHT: u32 = 240;

pub async fn serve(listener: TcpListener, mut shutdown: watch::Receiver<bool>) -> Result<()> {
    let mut clients = JoinSet::new();
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u32;
    let sequence = AtomicU32::new(seed.max(1));
    let counter = Arc::new(AtomicU32::new(0));
    let started = Instant::now();
    loop {
        tokio::select! {
            changed = shutdown.changed() => { if changed.is_err() || *shutdown.borrow() { break; } }
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let id = sequence.fetch_add(1, Ordering::Relaxed).max(1);
                clients.spawn(serve_connection(stream, id, seed.max(1), started, counter.clone()));
            }
            Some(_) = clients.join_next(), if !clients.is_empty() => {}
        }
    }
    clients.abort_all();
    while clients.join_next().await.is_some() {}
    Ok(())
}

async fn serve_connection(
    mut stream: TcpStream,
    id: u32,
    boot_id: u32,
    started: Instant,
    counter: Arc<AtomicU32>,
) -> Result<()> {
    stream.set_nodelay(true)?;
    let mut agent = Agent::new(id);
    let mut device = ReferenceDevice::new(boot_id);
    device.started = started;
    device.counter = counter;
    let mut decoder = p::FrameDecoder::new();
    let mut scratch = [0; p::MAX_FRAME];
    let mut outgoing = [0; p::MAX_FRAME];
    let mut bytes = [0; 2048];
    let mut ticker = tokio::time::interval(Duration::from_millis(250));
    let result = async {
        loop {
            tokio::select! {
                read = stream.read(&mut bytes) => {
                    let n = read?;
                    if n == 0 { return Ok(()); }
                    for &byte in &bytes[..n] {
                        if let Some(Ok(frame)) = decoder.push(byte) {
                            if let Ok(request) = p::decode_frame(frame, &mut scratch) {
                                let n = agent.handle(request, &mut device, &mut outgoing).map_err(|e| Error::Protocol(e.to_string()))?;
                                stream.write_all(&outgoing[..n]).await?;
                                if device.take_clicked() {
                                    let message = format!("reference input: counter={}", device.count());
                                    let _ = agent.push_log(device.uptime(), 3, "reference.input", &message);
                                }
                            }
                        }
                    }
                }
                _ = ticker.tick() => {
                    let message = format!("synthetic heartbeat; counter={}", device.count());
                    let _ = agent.push_log(device.uptime(), 3, "reference.device", &message);
                }
            }
            // Limit event work per iteration; control packets retain a turn.
            for _ in 0..4 {
                match agent.poll_event(&mut outgoing).map_err(|e| Error::Protocol(e.to_string()))? {
                    Some(n) => stream.write_all(&outgoing[..n]).await?,
                    None => break,
                }
            }
        }
    }.await;
    agent.reset(&mut device);
    result
}

struct Snapshot {
    id: u32,
    frame: u64,
    pixels: Vec<u8>,
}

pub struct ReferenceDevice {
    boot_id: u32,
    started: Instant,
    counter: Arc<AtomicU32>,
    next_capture: u32,
    capture: Option<Snapshot>,
    clicked: bool,
}

impl ReferenceDevice {
    pub fn new(boot_id: u32) -> Self {
        Self {
            boot_id,
            started: Instant::now(),
            counter: Arc::new(AtomicU32::new(0)),
            next_capture: 1,
            capture: None,
            clicked: false,
        }
    }
    fn count(&self) -> u32 {
        self.counter.load(Ordering::Acquire)
    }
    fn revision(&self) -> u32 {
        self.count().wrapping_add(1).max(1)
    }
    fn uptime(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
    fn take_clicked(&mut self) -> bool {
        std::mem::take(&mut self.clicked)
    }
    fn reply<T: p::minicbor::Encode<()>>(
        value: &T,
        out: &mut [u8],
    ) -> std::result::Result<usize, Status> {
        encode_payload(value, out).map_err(|_| Status::Capacity)
    }
}

impl Backend for ReferenceDevice {
    fn capabilities(&self) -> u32 {
        capability::LOGS
            | capability::METRICS
            | capability::UI_TREE
            | capability::INPUT
            | capability::CAPTURE
            | capability::CRASH
    }
    fn info(&self) -> p::DeviceInfo<'_> {
        p::DeviceInfo {
            name: "EmbedScope reference",
            model: "software / synthetic",
            firmware: env!("CARGO_PKG_VERSION"),
            build_id: "reference-v1",
            boot_id: self.boot_id,
            uptime_ms: self.uptime(),
        }
    }
    fn reset(&mut self) {
        self.capture = None;
        self.clicked = false;
    }
    fn handle(
        &mut self,
        op: u16,
        payload: &[u8],
        out: &mut [u8],
    ) -> std::result::Result<usize, Status> {
        match Operation::try_from(op)? {
            Operation::Metrics => Self::reply(
                &p::Metrics {
                    uptime_ms: self.uptime(),
                    heap_free: Some(196_608 - self.count() as u64 % 100 * 8),
                    heap_min_free: Some(180_224),
                    heap_largest_free: Some(131_072),
                    psram_free: None,
                    frame_ms: Some(16),
                    queue_depth: Some(0),
                    dropped_logs: 0,
                },
                out,
            ),
            Operation::UiTree => {
                let query: p::TreeQuery =
                    decode_payload(payload).map_err(|_| Status::InvalidArgument)?;
                if query.revision != 0 && query.revision != self.revision() {
                    return Err(Status::Stale);
                }
                let label = format!("Counter: {}", self.count());
                let revision = self.revision();
                let (id, parent, kind, name, x, y, width, height, next_cursor) = match query.cursor
                {
                    0 => (
                        1,
                        0,
                        "screen",
                        "Reference / synthetic",
                        0,
                        0,
                        WIDTH,
                        HEIGHT,
                        Some(1),
                    ),
                    1 => (2, 1, "label", label.as_str(), 32, 70, 256, 40, Some(2)),
                    2 => (3, 1, "button", "Increment", 32, 120, 256, 56, None),
                    _ => return Err(Status::InvalidArgument),
                };
                Self::reply(
                    &p::UiNode {
                        id,
                        parent,
                        kind,
                        name,
                        x,
                        y,
                        width,
                        height,
                        flags: 0,
                        revision,
                        next_cursor,
                    },
                    out,
                )
            }
            Operation::InputTap => {
                let tap: p::InputTap =
                    decode_payload(payload).map_err(|_| Status::InvalidArgument)?;
                if tap.x >= WIDTH || tap.y >= HEIGHT || !(1..=5000).contains(&tap.hold_ms) {
                    return Err(Status::InvalidArgument);
                }
                if (32..288).contains(&tap.x) && (120..176).contains(&tap.y) {
                    self.counter.fetch_add(1, Ordering::AcqRel);
                    self.clicked = true;
                }
                Ok(0)
            }
            Operation::CaptureStart => {
                if self.capture.is_some() {
                    return Err(Status::Busy);
                }
                let id = self.next_capture;
                self.next_capture = self.next_capture.wrapping_add(1).max(1);
                let pixels = render(self.count());
                let meta = p::CaptureMeta {
                    capture_id: id,
                    width: WIDTH,
                    height: HEIGHT,
                    stride: WIDTH * 3,
                    format: p::PixelFormat::Rgb888,
                    total_bytes: pixels.len() as u32,
                    frame_id: self.revision() as u64,
                };
                let n = Self::reply(&meta, out)?;
                self.capture = Some(Snapshot {
                    id,
                    frame: meta.frame_id,
                    pixels,
                });
                Ok(n)
            }
            Operation::CaptureRead => {
                let query: p::CaptureRead =
                    decode_payload(payload).map_err(|_| Status::InvalidArgument)?;
                let snapshot = self
                    .capture
                    .as_ref()
                    .filter(|c| c.id == query.capture_id)
                    .ok_or(Status::Stale)?;
                if query.length == 0
                    || query.length > 768
                    || query.offset as usize >= snapshot.pixels.len()
                {
                    return Err(Status::InvalidArgument);
                }
                let start = query.offset as usize;
                let end = (start + query.length as usize).min(snapshot.pixels.len());
                let _frame = snapshot.frame;
                Self::reply(
                    &p::CaptureChunk {
                        capture_id: snapshot.id,
                        offset: query.offset,
                        data: &snapshot.pixels[start..end],
                        done: end == snapshot.pixels.len(),
                    },
                    out,
                )
            }
            Operation::CaptureEnd => {
                let query: p::CaptureEnd =
                    decode_payload(payload).map_err(|_| Status::InvalidArgument)?;
                if self.capture.as_ref().map(|c| c.id) != Some(query.capture_id) {
                    return Err(Status::Stale);
                }
                self.capture = None;
                Ok(0)
            }
            Operation::Cancel => {
                self.capture = None;
                Ok(0)
            }
            Operation::Crash => Self::reply(
                &p::CrashInfo {
                    present: false,
                    reason: "reference device",
                    build_id: "reference-v1",
                    message: "No physical crash record; software reference device",
                    boot_id: self.boot_id,
                },
                out,
            ),
            _ => Err(Status::Unsupported),
        }
    }
}

fn render(counter: u32) -> Vec<u8> {
    let mut pixels = vec![0; (WIDTH * HEIGHT * 3) as usize];
    rect(&mut pixels, 0, 0, WIDTH, HEIGHT, [239, 240, 232]);
    rect(&mut pixels, 0, 0, WIDTH, 46, [27, 43, 42]);
    text(&mut pixels, 20, 15, "EMBEDSCOPE", [245, 247, 240], 2);
    text(&mut pixels, 32, 64, "REFERENCE DEVICE", [85, 104, 95], 1);
    text(
        &mut pixels,
        32,
        86,
        &format!("COUNTER {counter}"),
        [27, 43, 42],
        3,
    );
    rect(&mut pixels, 32, 120, 256, 56, [32, 104, 89]);
    text(&mut pixels, 56, 140, "TAP TO INCREMENT", [245, 247, 240], 2);
    text(
        &mut pixels,
        32,
        208,
        "SOFTWARE / SYNTHETIC",
        [85, 104, 95],
        1,
    );
    // A machine-visible change for every input, including counter wraparound.
    rect(&mut pixels, 32, 188, 16 + counter % 240, 5, [185, 78, 48]);
    pixels
}

fn rect(p: &mut [u8], x: u32, y: u32, w: u32, h: u32, color: [u8; 3]) {
    for row in y..(y + h).min(HEIGHT) {
        for col in x..(x + w).min(WIDTH) {
            let i = ((row * WIDTH + col) * 3) as usize;
            p[i..i + 3].copy_from_slice(&color);
        }
    }
}

fn text(p: &mut [u8], x: u32, y: u32, s: &str, color: [u8; 3], scale: u32) {
    for (index, ch) in s.chars().enumerate() {
        let rows: [u8; 7] = match ch {
            'A' => [14, 17, 17, 31, 17, 17, 17],
            'B' => [30, 17, 17, 30, 17, 17, 30],
            'C' => [14, 17, 16, 16, 16, 17, 14],
            'D' => [30, 17, 17, 17, 17, 17, 30],
            'E' => [31, 16, 16, 30, 16, 16, 31],
            'F' => [31, 16, 16, 30, 16, 16, 16],
            'G' => [14, 17, 16, 23, 17, 17, 15],
            'H' => [17, 17, 17, 31, 17, 17, 17],
            'I' => [14, 4, 4, 4, 4, 4, 14],
            'J' => [7, 2, 2, 2, 2, 18, 12],
            'K' => [17, 18, 20, 24, 20, 18, 17],
            'L' => [16, 16, 16, 16, 16, 16, 31],
            'M' => [17, 27, 21, 21, 17, 17, 17],
            'N' => [17, 25, 21, 19, 17, 17, 17],
            'O' => [14, 17, 17, 17, 17, 17, 14],
            'P' => [30, 17, 17, 30, 16, 16, 16],
            'Q' => [14, 17, 17, 17, 21, 18, 13],
            'R' => [30, 17, 17, 30, 20, 18, 17],
            'S' => [15, 16, 16, 14, 1, 1, 30],
            'T' => [31, 4, 4, 4, 4, 4, 4],
            'U' => [17, 17, 17, 17, 17, 17, 14],
            'V' => [17, 17, 17, 17, 17, 10, 4],
            'W' => [17, 17, 17, 21, 21, 21, 10],
            'X' => [17, 17, 10, 4, 10, 17, 17],
            'Y' => [17, 17, 10, 4, 4, 4, 4],
            'Z' => [31, 1, 2, 4, 8, 16, 31],
            '0' => [14, 17, 19, 21, 25, 17, 14],
            '1' => [4, 12, 4, 4, 4, 4, 14],
            '2' => [14, 17, 1, 2, 4, 8, 31],
            '3' => [30, 1, 1, 14, 1, 1, 30],
            '4' => [2, 6, 10, 18, 31, 2, 2],
            '5' => [31, 16, 16, 30, 1, 1, 30],
            '6' => [14, 16, 16, 30, 17, 17, 14],
            '7' => [31, 1, 2, 4, 8, 8, 8],
            '8' => [14, 17, 17, 14, 17, 17, 14],
            '9' => [14, 17, 17, 15, 1, 1, 14],
            '/' => [1, 1, 2, 4, 8, 16, 16],
            _ => [0; 7],
        };
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    rect(
                        p,
                        x + index as u32 * 6 * scale + col * scale,
                        y + row as u32 * scale,
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
    }
}
