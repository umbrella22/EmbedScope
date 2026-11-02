//! EmbedScope host SDK. A Session multiplexes bounded requests and events over
//! one transport. Device timestamps remain separate from host receipt time.
mod capture;
mod model;
mod session;
pub mod simulator;
mod transport;

pub use capture::Capture;
pub use escope_protocol as protocol;
pub use model::*;
pub use session::{ConnectOptions, Event, Session};
pub use transport::{devices, PortInfo};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O: {0}")]
    Io(String),
    #[error("device disconnected")]
    Disconnected,
    #[error("request timed out")]
    Timeout,
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("device rejected the operation: {0:?}")]
    Device(escope_protocol::Status),
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn decode<'a, T: escope_protocol::minicbor::Decode<'a, ()>>(
    bytes: &'a [u8],
) -> Result<T> {
    escope_protocol::decode_payload(bytes).map_err(|e| Error::Protocol(format!("payload: {e:?}")))
}

pub(crate) fn encode<T: escope_protocol::minicbor::Encode<()>>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = [0; escope_protocol::MAX_PAYLOAD];
    let n = escope_protocol::encode_payload(value, &mut bytes)
        .map_err(|e| Error::Protocol(format!("payload: {e:?}")))?;
    Ok(bytes[..n].to_vec())
}
