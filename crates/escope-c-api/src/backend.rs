use crate::{EsBytes, EsCallbacks, EsDeviceInfo, ES_INVALID_ARGUMENT};
use escope_agent::Backend;
use escope_protocol::{DeviceInfo, Status};

struct Text<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> Text<N> {
    const fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }

    unsafe fn set(&mut self, value: EsBytes) -> Result<(), Status> {
        if value.len as usize > N {
            return Err(Status::Capacity);
        }
        // SAFETY: the callback guarantees readable storage until the enclosing
        // API call returns. Null and pointer-range errors are checked first.
        let bytes = unsafe { borrowed(value) }.map_err(|_| Status::InvalidArgument)?;
        core::str::from_utf8(bytes).map_err(|_| Status::InvalidArgument)?;
        self.bytes[..bytes.len()].copy_from_slice(bytes);
        self.len = bytes.len();
        Ok(())
    }

    fn get(&self) -> &str {
        // Only complete validated UTF-8 strings enter this storage.
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

pub(crate) struct CallbackBackend {
    pub callbacks: EsCallbacks,
    name: Text<64>,
    model: Text<64>,
    firmware: Text<32>,
    build_id: Text<64>,
    boot_id: u32,
    uptime_ms: u64,
}

impl CallbackBackend {
    pub fn new(callbacks: EsCallbacks) -> Self {
        Self {
            callbacks,
            name: Text::new(),
            model: Text::new(),
            firmware: Text::new(),
            build_id: Text::new(),
            boot_id: 0,
            uptime_ms: 0,
        }
    }

    pub fn refresh(&mut self) -> Result<(), Status> {
        let info = self.callbacks.info.ok_or(Status::Internal)?;
        let mut snapshot = EsDeviceInfo::EMPTY;
        // SAFETY: init validates the required callback. The caller keeps its
        // user data and callback code valid and serializes all context calls.
        let status = unsafe { info(self.callbacks.user, &mut snapshot) };
        if status != 0 {
            return Err(Status::Internal);
        }
        // SAFETY: strings returned by the callback remain readable until this
        // API call returns. Text::set validates lengths and UTF-8 before copy.
        unsafe {
            self.name.set(snapshot.name)?;
            self.model.set(snapshot.model)?;
            self.firmware.set(snapshot.firmware)?;
            self.build_id.set(snapshot.build_id)?;
        }
        self.boot_id = snapshot.boot_id;
        self.uptime_ms = snapshot.uptime_ms;
        Ok(())
    }
}

impl Backend for CallbackBackend {
    fn capabilities(&self) -> u32 {
        self.callbacks.capabilities.map_or(0, |callback| {
            // SAFETY: required callback and user storage follow init's contract.
            unsafe { callback(self.callbacks.user) }
        })
    }

    fn info(&self) -> DeviceInfo<'_> {
        DeviceInfo {
            name: self.name.get(),
            model: self.model.get(),
            firmware: self.firmware.get(),
            build_id: self.build_id.get(),
            boot_id: self.boot_id,
            uptime_ms: self.uptime_ms,
        }
    }

    fn handle(&mut self, op: u16, payload: &[u8], reply: &mut [u8]) -> Result<usize, Status> {
        let callback = self.callbacks.handle.ok_or(Status::Unsupported)?;
        let mut written = 0;
        // SAFETY: these borrowed slices stay valid for the callback; the caller
        // must honor reply_capacity and cannot retain the borrowed pointers.
        let status = unsafe {
            callback(
                self.callbacks.user,
                op,
                payload.as_ptr(),
                payload.len() as u32,
                reply.as_mut_ptr(),
                reply.len() as u32,
                &mut written,
            )
        };
        match status {
            0 => Ok(written as usize),
            1 => Err(Status::InvalidArgument),
            2 => Err(Status::Unsupported),
            3 => Err(Status::Session),
            4 => Err(Status::Busy),
            5 => Err(Status::Stale),
            6 => Err(Status::Capacity),
            7 => Err(Status::Internal),
            8 => Err(Status::Version),
            9 => Err(Status::Cancelled),
            _ => Err(Status::Internal),
        }
    }

    fn reset(&mut self) {
        if let Some(callback) = self.callbacks.reset {
            // SAFETY: callback code and user storage follow init's contract.
            unsafe { callback(self.callbacks.user) };
        }
    }
}

/// # Safety
/// Nonempty buffers must identify readable, initialized storage for the
/// returned borrow. The function checks null and integer range overflow.
pub(crate) unsafe fn borrowed<'a>(bytes: EsBytes) -> Result<&'a [u8], i32> {
    if bytes.len == 0 {
        return Ok(&[]);
    }
    if bytes.data.is_null()
        || (bytes.data as usize)
            .checked_add(bytes.len as usize)
            .is_none()
        || bytes.len as usize > isize::MAX as usize
    {
        return Err(ES_INVALID_ARGUMENT);
    }
    // SAFETY: the allocation validity and lifetime come from the caller.
    Ok(unsafe { core::slice::from_raw_parts(bytes.data, bytes.len as usize) })
}
