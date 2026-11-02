use escope_agent::{Agent, Backend, LogRing};
use escope_protocol::{
    capability, decode_frame, decode_payload, encode_payload, DeviceInfo, Envelope, HelloReply,
    Kind, LogFilter, LogRecord, Metrics, Operation, Status, MAX_FRAME, MAX_PACKET, VERSION,
};

struct Device {
    capabilities: u32,
    handled: u32,
    resets: u32,
    invalid_length: bool,
}

impl Default for Device {
    fn default() -> Self {
        Self {
            capabilities: capability::LOGS | capability::METRICS,
            handled: 0,
            resets: 0,
            invalid_length: false,
        }
    }
}

impl Backend for Device {
    fn capabilities(&self) -> u32 {
        self.capabilities
    }
    fn info(&self) -> DeviceInfo<'_> {
        DeviceInfo {
            name: "test",
            model: "native",
            firmware: "0.1.0",
            build_id: "test-build",
            boot_id: 7,
            uptime_ms: 42,
        }
    }
    fn handle(&mut self, op: u16, _payload: &[u8], out: &mut [u8]) -> Result<usize, Status> {
        self.handled += 1;
        if self.invalid_length {
            return Ok(out.len() + 1);
        }
        if op == u16::from(Operation::Metrics) {
            encode_payload(
                &Metrics {
                    uptime_ms: 42,
                    heap_free: Some(100),
                    ..Metrics::default()
                },
                out,
            )
            .map_err(|_| Status::Capacity)
        } else {
            Err(Status::Unsupported)
        }
    }
    fn reset(&mut self) {
        self.resets += 1;
    }
}

fn request(session: u32, op: Operation, payload: &[u8]) -> Envelope<'_> {
    Envelope {
        version: VERSION,
        kind: Kind::Request,
        session,
        id: 1,
        op: op.into(),
        status: Status::Ok,
        payload,
    }
}

fn status(agent: &mut Agent, device: &mut Device, req: Envelope<'_>) -> Status {
    let mut out = [0; MAX_FRAME];
    let n = agent.handle(req, device, &mut out).unwrap();
    let mut scratch = [0; MAX_PACKET];
    decode_frame(&out[..n - 1], &mut scratch).unwrap().status
}

fn handshake(agent: &mut Agent, device: &mut Device) {
    assert_eq!(
        status(agent, device, request(0, Operation::Hello, &[])),
        Status::Ok
    );
}

#[test]
fn platform_log_loss_combines_with_fifo_loss_and_follows_subscription_lifetime() {
    let mut agent = Agent::new(17);
    let mut device = Device::default();
    agent.record_dropped_logs(100);
    assert_eq!(agent.dropped_logs(), 0);
    handshake(&mut agent, &mut device);
    assert_eq!(
        status(
            &mut agent,
            &mut device,
            request(17, Operation::LogsSubscribe, &[])
        ),
        Status::Ok
    );
    agent.record_dropped_logs(3);
    for _ in 0..17 {
        agent.push_log(1, 3, "device", "message").unwrap();
    }
    let mut out = [0; MAX_FRAME];
    let count = agent.poll_event(&mut out).unwrap().unwrap();
    let mut scratch = [0; MAX_PACKET];
    let event = decode_frame(&out[..count - 1], &mut scratch).unwrap();
    assert_eq!(
        decode_payload::<LogRecord<'_>>(event.payload)
            .unwrap()
            .dropped,
        4
    );
    agent.record_dropped_logs(u64::MAX);
    agent.record_dropped_logs(1);
    assert_eq!(agent.dropped_logs(), u64::MAX);
    assert_eq!(
        status(
            &mut agent,
            &mut device,
            request(17, Operation::LogsUnsubscribe, &[])
        ),
        Status::Ok
    );
    agent.record_dropped_logs(2);
    assert_eq!(agent.dropped_logs(), 0);
    assert_eq!(
        status(
            &mut agent,
            &mut device,
            request(17, Operation::LogsSubscribe, &[])
        ),
        Status::Ok
    );
    agent.record_dropped_logs(9);
    handshake(&mut agent, &mut device);
    assert_eq!(agent.dropped_logs(), 0);
    let session = agent.session_id();
    assert_eq!(
        status(
            &mut agent,
            &mut device,
            request(session, Operation::LogsSubscribe, &[])
        ),
        Status::Ok
    );
    agent.record_dropped_logs(5);
    agent.reset(&mut device);
    assert_eq!(agent.dropped_logs(), 0);
}

