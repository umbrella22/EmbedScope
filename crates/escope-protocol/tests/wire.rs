use escope_protocol::{
    decode_frame, decode_payload, encode_frame, encode_payload, CaptureChunk, CaptureMeta,
    Envelope, Error, FrameDecoder, Kind, Metrics, Operation, PixelFormat, Status, TreeQuery,
    UiNode, MAX_FRAME, MAX_PACKET, MAX_PAYLOAD, VERSION,
};

fn request(payload: &[u8]) -> Envelope<'_> {
    Envelope {
        version: VERSION,
        kind: Kind::Request,
        session: 0,
        id: 1,
        op: Operation::Hello.into(),
        status: Status::Ok,
        payload,
    }
}

#[test]
fn map_keys_and_unknown_optional_fields_are_stable() {
    let mut bytes = [0; 32];
    let n = encode_payload(
        &TreeQuery {
            cursor: 0,
            revision: 1,
        },
        &mut bytes,
    )
    .unwrap();
    assert_eq!(&bytes[..n], &[0xa2, 0, 0, 1, 1]);
    // An independent extension at key 99 contains a nested value.
    let extended = [0xa3, 0, 0, 1, 1, 0x18, 99, 0x82, 7, 8];
    assert_eq!(
        decode_payload::<TreeQuery>(&extended).unwrap(),
        TreeQuery {
            cursor: 0,
            revision: 1
        }
    );
    assert_eq!(
        decode_payload::<TreeQuery>(&[0xa2, 0, 0, 1, 1, 0]),
        Err(Error::Malformed)
    );
}

#[test]
fn payloads_match_minicbor_0_25_1_fixtures() {
    fn check<'a, T>(value: T, expected: &'a [u8])
    where
        T: minicbor::Encode<()> + minicbor::Decode<'a, ()> + PartialEq + core::fmt::Debug,
    {
        let mut encoded = [0; MAX_PAYLOAD];
        let n = encode_payload(&value, &mut encoded).unwrap();
        assert_eq!(&encoded[..n], expected);
        assert_eq!(decode_payload::<T>(expected).unwrap(), value);
    }

    // Captured with minicbor 0.25.1: codec upgrades must preserve protocol bytes,
    // including omitted optional fields, integer limits, enums and byte strings.
    check(
        Metrics {
            uptime_ms: 70_000,
            heap_free: Some(u64::MAX),
            heap_min_free: None,
            heap_largest_free: Some(42),
            psram_free: None,
            frame_ms: Some(16),
            queue_depth: Some(0),
            dropped_logs: 0x1_0000_0000,
        },
        &[
            0xa6, 0, 0x1a, 0, 1, 0x11, 0x70, 1, 0x1b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 3, 0x18, 42, 5, 16, 6, 0, 7, 0x1b, 0, 0, 0, 1, 0, 0, 0, 0,
        ],
    );
    check(
        CaptureMeta {
            capture_id: 9,
            width: 2,
            height: 1,
            stride: 8,
            format: PixelFormat::Rgba8888,
            total_bytes: 8,
            frame_id: u64::MAX,
        },
        &[
            0xa7, 0, 9, 1, 2, 2, 1, 3, 8, 4, 3, 5, 8, 6, 0x1b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff,
        ],
    );
    check(
        CaptureChunk {
            capture_id: 9,
            offset: 0,
            data: &[0, 0x18, 0xff],
            done: true,
        },
        &[0xa4, 0, 9, 1, 0, 2, 0x43, 0, 0x18, 0xff, 3, 0xf5],
    );
    check(
        UiNode {
            id: 7,
            parent: 0,
            kind: "label",
            name: "设置",
            x: -25,
            y: -1,
            width: 320,
            height: 240,
            flags: 0x8000_0001,
            revision: u32::MAX,
            next_cursor: None,
        },
        &[
            0xaa, 0, 7, 1, 0, 2, 0x65, b'l', b'a', b'b', b'e', b'l', 3, 0x66, 0xe8, 0xae, 0xbe,
            0xe7, 0xbd, 0xae, 4, 0x38, 0x18, 5, 0x20, 6, 0x19, 1, 0x40, 7, 0x18, 0xf0, 8, 0x1a,
            0x80, 0, 0, 1, 9, 0x1a, 0xff, 0xff, 0xff, 0xff,
        ],
    );
}

