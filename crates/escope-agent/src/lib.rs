//! Executor-independent, allocation-free EmbedScope device agent.
#![no_std]
#![forbid(unsafe_code)]

mod logs;

use escope_protocol::{
    capability, decode_payload, encode_frame, encode_payload, DeviceInfo, Envelope, Error,
    HelloReply, Kind, LogFilter, Operation, Status, MAX_FRAME, MAX_PAYLOAD, VERSION,
};
pub use logs::{LogRing, LOG_MESSAGE_BYTES, LOG_TARGET_BYTES};

/// Application integration boundary. UI operations execute on the UI thread;
/// a communication task may enqueue work and return Busy while it is pending.
pub trait Backend {
    fn capabilities(&self) -> u32;
    fn info(&self) -> DeviceInfo<'_>;
    /// Encode the operation's typed CBOR reply into `reply` and return its size.
    /// Unsupported operations return `Status::Unsupported`.
    fn handle(&mut self, op: u16, payload: &[u8], reply: &mut [u8]) -> Result<usize, Status>;
    /// Release capture state and any synthetic pressed input on disconnect.
    fn reset(&mut self);
}

pub struct Agent {
    session_id: u32,
    active: bool,
    subscribed: bool,
    max_level: u8,
    logs: LogRing<16>,
}

impl Agent {
    /// `session_id` is a nonzero device/boot session identifier.
    pub const fn new(session_id: u32) -> Self {
        Self {
            session_id,
            active: false,
            subscribed: false,
            max_level: 5,
            logs: LogRing::new(),
        }
    }

    pub fn session_id(&self) -> u32 {
        self.session_id
    }

    pub fn reset(&mut self, backend: &mut impl Backend) {
        self.active = false;
        self.advance_session();
        self.subscribed = false;
        self.logs.clear();
        backend.reset();
    }

    /// Dispatch one complete request and return a complete framed response.
    /// `out` must hold `MAX_FRAME` bytes. Capacity is checked before any
    /// backend side effect, so callers never execute a command without space
    /// for its response.
    /// Invalid frame bytes are handled by the protocol decoder before dispatch.
    pub fn handle(
        &mut self,
        request: Envelope<'_>,
        backend: &mut impl Backend,
        out: &mut [u8],
    ) -> Result<usize, Error> {
        if out.len() < MAX_FRAME {
            return Err(Error::Capacity);
        }
        let mut payload = [0u8; MAX_PAYLOAD];
        let result = self.dispatch(&request, backend, &mut payload);
        let (status, len) = match result {
            Ok(len) if len <= payload.len() => (Status::Ok, len),
            Ok(_) => (Status::Internal, 0),
            Err(status) => (status, 0),
        };
        encode_frame(
            &Envelope {
                version: VERSION,
                kind: Kind::Response,
                session: self.session_id,
                id: request.id,
                op: request.op,
                status,
                payload: &payload[..len],
            },
            out,
        )
    }

    fn dispatch(
        &mut self,
        request: &Envelope<'_>,
        backend: &mut impl Backend,
        reply: &mut [u8],
    ) -> Result<usize, Status> {
        if request.version != VERSION {
            return Err(Status::Version);
        }
        if request.payload.len() > MAX_PAYLOAD {
            return Err(Status::Capacity);
        }
        if request.kind != Kind::Request || request.id == 0 || request.status != Status::Ok {
            return Err(Status::InvalidArgument);
        }
        let op = Operation::try_from(request.op)?;
        if op == Operation::Hello {
            if request.session != 0 || !request.payload.is_empty() || self.session_id == 0 {
                return Err(Status::InvalidArgument);
            }
            // A fresh handshake invalidates outstanding backend work and logs.
            if self.active {
                self.advance_session();
            }
            self.active = false;
            backend.reset();
            self.subscribed = false;
            self.logs.clear();
            let info = backend.info();
            let hello = HelloReply {
                session: self.session_id,
                capabilities: backend.capabilities(),
                max_payload: MAX_PAYLOAD as u32,
                protocol: VERSION,
                name: info.name,
                build_id: info.build_id,
            };
            let n = encode_payload(&hello, reply).map_err(|_| Status::Capacity)?;
            self.active = true;
            return Ok(n);
        }
        if !self.active || request.session != self.session_id {
            return Err(Status::Session);
        }
        let required = match op {
            Operation::Metrics => capability::METRICS,
            Operation::LogsSubscribe | Operation::LogsUnsubscribe => capability::LOGS,
            Operation::UiTree => capability::UI_TREE,
            Operation::InputTap => capability::INPUT,
            Operation::CaptureStart | Operation::CaptureRead | Operation::CaptureEnd => {
                capability::CAPTURE
            }
            Operation::Crash => capability::CRASH,
            Operation::Log => return Err(Status::Unsupported),
            _ => 0,
        };
        if backend.capabilities() & required != required {
            return Err(Status::Unsupported);
        }
        match op {
            Operation::Info => {
                if !request.payload.is_empty() {
                    return Err(Status::InvalidArgument);
                }
                encode_payload(&backend.info(), reply).map_err(|_| Status::Capacity)
            }
            Operation::LogsSubscribe => {
                let filter = if request.payload.is_empty() {
                    LogFilter { max_level: 5 }
                } else {
                    decode_payload(request.payload).map_err(|_| Status::InvalidArgument)?
                };
                if !(1..=5).contains(&filter.max_level) {
                    return Err(Status::InvalidArgument);
                }
                self.max_level = filter.max_level;
                self.subscribed = true;
                Ok(0)
            }
            Operation::LogsUnsubscribe => {
                if !request.payload.is_empty() {
                    return Err(Status::InvalidArgument);
                }
                self.subscribed = false;
                self.logs.clear();
                Ok(0)
            }
            _ => backend.handle(request.op, request.payload, reply),
        }
    }

    fn advance_session(&mut self) {
        self.session_id = self.session_id.wrapping_add(1).max(1);
    }

    /// Publish a structured record. Full queues discard the new record and
    /// preserve the dropped count for the next delivered event.
    pub fn push_log(
        &mut self,
        timestamp_ms: u64,
        level: u8,
        target: &str,
        message: &str,
    ) -> Result<(), Status> {
        if !(1..=5).contains(&level) {
            return Err(Status::InvalidArgument);
        }
        if self.subscribed && level <= self.max_level {
            self.logs.push(timestamp_ms, level, target, message);
        }
        Ok(())
    }

    pub fn dropped_logs(&self) -> u64 {
        self.logs.dropped()
    }

    /// Include logs lost by a platform queue before `push_log`. Call from the
    /// Agent task, alongside queued log delivery. Only an active subscription
    /// accumulates loss; unsubscribe, Hello and reset clear the total.
    pub fn record_dropped_logs(&mut self, count: u64) {
        if self.subscribed {
            self.logs.record_dropped(count);
        }
    }

    /// Emit at most one log event. A failed encoding retains the queued record.
    pub fn poll_event(&mut self, out: &mut [u8]) -> Result<Option<usize>, Error> {
        if !self.active || !self.subscribed {
            return Ok(None);
        }
        let Some(record) = self.logs.front() else {
            return Ok(None);
        };
        let mut payload = [0; MAX_PAYLOAD];
        let len = encode_payload(&record, &mut payload)?;
        let n = encode_frame(
            &Envelope {
                version: VERSION,
                kind: Kind::Event,
                session: self.session_id,
                id: 0,
                op: Operation::Log.into(),
                status: Status::Ok,
                payload: &payload[..len],
            },
            out,
        )?;
        self.logs.pop();
        Ok(Some(n))
    }
}