#[test]
fn hello_binds_session_and_reports_identity_and_capabilities() {
    let mut agent = Agent::new(99);
    let mut device = Device::default();
    assert_eq!(
        status(
            &mut agent,
            &mut device,
            request(99, Operation::Metrics, &[])
        ),
        Status::Session
    );
    let mut out = [0; MAX_FRAME];
    let n = agent
        .handle(request(0, Operation::Hello, &[]), &mut device, &mut out)
        .unwrap();
    let mut scratch = [0; MAX_PACKET];
    let response = decode_frame(&out[..n - 1], &mut scratch).unwrap();
    assert_eq!(
        (
            response.kind,
            response.session,
            response.id,
            response.status
        ),
        (Kind::Response, 99, 1, Status::Ok)
    );
    let hello: HelloReply<'_> = decode_payload(response.payload).unwrap();
    assert_eq!(
        (hello.session, hello.name, hello.build_id),
        (99, "test", "test-build")
    );
    assert_eq!(hello.capabilities, capability::LOGS | capability::METRICS);
    assert_eq!(device.resets, 1);
    assert_eq!(
        status(
            &mut agent,
            &mut device,
            request(98, Operation::Metrics, &[])
        ),
        Status::Session
    );
    let n = agent
        .handle(request(99, Operation::Metrics, &[]), &mut device, &mut out)
        .unwrap();
    let response = decode_frame(&out[..n - 1], &mut scratch).unwrap();
    assert_eq!(
        decode_payload::<Metrics>(response.payload)
            .unwrap()
            .heap_free,
        Some(100)
    );
}

#[test]
fn capability_unknown_operation_and_invalid_requests_have_no_backend_effects() {
    let mut agent = Agent::new(7);
    let mut device = Device::default();
    handshake(&mut agent, &mut device);
    assert_eq!(
        status(
            &mut agent,
            &mut device,
            request(7, Operation::InputTap, &[])
        ),
        Status::Unsupported
    );
    let mut unknown = request(7, Operation::Info, &[]);
    unknown.op = 65000;
    assert_eq!(
        status(&mut agent, &mut device, unknown),
        Status::Unsupported
    );
    let mut wrong_version = request(7, Operation::Metrics, &[]);
    wrong_version.version = 100;
    assert_eq!(
        status(&mut agent, &mut device, wrong_version),
        Status::Version
    );
    let mut invalid = request(7, Operation::Info, &[]);
    invalid.kind = Kind::Event;
    assert_eq!(
        status(&mut agent, &mut device, invalid),
        Status::InvalidArgument
    );
    invalid = request(7, Operation::Info, &[]);
    invalid.id = 0;
    assert_eq!(
        status(&mut agent, &mut device, invalid),
        Status::InvalidArgument
    );
    assert_eq!(device.handled, 0);
}

#[test]
fn reset_invalidates_session_and_releases_backend_and_events() {
    let mut agent = Agent::new(7);
    let mut device = Device::default();
    handshake(&mut agent, &mut device);
    assert_eq!(
        status(
            &mut agent,
            &mut device,
            request(7, Operation::LogsSubscribe, &[])
        ),
        Status::Ok
    );
    agent.push_log(0, 3, "test", "queued").unwrap();
    agent.reset(&mut device);
    assert_eq!(device.resets, 2);
    assert_eq!(
        status(&mut agent, &mut device, request(7, Operation::Info, &[])),
        Status::Session
    );
    let mut out = [0; MAX_FRAME];
    assert_eq!(agent.poll_event(&mut out).unwrap(), None);
    handshake(&mut agent, &mut device);
    assert_eq!(
        status(&mut agent, &mut device, request(7, Operation::Info, &[])),
        Status::Session
    );
    let current = agent.session_id();
    assert_eq!(
        status(
            &mut agent,
            &mut device,
            request(current, Operation::Info, &[])
        ),
        Status::Ok
    );
}

