use crate::{Error, Result};
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_serial::SerialPortBuilderExt;

pub(crate) trait Transport: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Transport for T {}

#[derive(Clone, Debug, Serialize)]
pub struct PortInfo {
    pub port: String,
    pub transport: &'static str,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
    pub usb_vid: Option<u16>,
    pub usb_pid: Option<u16>,
    /// CDC communication interface on Linux/Windows, data interface on macOS.
    pub usb_interface: Option<u8>,
}

/// Enumerate serial ports without opening them or changing their line state.
pub fn devices() -> Result<Vec<PortInfo>> {
    tokio_serial::available_ports()
        .map_err(|e| Error::Io(e.to_string()))
        .map(|ports| {
            ports
                .into_iter()
                .map(|p| {
                    let (manufacturer, product, serial_number, usb_vid, usb_pid, usb_interface) =
                        match p.port_type {
                            tokio_serial::SerialPortType::UsbPort(usb) => (
                                usb.manufacturer,
                                usb.product,
                                usb.serial_number,
                                Some(usb.vid),
                                Some(usb.pid),
                                usb.interface,
                            ),
                            _ => (None, None, None, None, None, None),
                        };
                    PortInfo {
                        port: p.port_name,
                        transport: "serial",
                        manufacturer,
                        product,
                        serial_number,
                        usb_vid,
                        usb_pid,
                        usb_interface,
                    }
                })
                .collect()
        })
}

pub(crate) async fn open(
    endpoint: &str,
    baud: u32,
    dtr: Option<bool>,
) -> Result<Box<dyn Transport>> {
    if let Some(address) = endpoint.strip_prefix("tcp://") {
        let stream = tokio::net::TcpStream::connect(address).await?;
        stream.set_nodelay(true)?;
        Ok(Box::new(stream))
    } else {
        let path = endpoint.strip_prefix("serial://").unwrap_or(endpoint);
        let builder = tokio_serial::new(path, baud);
        let builder = match dtr {
            Some(value) => builder.dtr_on_open(value),
            None => builder,
        };
        // serialport opens Unix ports exclusively by default. A second
        // TIOCEXCL on the same descriptor returns EBUSY on macOS.
        let stream = builder
            .open_native_async()
            .map_err(|e| Error::Io(e.to_string()))?;
        Ok(Box::new(stream))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use tokio_serial::SerialPort;

    #[tokio::test]
    async fn serial_opens_exclusively_and_reopens_after_close() {
        let (_master, slave) = tokio_serial::SerialStream::pair().unwrap();
        let path = slave.name().unwrap();
        drop(slave);
        // Baud zero avoids hardware speed ioctls on a pseudo terminal.
        let first = open(&path, 0, None).await.unwrap();
        assert!(open(&path, 0, None).await.is_err());
        drop(first);
        assert!(open(&path, 0, None).await.is_ok());
    }
}