#[test]
fn reference_frame_and_exact_capacity_round_trip() {
    let envelope = request(&[]);
    let mut frame = [0; MAX_FRAME];
    let n = encode_frame(&envelope, &mut frame).unwrap();
    // CBOR map with fixed keys followed by CRC32-IEEE and COBS framing.
    let expected = [
        0x02, 0xa7, 0x03, 0x01, 0x01, 0x02, 0x02, 0x06, 0x03, 0x01, 0x04, 0x01, 0x05, 0x07, 0x06,
        0x40, 0xda, 0xf6, 0x07, 0x51, 0x00,
    ];
    assert_eq!(&frame[..n], &expected);
    let mut decoded = [0; MAX_PACKET];
    assert_eq!(
        decode_frame(&frame[..n - 1], &mut decoded).unwrap(),
        envelope
    );
    assert_eq!(
        encode_frame(&envelope, &mut frame[..n - 1]),
        Err(Error::Capacity)
    );
    assert_eq!(encode_frame(&envelope, &mut frame[..n]).unwrap(), n);
}

#[test]
fn maximum_payload_and_all_byte_values_fit_declared_bound() {
    let mut payload = [0; MAX_PAYLOAD];
    for (i, byte) in payload.iter_mut().enumerate() {
        *byte = i as u8;
    }
    let mut frame = [0; MAX_FRAME];
    let n = encode_frame(&request(&payload), &mut frame).unwrap();
    assert!(n <= MAX_FRAME);
    let mut decoded = [0; MAX_PACKET];
    assert_eq!(
        decode_frame(&frame[..n - 1], &mut decoded).unwrap().payload,
        payload
    );
    assert_eq!(
        encode_frame(&request(&[1; MAX_PAYLOAD + 1]), &mut frame),
        Err(Error::Capacity)
    );
    assert_eq!(
        decode_frame(&frame[..n - 1], &mut [0; 3]),
        Err(Error::Capacity)
    );
}

#[test]
fn bytewise_split_and_concatenation_preserve_each_message() {
    let mut first = [0; MAX_FRAME];
    let first_n = encode_frame(&request(b"first"), &mut first).unwrap();
    let mut second = [0; MAX_FRAME];
    let second_n = encode_frame(&request(b"second"), &mut second).unwrap();
    let mut parser = FrameDecoder::new();
    let mut scratch = [0; MAX_PACKET];
    let mut count = 0;
    for byte in first[..first_n].iter().chain(second[..second_n].iter()) {
        if let Some(encoded) = parser.push(*byte) {
            let message = decode_frame(encoded.unwrap(), &mut scratch).unwrap();
            assert_eq!(
                message.payload,
                if count == 0 {
                    &b"first"[..]
                } else {
                    &b"second"[..]
                }
            );
            count += 1;
        }
    }
    assert_eq!(count, 2);
    assert!(parser.push(0).is_none());
}

#[test]
fn corrupt_and_oversized_frames_recover_at_next_delimiter() {
    let mut frame = [0; MAX_FRAME];
    let n = encode_frame(&request(b"hello"), &mut frame).unwrap();
    let mut corrupt = frame;
    corrupt[n - 2] ^= 0x80;
    let mut parser = FrameDecoder::new();
    let mut scratch = [0; MAX_PACKET];
    let mut errors = 0;
    let mut messages = 0;
    for byte in corrupt[..n].iter().chain(frame[..n].iter()) {
        if let Some(encoded) = parser.push(*byte) {
            match decode_frame(encoded.unwrap(), &mut scratch) {
                Err(Error::Checksum) => errors += 1,
                Ok(message) => {
                    assert_eq!(message.payload, b"hello");
                    messages += 1;
                }
                other => panic!("unexpected decode result: {other:?}"),
            }
        }
    }
    assert_eq!((errors, messages), (1, 1));
    for _ in 0..MAX_FRAME + 100 {
        assert!(parser.push(1).is_none());
    }
    assert_eq!(parser.push(0).unwrap(), Err(Error::Capacity));
    for byte in &frame[..n] {
        if let Some(encoded) = parser.push(*byte) {
            assert_eq!(
                decode_frame(encoded.unwrap(), &mut scratch)
                    .unwrap()
                    .payload,
                b"hello"
            );
            messages += 1;
        }
    }
    assert_eq!(messages, 2);
}

#[test]
fn malformed_cobs_and_envelope_are_rejected() {
    let mut scratch = [0; MAX_PACKET];
    assert_eq!(decode_frame(&[], &mut scratch), Err(Error::Malformed));
    assert_eq!(decode_frame(&[3, 1], &mut scratch), Err(Error::Malformed));
    assert_eq!(decode_frame(&[2, 0], &mut scratch), Err(Error::Malformed));
    assert_eq!(decode_frame(&[1], &mut scratch), Err(Error::Malformed));
}