#[test]
fn repeated_hello_rotates_session_and_output_capacity_precedes_side_effects() {
    let mut agent = Agent::new(u32::MAX);
    let mut device = Device::default();
    handshake(&mut agent, &mut device);
    assert_eq!(agent.session_id(), u32::MAX);
    let mut tiny = [0; 1];
    assert!(agent
        .handle(request(0, Operation::Hello, &[]), &mut device, &mut tiny)
        .is_err());
    assert_eq!((agent.session_id(), device.resets), (u32::MAX, 1));
    handshake(&mut agent, &mut device);
    assert_eq!(agent.session_id(), 1);
    assert_eq!(
        status(
            &mut agent,
            &mut device,
            request(u32::MAX, Operation::Info, &[])
        ),
        Status::Session
    );
    assert_eq!(
        status(&mut agent, &mut device, request(1, Operation::Info, &[])),
        Status::Ok
    );
}

#[test]
fn logs_are_filtered_bounded_report_loss_and_keep_records_after_small_output() {
    let mut agent = Agent::new(7);
    let mut device = Device::default();
    handshake(&mut agent, &mut device);
    let mut filter = [0; 20];
    let len = encode_payload(&LogFilter { max_level: 3 }, &mut filter).unwrap();
    assert_eq!(
        status(
            &mut agent,
            &mut device,
            request(7, Operation::LogsSubscribe, &filter[..len])
        ),
        Status::Ok
    );
    agent.push_log(0, 5, "test", "filtered").unwrap();
    let mut out = [0; MAX_FRAME];
    assert_eq!(agent.poll_event(&mut out).unwrap(), None);
    for i in 0..20 {
        agent.push_log(i, 3, "test", "hello").unwrap();
    }
    assert_eq!(agent.dropped_logs(), 4);
    assert!(agent.poll_event(&mut [0; 1]).is_err());
    let mut scratch = [0; MAX_PACKET];
    for i in 0..16 {
        let n = agent.poll_event(&mut out).unwrap().unwrap();
        let envelope = decode_frame(&out[..n - 1], &mut scratch).unwrap();
        assert_eq!(
            (envelope.kind, envelope.id, envelope.op),
            (Kind::Event, 0, Operation::Log.into())
        );
        let log: LogRecord<'_> = decode_payload(envelope.payload).unwrap();
        assert_eq!(
            (log.seq, log.timestamp_ms, log.dropped, log.message),
            (i + 1, i, 4, "hello")
        );
    }
    assert_eq!(agent.poll_event(&mut out).unwrap(), None);
    assert_eq!(
        status(
            &mut agent,
            &mut device,
            request(7, Operation::LogsUnsubscribe, &[])
        ),
        Status::Ok
    );
    agent.push_log(99, 3, "test", "unsubscribed").unwrap();
    assert_eq!(agent.poll_event(&mut out).unwrap(), None);
}

#[test]
fn utf8_truncation_and_zero_capacity_ring_are_explicit() {
    let mut ring = LogRing::<2>::new();
    let message = "团".repeat(100);
    let target = "🙂".repeat(20);
    assert!(ring.push(1, 3, &target, &message));
    let log = ring.front().unwrap();
    assert!(log.truncated);
    assert_eq!(log.message.len(), 192);
    assert_eq!(log.target.len(), 48);
    assert!(ring.pop());
    assert!(ring.is_empty());
    assert!(!ring.pop());
    let mut zero = LogRing::<0>::new();
    assert!(!zero.push(0, 3, "t", "m"));
    assert_eq!(zero.dropped(), 1);
    assert!(zero.front().is_none());
}

#[test]
fn invalid_provider_length_is_internal_error_not_slice_panic() {
    let mut agent = Agent::new(7);
    let mut device = Device::default();
    handshake(&mut agent, &mut device);
    device.invalid_length = true;
    assert_eq!(
        status(&mut agent, &mut device, request(7, Operation::Metrics, &[])),
        Status::Internal
    );
}
