//! Allocation-free framing and typed payloads for the EmbedScope protocol.
#![no_std]
#![forbid(unsafe_code)]

mod frame;
mod payload;

pub use frame::{decode_frame, encode_frame, FrameDecoder};
pub use minicbor;
pub use payload::*;

/// Protocol major. Optional map fields may be added within this major.
pub const VERSION: u16 = 1;
pub const MAX_PAYLOAD: usize = 1024;
/// Maximum CBOR envelope bytes: payload plus all fixed-width map fields.
pub const MAX_ENVELOPE: usize = MAX_PAYLOAD + 64;
pub const MAX_PACKET: usize = MAX_ENVELOPE + 4;
/// COBS upper bound, including the trailing zero delimiter.
pub const MAX_FRAME: usize = MAX_PACKET + MAX_PACKET / 254 + 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Capacity,
    Malformed,
    Checksum,
    Version,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Capacity => "protocol buffer capacity exceeded",
            Self::Malformed => "malformed protocol message",
            Self::Checksum => "protocol checksum mismatch",
            Self::Version => "unsupported protocol version",
        })
    }
}

impl core::error::Error for Error {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, minicbor::Encode, minicbor::Decode)]
#[cbor(index_only)]
pub enum Kind {
    #[n(0)]
    Request,
    #[n(1)]
    Response,
    #[n(2)]
    Event,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, minicbor::Encode, minicbor::Decode)]
#[cbor(index_only)]
pub enum Status {
    #[n(0)]
    Ok,
    #[n(1)]
    InvalidArgument,
    #[n(2)]
    Unsupported,
    #[n(3)]
    Session,
    #[n(4)]
    Busy,
    #[n(5)]
    Stale,
    #[n(6)]
    Capacity,
    #[n(7)]
    Internal,
    #[n(8)]
    Version,
    #[n(9)]
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum Operation {
    Hello = 1,
    Info = 2,
    Metrics = 3,
    LogsSubscribe = 4,
    LogsUnsubscribe = 5,
    UiTree = 6,
    InputTap = 7,
    CaptureStart = 8,
    CaptureRead = 9,
    CaptureEnd = 10,
    Crash = 11,
    Cancel = 12,
    Log = 13,
}

impl From<Operation> for u16 {
    fn from(value: Operation) -> Self {
        value as u16
    }
}

impl TryFrom<u16> for Operation {
    type Error = Status;
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => Self::Hello,
            2 => Self::Info,
            3 => Self::Metrics,
            4 => Self::LogsSubscribe,
            5 => Self::LogsUnsubscribe,
            6 => Self::UiTree,
            7 => Self::InputTap,
            8 => Self::CaptureStart,
            9 => Self::CaptureRead,
            10 => Self::CaptureEnd,
            11 => Self::Crash,
            12 => Self::Cancel,
            13 => Self::Log,
            _ => return Err(Status::Unsupported),
        })
    }
}

/// Device capability bits. Operations remain separately versioned identifiers.
pub mod capability {
    pub const LOGS: u32 = 1 << 0;
    pub const METRICS: u32 = 1 << 1;
    pub const UI_TREE: u32 = 1 << 2;
    pub const INPUT: u32 = 1 << 3;
    pub const CAPTURE: u32 = 1 << 4;
    pub const CRASH: u32 = 1 << 5;
}

/// All fields have stable integer map keys; payload is a nested CBOR byte string.
#[derive(Clone, Copy, Debug, Eq, PartialEq, minicbor::Encode, minicbor::Decode)]
#[cbor(map)]
pub struct Envelope<'a> {
    #[n(0)]
    pub version: u16,
    #[n(1)]
    pub kind: Kind,
    #[n(2)]
    pub session: u32,
    #[n(3)]
    pub id: u32,
    #[n(4)]
    pub op: u16,
    #[n(5)]
    pub status: Status,
    #[b(6)]
    #[cbor(with = "minicbor::bytes")]
    pub payload: &'a [u8],
}

pub fn encode_payload<T: minicbor::Encode<()>>(value: &T, out: &mut [u8]) -> Result<usize, Error> {
    let mut encoder = minicbor::Encoder::new(minicbor::encode::write::Cursor::new(out));
    encoder.encode(value).map_err(|_| Error::Capacity)?;
    Ok(encoder.writer().position())
}

/// Decode exactly one value. Trailing bytes are rejected.
pub fn decode_payload<'a, T: minicbor::Decode<'a, ()>>(data: &'a [u8]) -> Result<T, Error> {
    let mut decoder = minicbor::Decoder::new(data);
    let value = decoder.decode().map_err(|_| Error::Malformed)?;
    if decoder.position() != data.len() {
        return Err(Error::Malformed);
    }
    Ok(value)
}
