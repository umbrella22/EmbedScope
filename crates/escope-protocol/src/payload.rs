use minicbor::{Decode, Encode};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct HelloReply<'a> {
    #[n(0)]
    pub session: u32,
    #[n(1)]
    pub capabilities: u32,
    #[n(2)]
    pub max_payload: u32,
    #[n(3)]
    pub protocol: u16,
    #[b(4)]
    pub name: &'a str,
    #[b(5)]
    pub build_id: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct DeviceInfo<'a> {
    #[b(0)]
    pub name: &'a str,
    #[b(1)]
    pub model: &'a str,
    #[b(2)]
    pub firmware: &'a str,
    #[b(3)]
    pub build_id: &'a str,
    #[n(4)]
    pub boot_id: u32,
    #[n(5)]
    pub uptime_ms: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct Metrics {
    #[n(0)]
    pub uptime_ms: u64,
    #[n(1)]
    pub heap_free: Option<u64>,
    #[n(2)]
    pub heap_min_free: Option<u64>,
    #[n(3)]
    pub heap_largest_free: Option<u64>,
    #[n(4)]
    pub psram_free: Option<u64>,
    #[n(5)]
    pub frame_ms: Option<u32>,
    #[n(6)]
    pub queue_depth: Option<u32>,
    #[n(7)]
    pub dropped_logs: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct TreeQuery {
    #[n(0)]
    pub cursor: u32,
    #[n(1)]
    pub revision: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct UiNode<'a> {
    #[n(0)]
    pub id: u32,
    #[n(1)]
    pub parent: u32,
    #[b(2)]
    pub kind: &'a str,
    #[b(3)]
    pub name: &'a str,
    #[n(4)]
    pub x: i32,
    #[n(5)]
    pub y: i32,
    #[n(6)]
    pub width: u32,
    #[n(7)]
    pub height: u32,
    #[n(8)]
    pub flags: u32,
    #[n(9)]
    pub revision: u32,
    #[n(10)]
    pub next_cursor: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct InputTap {
    #[n(0)]
    pub x: u32,
    #[n(1)]
    pub y: u32,
    #[n(2)]
    pub hold_ms: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Encode, Decode)]
#[cbor(index_only)]
pub enum PixelFormat {
    #[n(1)]
    Rgb565Le,
    #[n(2)]
    Rgb888,
    #[n(3)]
    Rgba8888,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct CaptureMeta {
    #[n(0)]
    pub capture_id: u32,
    #[n(1)]
    pub width: u32,
    #[n(2)]
    pub height: u32,
    #[n(3)]
    pub stride: u32,
    #[n(4)]
    pub format: PixelFormat,
    #[n(5)]
    pub total_bytes: u32,
    #[n(6)]
    pub frame_id: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct CaptureRead {
    #[n(0)]
    pub capture_id: u32,
    #[n(1)]
    pub offset: u32,
    #[n(2)]
    pub length: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct CaptureChunk<'a> {
    #[n(0)]
    pub capture_id: u32,
    #[n(1)]
    pub offset: u32,
    #[b(2)]
    #[cbor(with = "minicbor::bytes")]
    pub data: &'a [u8],
    #[n(3)]
    pub done: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct CaptureEnd {
    #[n(0)]
    pub capture_id: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct Cancel {
    #[n(0)]
    pub request_id: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct LogFilter {
    /// Standard log severity: Error=1, Warn=2, Info=3, Debug=4, Trace=5.
    #[n(0)]
    pub max_level: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct LogRecord<'a> {
    #[n(0)]
    pub seq: u64,
    #[n(1)]
    pub timestamp_ms: u64,
    #[n(2)]
    pub level: u8,
    #[b(3)]
    pub target: &'a str,
    #[b(4)]
    pub message: &'a str,
    #[n(5)]
    pub dropped: u64,
    #[n(6)]
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct CrashInfo<'a> {
    #[n(0)]
    pub present: bool,
    #[b(1)]
    pub reason: &'a str,
    #[b(2)]
    pub build_id: &'a str,
    #[b(3)]
    pub message: &'a str,
    #[n(4)]
    pub boot_id: u32,
}
