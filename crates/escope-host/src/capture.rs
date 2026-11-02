use crate::{decode, encode, Error, Result, Session};
use escope_protocol::{
    capability, CaptureChunk, CaptureEnd, CaptureMeta, CaptureRead, Operation, PixelFormat,
};
use std::{io::BufWriter, path::Path};

/// A validated display frame. `rgb` contains packed RGB888 rows without padding.
#[derive(Clone, Debug)]
pub struct Capture {
    pub meta: CaptureMeta,
    pub rgb: Vec<u8>,
}

impl Capture {
    pub fn save_png(&self, path: impl AsRef<Path>) -> Result<()> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        let mut encoder =
            png::Encoder::new(BufWriter::new(file), self.meta.width, self.meta.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| Error::Io(e.to_string()))?;
        writer
            .write_image_data(&self.rgb)
            .map_err(|e| Error::Io(e.to_string()))?;
        writer.finish().map_err(|e| Error::Io(e.to_string()))
    }

    pub fn from_raw(meta: CaptureMeta, raw: &[u8]) -> Result<Self> {
        validate_meta(&meta)?;
        if raw.len() != meta.total_bytes as usize {
            return Err(Error::Protocol(
                "capture byte count does not match metadata".into(),
            ));
        }
        let bytes_per_pixel = pixel_bytes(meta.format);
        let mut rgb = Vec::with_capacity(meta.width as usize * meta.height as usize * 3);
        for row in raw.chunks_exact(meta.stride as usize) {
            for pixel in row[..meta.width as usize * bytes_per_pixel].chunks_exact(bytes_per_pixel)
            {
                match meta.format {
                    PixelFormat::Rgb565Le => {
                        let value = u16::from_le_bytes([pixel[0], pixel[1]]);
                        rgb.extend_from_slice(&[
                            ((value >> 11) as u32 * 255 / 31) as u8,
                            (((value >> 5) & 63) as u32 * 255 / 63) as u8,
                            ((value & 31) as u32 * 255 / 31) as u8,
                        ]);
                    }
                    PixelFormat::Rgb888 | PixelFormat::Rgba8888 => {
                        rgb.extend_from_slice(&pixel[..3])
                    }
                }
            }
        }
        Ok(Self { meta, rgb })
    }
}

fn pixel_bytes(format: PixelFormat) -> usize {
    match format {
        PixelFormat::Rgb565Le => 2,
        PixelFormat::Rgb888 => 3,
        PixelFormat::Rgba8888 => 4,
    }
}

fn validate_meta(meta: &CaptureMeta) -> Result<()> {
    // Host memory budget, independent of an embedded device's row size.
    const MAX_CAPTURE_BYTES: u32 = 32 * 1024 * 1024;
    let row = meta.width.checked_mul(pixel_bytes(meta.format) as u32);
    if meta.capture_id == 0
        || meta.width == 0
        || meta.height == 0
        || row.is_none()
        || row.unwrap() > meta.stride
        || meta.stride.checked_mul(meta.height) != Some(meta.total_bytes)
        || meta.total_bytes > MAX_CAPTURE_BYTES
    {
        return Err(Error::Protocol(
            "invalid or over-budget capture metadata".into(),
        ));
    }
    Ok(())
}

struct Lease {
    session: Session,
    id: Option<u32>,
    armed: bool,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let (Some(id), Ok(runtime)) = (self.id, tokio::runtime::Handle::try_current()) {
            let session = self.session.clone();
            runtime.spawn(async move {
                if let Ok(bytes) = encode(&CaptureEnd { capture_id: id }) {
                    let result = session
                        .request_raw(Operation::CaptureEnd.into(), &bytes)
                        .await;
                    if !matches!(
                        result,
                        Ok(_) | Err(Error::Device(escope_protocol::Status::Stale))
                    ) {
                        session.close();
                    }
                } else {
                    session.close();
                }
            });
        } else {
            // The device may have allocated a frame before a lost/late start
            // reply. Without a capture ID, disconnect is the reset boundary.
            self.session.close();
        }
    }
}

impl Session {
    pub async fn capture(&self) -> Result<Capture> {
        self.require(capability::CAPTURE)?;
        let mut lease = Lease {
            session: self.clone(),
            id: None,
            armed: true,
        };
        let bytes = match self.request_raw(Operation::CaptureStart.into(), &[]).await {
            Ok(bytes) => bytes,
            Err(error @ Error::Device(_)) => {
                // A confirmed rejection did not grant a capture lease.
                lease.armed = false;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        let meta: CaptureMeta = decode(&bytes)?;
        lease.id = (meta.capture_id != 0).then_some(meta.capture_id);
        validate_meta(&meta)?;
        let mut raw = Vec::with_capacity(meta.total_bytes as usize);
        let chunk_size = self.hello().max_payload.saturating_sub(64).min(768);
        if chunk_size == 0 {
            return Err(Error::Protocol(
                "negotiated payload cannot hold a capture chunk".into(),
            ));
        }
        while raw.len() < meta.total_bytes as usize {
            let offset = raw.len() as u32;
            let requested = chunk_size.min(meta.total_bytes - offset);
            let query = encode(&CaptureRead {
                capture_id: meta.capture_id,
                offset,
                length: requested,
            })?;
            let bytes = self
                .request_raw(Operation::CaptureRead.into(), &query)
                .await?;
            let chunk: CaptureChunk<'_> = decode(&bytes)?;
            if chunk.capture_id != meta.capture_id
                || chunk.offset != offset
                || chunk.data.is_empty()
                || chunk.data.len() > requested as usize
                || chunk.done != (offset as usize + chunk.data.len() == meta.total_bytes as usize)
            {
                return Err(Error::Protocol(
                    "capture chunk has invalid identity, offset, length or terminal flag".into(),
                ));
            }
            raw.extend_from_slice(chunk.data);
        }
        self.request_raw(
            Operation::CaptureEnd.into(),
            &encode(&CaptureEnd {
                capture_id: meta.capture_id,
            })?,
        )
        .await?;
        lease.armed = false;
        Capture::from_raw(meta, &raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn meta() -> CaptureMeta {
        CaptureMeta {
            capture_id: 1,
            width: 2,
            height: 1,
            stride: 6,
            format: PixelFormat::Rgb565Le,
            total_bytes: 6,
            frame_id: 7,
        }
    }
    #[test]
    fn rgb565_uses_little_endian_and_ignores_row_padding() {
        let capture = Capture::from_raw(meta(), &[0x00, 0xf8, 0xe0, 0x07, 0xff, 0xff]).unwrap();
        assert_eq!(capture.rgb, [255, 0, 0, 0, 255, 0]);
    }
    #[test]
    fn metadata_overflow_and_short_stride_are_rejected() {
        let mut m = meta();
        m.width = u32::MAX;
        assert!(Capture::from_raw(m, &[]).is_err());
        m = meta();
        m.stride = 2;
        m.total_bytes = 2;
        assert!(Capture::from_raw(m, &[0; 2]).is_err());
        m = meta();
        m.height = u32::MAX;
        assert!(Capture::from_raw(m, &[]).is_err());
    }
}
