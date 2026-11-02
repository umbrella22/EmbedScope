use escope_host::{protocol as p, simulator, Error, Session};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::watch,
};

async fn reference() -> (
    String,
    watch::Sender<bool>,
    tokio::task::JoinHandle<escope_host::Result<()>>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("tcp://{}", listener.local_addr().unwrap());
    let (stop, receiver) = watch::channel(false);
    (
        endpoint,
        stop,
        tokio::spawn(simulator::serve(listener, receiver)),
    )
}

#[tokio::test]
async fn concurrent_logs_control_capture_and_reconnect_preserve_device_state() {
    let (endpoint, stop, server) = reference().await;
    let session = Session::connect(&endpoint, 115200, Duration::from_secs(2))
        .await
        .unwrap();
    let mut logs = session.subscribe();
    session.start_logs(5).await.unwrap();
    let before = session.capture().await.unwrap();
    let (info, metrics, tree) = tokio::join!(session.info(), session.metrics(), session.ui_tree());
    assert_eq!(info.unwrap().model, "software / synthetic");
    assert_eq!(metrics.unwrap().psram_free, None);
    assert_eq!(tree.unwrap()[1].name, "Counter: 0");
    session.tap(100, 140, 60).await.unwrap();
    let after = session.capture().await.unwrap();
    assert_ne!(before.rgb, after.rgb);
    assert!(after.meta.frame_id > before.meta.frame_id);
    assert_eq!(session.ui_tree().await.unwrap()[1].name, "Counter: 1");
    let event = tokio::time::timeout(Duration::from_secs(2), logs.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.log().unwrap().session, session.id());
    let old_session = session.id();
    session.close();
    let next = Session::connect(&endpoint, 115200, Duration::from_secs(2))
        .await
        .unwrap();
    assert_ne!(old_session, next.id());
    assert_eq!(next.ui_tree().await.unwrap()[1].name, "Counter: 1");
    next.close();
    let _ = stop.send(true);
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn unsupported_and_invalid_input_are_explicit_and_connection_survives() {
    let (endpoint, stop, server) = reference().await;
    let session = Session::connect(&endpoint, 115200, Duration::from_secs(2))
        .await
        .unwrap();
    assert!(matches!(
        session.request_raw(999, &[]).await,
        Err(Error::Device(p::Status::Unsupported))
    ));
    assert!(matches!(
        session.tap(u32::MAX, 0, 60).await,
        Err(Error::Device(p::Status::InvalidArgument))
    ));
    assert!(session.info().await.is_ok());
    assert_eq!(session.pending_requests(), 0);
    session.close();
    let _ = stop.send(true);
    server.await.unwrap().unwrap();
}

struct Minimal;
impl escope_agent::Backend for Minimal {
    fn capabilities(&self) -> u32 {
        0
    }
    fn info(&self) -> p::DeviceInfo<'_> {
        p::DeviceInfo {
            name: "minimal",
            model: "fixture",
            firmware: "0",
            build_id: "fixture",
            boot_id: 1,
            uptime_ms: 0,
        }
    }
    fn handle(&mut self, _: u16, _: &[u8], _: &mut [u8]) -> Result<usize, p::Status> {
        Err(p::Status::Unsupported)
    }
    fn reset(&mut self) {}
}

#[tokio::test]
async fn timeout_and_task_cancellation_remove_waiters_and_late_response_is_ignored() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("tcp://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut decoder = p::FrameDecoder::new();
        let mut scratch = [0; p::MAX_FRAME];
        let mut out = [0; p::MAX_FRAME];
        let mut agent = escope_agent::Agent::new(42);
        let mut backend = Minimal;
        let mut slow = true;
        while let Ok(byte) = stream.read_u8().await {
            if let Some(Ok(frame)) = decoder.push(byte) {
                let request = p::decode_frame(frame, &mut scratch).unwrap();
                if request.op == p::Operation::Info as u16 && slow {
                    slow = false;
                    tokio::time::sleep(Duration::from_millis(350)).await;
                }
                let n = agent.handle(request, &mut backend, &mut out).unwrap();
                if stream.write_all(&out[..n]).await.is_err() {
                    break;
                }
            }
        }
    });
    let session = Session::connect(&endpoint, 115200, Duration::from_millis(150))
        .await
        .unwrap();
    assert!(matches!(session.info().await, Err(Error::Timeout)));
    assert_eq!(session.pending_requests(), 0);
    let cloned = session.clone();
    let cancelled = tokio::spawn(async move { cloned.info().await });
    tokio::task::yield_now().await;
    cancelled.abort();
    let _ = cancelled.await;
    assert_eq!(session.pending_requests(), 0);
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(session.info().await.unwrap().name, "minimal");
    assert!(matches!(
        session.metrics().await,
        Err(Error::Device(p::Status::Unsupported))
    ));
    session.close();
    server.abort();
}

#[tokio::test]
async fn disconnected_inflight_request_fails_and_cleans_up() {
    let (endpoint, stop, server) = reference().await;
    let session = Session::connect(&endpoint, 115200, Duration::from_secs(2))
        .await
        .unwrap();
    let _ = stop.send(true);
    server.await.unwrap().unwrap();
    let result = session.info().await;
    assert!(matches!(
        result,
        Err(Error::Disconnected) | Err(Error::Io(_))
    ));
    assert_eq!(session.pending_requests(), 0);
}
