use crate::{decode_payload, encode_payload, Envelope, Error, MAX_FRAME, MAX_PACKET, MAX_PAYLOAD};

/// Encode a complete COBS-delimited frame with an IEEE CRC32 (little endian).
pub fn encode_frame(envelope: &Envelope<'_>, out: &mut [u8]) -> Result<usize, Error> {
    if envelope.payload.len() > MAX_PAYLOAD {
        return Err(Error::Capacity);
    }
    let mut packet = [0u8; MAX_PACKET];
    let len = encode_payload(envelope, &mut packet[..MAX_PACKET - 4])?;
    let checksum = crc32(&packet[..len]);
    packet[len..len + 4].copy_from_slice(&checksum.to_le_bytes());
    cobs_encode(&packet[..len + 4], out)
}

/// Decode a frame without its trailing zero into caller-owned scratch storage.
///
/// Returned payload bytes borrow `scratch`; transport storage may be reused.
pub fn decode_frame<'a>(encoded: &[u8], scratch: &'a mut [u8]) -> Result<Envelope<'a>, Error> {
    if encoded.is_empty() {
        return Err(Error::Malformed);
    }
    if encoded.len() >= MAX_FRAME {
        return Err(Error::Capacity);
    }
    let len = cobs_decode(encoded, scratch)?;
    if len < 4 {
        return Err(Error::Malformed);
    }
    let body_len = len - 4;
    let checksum = u32::from_le_bytes(
        scratch[body_len..len]
            .try_into()
            .map_err(|_| Error::Malformed)?,
    );
    if crc32(&scratch[..body_len]) != checksum {
        return Err(Error::Checksum);
    }
    let envelope: Envelope<'a> = decode_payload(&scratch[..body_len])?;
    if envelope.payload.len() > MAX_PAYLOAD {
        return Err(Error::Capacity);
    }
    // The Agent returns a structured Version response when framing is valid.
    // Accept the envelope here so its request id can reach that boundary.
    Ok(envelope)
}

/// Bounded stream delimiter collector. Bad/oversized frames never poison the
/// next frame. Each successful result excludes the zero delimiter.
pub struct FrameDecoder {
    bytes: [u8; MAX_FRAME - 1],
    len: usize,
    overflow: bool,
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameDecoder {
    pub const fn new() -> Self {
        Self {
            bytes: [0; MAX_FRAME - 1],
            len: 0,
            overflow: false,
        }
    }

    pub fn reset(&mut self) {
        self.len = 0;
        self.overflow = false;
    }

    pub fn push(&mut self, byte: u8) -> Option<Result<&[u8], Error>> {
        if byte == 0 {
            if self.overflow {
                self.reset();
                return Some(Err(Error::Capacity));
            }
            let len = self.len;
            self.len = 0;
            return (len != 0).then_some(Ok(&self.bytes[..len]));
        }
        if !self.overflow {
            if self.len == self.bytes.len() {
                self.overflow = true;
            } else {
                self.bytes[self.len] = byte;
                self.len += 1;
            }
        }
        None
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

fn cobs_encode(input: &[u8], out: &mut [u8]) -> Result<usize, Error> {
    if out.is_empty() {
        return Err(Error::Capacity);
    }
    let mut code_index = 0usize;
    let mut write = 1usize;
    let mut code = 1u8;
    for &byte in input {
        if byte == 0 {
            *out.get_mut(code_index).ok_or(Error::Capacity)? = code;
            code_index = write;
            write = write.checked_add(1).ok_or(Error::Capacity)?;
            code = 1;
        } else {
            *out.get_mut(write).ok_or(Error::Capacity)? = byte;
            write += 1;
            code += 1;
            if code == 0xff {
                *out.get_mut(code_index).ok_or(Error::Capacity)? = code;
                code_index = write;
                write += 1;
                code = 1;
            }
        }
    }
    *out.get_mut(code_index).ok_or(Error::Capacity)? = code;
    *out.get_mut(write).ok_or(Error::Capacity)? = 0;
    Ok(write + 1)
}

fn cobs_decode(input: &[u8], out: &mut [u8]) -> Result<usize, Error> {
    let mut read = 0usize;
    let mut write = 0usize;
    while read < input.len() {
        let code = input[read];
        if code == 0 {
            return Err(Error::Malformed);
        }
        read += 1;
        let count = usize::from(code - 1);
        let end = read.checked_add(count).ok_or(Error::Malformed)?;
        let bytes = input.get(read..end).ok_or(Error::Malformed)?;
        if bytes.contains(&0) {
            return Err(Error::Malformed);
        }
        let write_end = write.checked_add(count).ok_or(Error::Capacity)?;
        out.get_mut(write..write_end)
            .ok_or(Error::Capacity)?
            .copy_from_slice(bytes);
        write = write_end;
        read = end;
        if code != 0xff && read < input.len() {
            *out.get_mut(write).ok_or(Error::Capacity)? = 0;
            write += 1;
        }
    }
    if write > MAX_PACKET {
        return Err(Error::Capacity);
    }
    Ok(write)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ieee_crc_reference_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn cobs_zero_and_254_boundary() {
        for input in [&[0, 1, 0, 2][..], &[1u8; 254][..], &[0u8; 254][..]] {
            let mut encoded = [0; 300];
            let n = cobs_encode(input, &mut encoded).unwrap();
            assert_eq!(encoded[n - 1], 0);
            assert!(!encoded[..n - 1].contains(&0));
            let mut decoded = [0; 300];
            let m = cobs_decode(&encoded[..n - 1], &mut decoded).unwrap();
            assert_eq!(&decoded[..m], input);
        }
    }
}
