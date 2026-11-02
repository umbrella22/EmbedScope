use escope_protocol::LogRecord;

pub const LOG_TARGET_BYTES: usize = 48;
pub const LOG_MESSAGE_BYTES: usize = 192;

#[derive(Clone, Copy)]
struct Entry {
    seq: u64,
    timestamp_ms: u64,
    level: u8,
    target: [u8; LOG_TARGET_BYTES],
    target_len: usize,
    message: [u8; LOG_MESSAGE_BYTES],
    message_len: usize,
    truncated: bool,
}

impl Entry {
    const EMPTY: Self = Self {
        seq: 0,
        timestamp_ms: 0,
        level: 0,
        target: [0; LOG_TARGET_BYTES],
        target_len: 0,
        message: [0; LOG_MESSAGE_BYTES],
        message_len: 0,
        truncated: false,
    };
}

/// Fixed-capacity FIFO. Overflow discards new records and increments a
/// cumulative counter; each delivered record reports the current total.
pub struct LogRing<const N: usize> {
    entries: [Entry; N],
    head: usize,
    len: usize,
    seq: u64,
    dropped: u64,
}

impl<const N: usize> Default for LogRing<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> LogRing<N> {
    pub const fn new() -> Self {
        Self {
            entries: [Entry::EMPTY; N],
            head: 0,
            len: 0,
            seq: 0,
            dropped: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Include records lost by a bounded platform queue before reaching this FIFO.
    pub fn record_dropped(&mut self, count: u64) {
        self.dropped = self.dropped.saturating_add(count);
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
        self.seq = 0;
        self.dropped = 0;
    }

    pub fn push(&mut self, timestamp_ms: u64, level: u8, target: &str, message: &str) -> bool {
        self.seq = self.seq.wrapping_add(1);
        if self.len == N {
            self.dropped = self.dropped.saturating_add(1);
            return false;
        }
        let index = (self.head + self.len) % N;
        let entry = &mut self.entries[index];
        entry.seq = self.seq;
        entry.timestamp_ms = timestamp_ms;
        entry.level = level;
        entry.target_len = copy_text(target, &mut entry.target);
        entry.message_len = copy_text(message, &mut entry.message);
        entry.truncated = entry.target_len != target.len() || entry.message_len != message.len();
        self.len += 1;
        true
    }

    pub fn front(&self) -> Option<LogRecord<'_>> {
        if self.len == 0 {
            return None;
        }
        let entry = &self.entries[self.head];
        Some(LogRecord {
            seq: entry.seq,
            timestamp_ms: entry.timestamp_ms,
            level: entry.level,
            // Every write copies an entire valid UTF-8 prefix.
            target: core::str::from_utf8(&entry.target[..entry.target_len]).ok()?,
            message: core::str::from_utf8(&entry.message[..entry.message_len]).ok()?,
            dropped: self.dropped,
            truncated: entry.truncated,
        })
    }

    pub fn pop(&mut self) -> bool {
        if self.len == 0 {
            return false;
        }
        self.head = (self.head + 1) % N;
        self.len -= 1;
        true
    }
}

fn copy_text(text: &str, out: &mut [u8]) -> usize {
    let mut len = text.len().min(out.len());
    while !text.is_char_boundary(len) {
        len -= 1;
    }
    out[..len].copy_from_slice(&text.as_bytes()[..len]);
    len
}
