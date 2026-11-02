use crate::{Error, Result};
use escope_protocol::{
    decode_frame, encode_frame, Envelope, FrameDecoder, Kind, Operation, Status, MAX_FRAME,
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{broadcast, mpsc, oneshot, Semaphore},
    task::AbortHandle,
};

#[derive(Clone, Debug)]
pub struct Event {
    pub session: u32,
    pub operation: u16,
    pub payload: Vec<u8>,
}

struct Reply {
    session: u32,
    payload: Vec<u8>,
}
struct Pending {
    operation: u16,
    tx: oneshot::Sender<Result<Reply>>,
}
type Waiters = Arc<Mutex<HashMap<u32, Pending>>>;
struct Request {
    session: u32,
    id: u32,
    operation: u16,
    payload: Vec<u8>,
}

struct Inner {
    requests: mpsc::Sender<Request>,
    pending: Waiters,
    events: broadcast::Sender<Event>,
    session: Arc<AtomicU32>,
    next_id: AtomicU32,
    closed: Arc<AtomicBool>,
    bad_frames: Arc<AtomicU64>,
    limit: Semaphore,
    timeout: Duration,
    tasks: Vec<AbortHandle>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

struct PendingGuard {
    id: u32,
    waiters: Waiters,
}
impl Drop for PendingGuard {
    fn drop(&mut self) {
        self.waiters.lock().unwrap().remove(&self.id);
    }
}

/// One connection, one reader, bounded in-flight calls and a bounded event feed.
#[derive(Clone)]
pub struct Session {
    inner: Arc<Inner>,
    hello: Arc<OnceLock<crate::Hello>>,
}

/// Serial line state is preserved by default. Some USB CDC firmware requires
/// dtr=Some(true); choose it explicitly for the target's console contract.
#[derive(Clone, Copy, Debug)]
pub struct ConnectOptions {
    pub baud: u32,
    pub timeout: Duration,
    pub dtr: Option<bool>,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            baud: 115200,
            timeout: Duration::from_secs(3),
            dtr: None,
        }
    }
}

impl Session {
    pub async fn connect(endpoint: &str, baud: u32, timeout: Duration) -> Result<Self> {
        Self::connect_with_options(
            endpoint,
            ConnectOptions {
                baud,
                timeout,
                dtr: None,
            },
        )
        .await
    }

    pub async fn connect_with_options(endpoint: &str, options: ConnectOptions) -> Result<Self> {
        let timeout = options.timeout;
        let stream = tokio::time::timeout(
            timeout,
            crate::transport::open(endpoint, options.baud, options.dtr),
        )
        .await
        .map_err(|_| Error::Timeout)??;
        let session = Self::from_transport(stream, timeout);
        let reply = session.call(Operation::Hello as u16, &[]).await?;
        if reply.session == 0 {
            return Err(Error::Protocol("hello returned a zero session".into()));
        }
        session
            .inner
            .session
            .store(reply.session, Ordering::Release);
        // Decode the negotiated hello before exposing the connection.
        session.validate_hello(&reply.payload)?;
        Ok(session)
    }

