use escope_agent::{Agent, Backend};
use escope_host::{protocol as p, simulator::ReferenceDevice, Error, Session};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Notify,
    task::JoinHandle,
};

#[derive(Clone, Copy)]
enum Fault {
    DelayStart,
    MalformedMeta,
    RejectStart,
}

struct BackendFixture {
    reference: ReferenceDevice,
    active: Arc<AtomicBool>,
    started: Arc<Notify>,
    fault: Fault,
}

impl Backend for BackendFixture {
    fn capabilities(&self) -> u32 {
        self.reference.capabilities()
    }
    fn info(&self) -> p::DeviceInfo<'_> {
        self.reference.info()
    }
    fn handle(&mut self, op: u16, payload: &[u8], out: &mut [u8]) -> Result<usize, p::Status> {
        if op == u16::from(p::Operation::CaptureStart) && matches!(self.fault, Fault::RejectStart) {
            return Err(p::Status::Busy);
        }
        let result = self.reference.handle(op, payload, out);
        if result.is_ok() {
            if op == u16::from(p::Operation::CaptureStart) {
                self.active.store(true, Ordering::Release);
                self.started.notify_one();
            } else if op == u16::from(p::Operation::CaptureEnd) {
                self.active.store(false, Ordering::Release);
            }
        }
        result
    }
    fn reset(&mut self) {
        self.reference.reset();
        self.active.store(false, Ordering::Release);
    }
}

struct Fixture {
    session: Session,
    active: Arc<AtomicBool>,
    started: Arc<Notify>,
    server: JoinHandle<()>,
}

async fn fixture(fault: Fault) -> Fixture {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("tcp://{}", listener.local_addr().unwrap());
    let active = Arc::new(AtomicBool::new(false));
    let started = Arc::new(Notify::new());
    let server_active = active.clone();
    let server_started = started.clone();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut agent = Agent::new(17);
        let mut backend = BackendFixture {
            reference: ReferenceDevice::new(17),
            active: server_active,
            started: server_started,
            fault,
        };
        let mut decoder = p::FrameDecoder::new();
        let mut scratch = [0; p::MAX_FRAME];
        let mut out = [0; p::MAX_FRAME];
        let mut rx = [0; 2048];
        'connection: loop {
            let n = match stream.read(&mut rx).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            for &byte in &rx[..n] {
                let Some(Ok(frame)) = decoder.push(byte) else {
                    continue;
                };
                let request = p::decode_frame(frame, &mut scratch).unwrap();
                let capture_start = request.op == u16::from(p::Operation::CaptureStart);
                let request_id = request.id;
                let mut n = agent.handle(request, &mut backend, &mut out).unwrap();
                if capture_start {
                    match fault {
                        Fault::DelayStart => tokio::time::sleep(Duration::from_millis(250)).await,
                        Fault::MalformedMeta => {
                            n = p::encode_frame(
                                &p::Envelope {
                                    version: p::VERSION,
                                    kind: p::Kind::Response,
                                    session: agent.session_id(),
                                    id: request_id,
                                    op: p::Operation::CaptureStart.into(),
                                    status: p::Status::Ok,
                                    payload: &[0xff],
                                },
                                &mut out,
                            )
                            .unwrap();
                        }
                        Fault::RejectStart => {}
                    }
                }
                if stream.write_all(&out[..n]).await.is_err() {
                    break 'connection;
                }
            }
        }
        agent.reset(&mut backend);
    });
    let session = Session::connect(&endpoint, 115200, Duration::from_millis(80))
        .await
        .unwrap();
    Fixture {
        session,
        active,
        started,
        server,
    }
}

async fn assert_released(fixture: Fixture) {
    tokio::time::timeout(Duration::from_secs(2), fixture.server)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !fixture.active.load(Ordering::Acquire),
        "backend reset must release its capture slot"
    );
}

#[tokio::test]
async fn timed_out_start_closes_unknown_capture_and_releases_backend() {
    let fixture = fixture(Fault::DelayStart).await;
    assert!(matches!(
        fixture.session.capture().await,
        Err(Error::Timeout)
    ));
    assert_eq!(fixture.session.pending_requests(), 0);
    assert!(
        fixture.session.is_closed(),
        "unknown capture ID after timeout requires a disconnect/reset"
    );
    assert_released(fixture).await;
}

#[tokio::test]
async fn cancelled_start_closes_unknown_capture_and_releases_backend() {
    let fixture = fixture(Fault::DelayStart).await;
    let session = fixture.session.clone();
    let capture = tokio::spawn(async move { session.capture().await });
    fixture.started.notified().await;
    assert!(fixture.active.load(Ordering::Acquire));
    capture.abort();
    let _ = capture.await;
    assert_eq!(fixture.session.pending_requests(), 0);
    assert!(
        fixture.session.is_closed(),
        "cancelled start must not leave an allocated capture in an open session"
    );
    assert_released(fixture).await;
}

#[tokio::test]
async fn malformed_start_metadata_closes_unknown_capture_and_releases_backend() {
    let fixture = fixture(Fault::MalformedMeta).await;
    assert!(matches!(
        fixture.session.capture().await,
        Err(Error::Protocol(_))
    ));
    assert!(fixture.session.is_closed());
    assert_released(fixture).await;
}

#[tokio::test]
async fn explicit_start_rejection_preserves_usable_session() {
    let fixture = fixture(Fault::RejectStart).await;
    assert!(matches!(
        fixture.session.capture().await,
        Err(Error::Device(p::Status::Busy))
    ));
    assert!(!fixture.session.is_closed());
    assert!(!fixture.active.load(Ordering::Acquire));
    assert!(fixture.session.info().await.is_ok());
    fixture.session.close();
    assert_released(fixture).await;
}
