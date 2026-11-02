use core::ffi::c_void;

pub const ES_OK: i32 = 0;
pub const ES_INVALID_ARGUMENT: i32 = 1;
pub const ES_CAPACITY: i32 = 2;
pub const ES_BUSY: i32 = 3;
pub const ES_PROTOCOL: i32 = 4;
pub const ES_CALLBACK: i32 = 5;

/// Borrowed bytes, valid until the current API call returns. A zero length may
/// use a null pointer. String values must contain valid UTF-8.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EsBytes {
    pub data: *const u8,
    pub len: u32,
}

impl EsBytes {
    pub const EMPTY: Self = Self {
        data: core::ptr::null(),
        len: 0,
    };
}

/// Callback output. Strings are copied into bounded context storage.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EsDeviceInfo {
    pub name: EsBytes,
    pub model: EsBytes,
    pub firmware: EsBytes,
    pub build_id: EsBytes,
    pub boot_id: u32,
    pub uptime_ms: u64,
}

impl EsDeviceInfo {
    pub const EMPTY: Self = Self {
        name: EsBytes::EMPTY,
        model: EsBytes::EMPTY,
        firmware: EsBytes::EMPTY,
        build_id: EsBytes::EMPTY,
        boot_id: 0,
        uptime_ms: 0,
    };
}

pub type CapabilitiesFn = unsafe extern "C" fn(user: *mut c_void) -> u32;
pub type InfoFn = unsafe extern "C" fn(user: *mut c_void, info: *mut EsDeviceInfo) -> i32;
pub type HandleFn = unsafe extern "C" fn(
    user: *mut c_void,
    operation: u16,
    payload: *const u8,
    payload_len: u32,
    reply: *mut u8,
    reply_capacity: u32,
    reply_len: *mut u32,
) -> u16;
pub type ResetFn = unsafe extern "C" fn(user: *mut c_void);

/// Function pointers and `user` remain valid for the context's lifetime.
/// `info` and `capabilities` are required; other callbacks are optional.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EsCallbacks {
    pub user: *mut c_void,
    pub capabilities: Option<CapabilitiesFn>,
    pub info: Option<InfoFn>,
    pub handle: Option<HandleFn>,
    pub reset: Option<ResetFn>,
}