    fn from_transport(stream: Box<dyn crate::transport::Transport>, timeout: Duration) -> Self {
        let (mut reader, mut writer) = tokio::io::split(stream);
        let (tx, mut rx) = mpsc::channel::<Request>(32);
        let (events, _) = broadcast::channel(128);
        let pending: Waiters = Arc::new(Mutex::new(HashMap::new()));
        let current = Arc::new(AtomicU32::new(0));
        let closed = Arc::new(AtomicBool::new(false));
        let bad_frames = Arc::new(AtomicU64::new(0));
        let write_pending = pending.clone();
        let write_closed = closed.clone();
        let write_task = tokio::spawn(async move {
            let mut bytes = [0; MAX_FRAME];
            while let Some(request) = rx.recv().await {
                if !write_pending.lock().unwrap().contains_key(&request.id) {
                    continue;
                }
                let envelope = Envelope {
                    version: 1,
                    kind: Kind::Request,
                    session: request.session,
                    id: request.id,
                    op: request.operation,
                    status: Status::Ok,
                    payload: &request.payload,
                };
                let result = match encode_frame(&envelope, &mut bytes) {
                    Ok(n) => writer.write_all(&bytes[..n]).await.map_err(Error::from),
                    Err(e) => Err(Error::Protocol(format!("encode: {e:?}"))),
                };
                if let Err(error) = result {
                    if let Some(p) = write_pending.lock().unwrap().remove(&request.id) {
                        let _ = p.tx.send(Err(error));
                    }
                    disconnect(&write_closed, &write_pending);
                    return;
                }
            }
            let _ = writer.shutdown().await;
        });
        let read_pending = pending.clone();
        let read_closed = closed.clone();
        let read_events = events.clone();
        let read_session = current.clone();
        let read_bad = bad_frames.clone();
        let read_task = tokio::spawn(async move {
            let mut decoder = FrameDecoder::new();
            let mut bytes = [0; 2048];
            let mut scratch = [0; MAX_FRAME];
            loop {
                let n = match reader.read(&mut bytes).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                for &byte in &bytes[..n] {
                    let Some(frame) = decoder.push(byte) else {
                        continue;
                    };
                    let envelope = match frame.and_then(|f| decode_frame(f, &mut scratch)) {
                        Ok(envelope) => envelope,
                        Err(_) => {
                            read_bad.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                    };
                    let expected_session = read_session.load(Ordering::Acquire);
                    match envelope.kind {
                        Kind::Response => {
                            if let Some(pending) = read_pending.lock().unwrap().remove(&envelope.id)
                            {
                                let result = if envelope.version != 1
                                    || pending.operation != envelope.op
                                    || (expected_session != 0
                                        && envelope.session != expected_session)
                                {
                                    Err(Error::Protocol(
                                        "response identity/version mismatch".into(),
                                    ))
                                } else if envelope.status != Status::Ok {
                                    Err(Error::Device(envelope.status))
                                } else {
                                    Ok(Reply {
                                        session: envelope.session,
                                        payload: envelope.payload.to_vec(),
                                    })
                                };
                                let _ = pending.tx.send(result);
                            }
                        }
                        Kind::Event
                            if expected_session != 0
                                && envelope.session == expected_session
                                && envelope.version == 1 =>
                        {
                            let _ = read_events.send(Event {
                                session: envelope.session,
                                operation: envelope.op,
                                payload: envelope.payload.to_vec(),
                            });
                        }
                        _ => {
                            read_bad.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
            disconnect(&read_closed, &read_pending);
        });
        Self {
            inner: Arc::new(Inner {
                requests: tx,
                pending,
                events,
                session: current,
                next_id: AtomicU32::new(1),
                closed,
                bad_frames,
                limit: Semaphore::new(32),
                timeout,
                tasks: vec![write_task.abort_handle(), read_task.abort_handle()],
            }),
            hello: Arc::new(OnceLock::new()),
        }
    }

    async fn call(&self, operation: u16, payload: &[u8]) -> Result<Reply> {
        let max = self
            .hello
            .get()
            .map_or(escope_protocol::MAX_PAYLOAD, |h| h.max_payload as usize);
        if payload.len() > max {
            return Err(Error::Protocol(
                "request exceeds negotiated maximum payload".into(),
            ));
        }
        let _permit = tokio::time::timeout(self.inner.timeout, self.inner.limit.acquire())
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(|_| Error::Disconnected)?;
        if self.is_closed() {
            return Err(Error::Disconnected);
        }
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        // Exhausting a session's request space requires a new connection.
        if id == 0 {
            self.close();
            return Err(Error::Protocol("request ID space exhausted".into()));
        }
        let (tx, rx) = oneshot::channel();
        self.inner
            .pending
            .lock()
            .unwrap()
            .insert(id, Pending { operation, tx });
        let _guard = PendingGuard {
            id,
            waiters: self.inner.pending.clone(),
        };
        let request = Request {
            session: self.id(),
            id,
            operation,
            payload: payload.to_vec(),
        };
        tokio::time::timeout(self.inner.timeout, async {
            self.inner
                .requests
                .send(request)
                .await
                .map_err(|_| Error::Disconnected)?;
            rx.await.map_err(|_| Error::Disconnected)?
        })
        .await
        .map_err(|_| Error::Timeout)?
    }

    pub async fn request_raw(&self, operation: u16, payload: &[u8]) -> Result<Vec<u8>> {
        Ok(self.call(operation, payload).await?.payload)
    }
    fn validate_hello(&self, bytes: &[u8]) -> Result<()> {
        let hello: escope_protocol::HelloReply<'_> = crate::decode(bytes)?;
        if hello.protocol != 1 || hello.session != self.id() || hello.max_payload == 0 {
            return Err(Error::Protocol("invalid hello negotiation".into()));
        }
        let _ = self.hello.set(crate::Hello {
            session: hello.session,
            capabilities: hello.capabilities,
            max_payload: hello.max_payload.min(escope_protocol::MAX_PAYLOAD as u32),
            protocol: hello.protocol,
            name: hello.name.into(),
            build_id: hello.build_id.into(),
        });
        Ok(())
    }
    pub fn hello(&self) -> &crate::Hello {
        self.hello
            .get()
            .expect("connect validates hello before exposing Session")
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }
    pub fn id(&self) -> u32 {
        self.inner.session.load(Ordering::Acquire)
    }
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }
    pub fn corrupt_frames(&self) -> u64 {
        self.inner.bad_frames.load(Ordering::Relaxed)
    }
    pub fn pending_requests(&self) -> usize {
        self.inner.pending.lock().unwrap().len()
    }
    pub fn close(&self) {
        disconnect(&self.inner.closed, &self.inner.pending);
        for task in &self.inner.tasks {
            task.abort();
        }
    }
}

fn disconnect(closed: &AtomicBool, pending: &Waiters) {
    closed.store(true, Ordering::Release);
    for (_, p) in pending.lock().unwrap().drain() {
        let _ = p.tx.send(Err(Error::Disconnected));
    }
}
