use escope_protocol::{
    decode_frame, decode_payload, encode_frame, encode_payload, Envelope, Error, FrameDecoder,
    Kind, Operation, Status, TreeQuery, MAX_FRAME, MAX_PACKET, MAX_PAYLOAD, VERSION,
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
