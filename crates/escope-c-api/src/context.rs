use crate::backend::{borrowed, CallbackBackend};
use crate::*;
use core::{
    cell::{Cell, UnsafeCell},
    ffi::c_void,
    mem, ptr,
};
use escope_agent::Agent;
use escope_protocol::{
    decode_frame, encode_frame, Envelope, FrameDecoder, Kind, MAX_FRAME, MAX_PACKET, VERSION,
};

const MAGIC: u64 = 0x4553_434f_5045_0001;

struct Inner {
    agent: Agent,
    backend: CallbackBackend,
    decoder: FrameDecoder,
    scratch: [u8; MAX_PACKET],
    tx: [u8; MAX_FRAME],
    tx_len: usize,
}

struct Context {
    magic: u64,
    busy: Cell<bool>,
    inner: UnsafeCell<Inner>,
}

#[no_mangle]
pub extern "C" fn escope_context_size() -> u32 {
    mem::size_of::<Context>() as u32
}

#[no_mangle]
pub extern "C" fn escope_context_alignment() -> u32 {
    mem::align_of::<Context>() as u32
}

/// Initialize aligned caller storage. No heap allocation or global state.
///
/// # Safety
/// `storage` must identify `storage_len` writable bytes with no live context.
/// `callbacks` must identify a readable EsCallbacks. Callback code and user data
/// remain valid until the context is released. All operations are serialized;
/// input/output buffers must not overlap the context.
#[no_mangle]
pub unsafe extern "C" fn escope_init(
    storage: *mut c_void,
    storage_len: u32,
    callbacks: *const EsCallbacks,
    session_seed: u32,
) -> i32 {
    if storage.is_null()
        || !aligned(storage, mem::align_of::<Context>())
        || storage_len < escope_context_size()
        || session_seed == 0
        || !valid_range(storage.cast(), storage_len as usize)
        || callbacks.is_null()
        || !aligned(callbacks, mem::align_of::<EsCallbacks>())
        || !valid_range(callbacks.cast(), mem::size_of::<EsCallbacks>())
        || overlaps(
            storage.cast(),
            escope_context_size() as usize,
            callbacks.cast(),
            mem::size_of::<EsCallbacks>(),
        )
    {
        return ES_INVALID_ARGUMENT;
    }
    // SAFETY: init's caller provides a readable callback structure.
    let callbacks = unsafe { callbacks.read() };
    if callbacks.info.is_none() || callbacks.capabilities.is_none() {
        return ES_INVALID_ARGUMENT;
    }
    let context = Context {
        magic: MAGIC,
        busy: Cell::new(false),
        inner: UnsafeCell::new(Inner {
            agent: Agent::new(session_seed),
            backend: CallbackBackend::new(callbacks),
            decoder: FrameDecoder::new(),
            scratch: [0; MAX_PACKET],
            tx: [0; MAX_FRAME],
            tx_len: 0,
        }),
    };
    // SAFETY: alignment and length were checked; storage is writable and does
    // not contain a live context. No callback executes during initialization.
    unsafe { storage.cast::<Context>().write(context) };
    ES_OK
}

/// Consume bytes through the first complete frame. A pending TX frame returns
/// Busy with consumed=0. Malformed completed frames return Protocol and report
/// the consumed prefix, so the caller can advance and continue.
///
/// # Safety
/// `context` is a live initialized context. Nonempty input and `consumed` are
/// valid allocations, do not overlap each other or context, and remain valid
/// for this call. `consumed` is writable and aligned. Serialize context calls.
#[no_mangle]
pub unsafe extern "C" fn escope_feed(
    context: *mut c_void,
    data: *const u8,
    len: u32,
    consumed: *mut u32,
) -> i32 {
    if !valid_output(context, consumed)
        || !valid_range(data, len as usize)
        || overlaps_context(context, data, len as usize)
        || overlaps(data, len as usize, consumed.cast(), mem::size_of::<u32>())
    {
        return ES_INVALID_ARGUMENT;
    }
    // SAFETY: consumed is a checked writable output parameter.
    unsafe { consumed.write(0) };
    // SAFETY: input storage is valid for len bytes by the function contract.
    let input = match unsafe { borrowed(EsBytes { data, len }) } {
        Ok(input) => input,
        Err(error) => return error,
    };
    // SAFETY: with_inner verifies the context pointer and guards reentry.
    unsafe {
        with_inner(context, |inner| {
            if input.is_empty() {
                return ES_OK;
            }
            if inner.tx_len != 0 {
                return ES_BUSY;
            }
            for (index, &byte) in input.iter().enumerate() {
                // SAFETY: consumed remains writable and does not alias input/context.
                consumed.write((index + 1) as u32);
                let Some(frame) = inner.decoder.push(byte) else {
                    continue;
                };
                let encoded = match frame {
                    Ok(frame) => frame,
                    Err(_) => return ES_PROTOCOL,
                };
                let request = match decode_frame(encoded, &mut inner.scratch) {
                    Ok(request) => request,
                    Err(_) => return ES_PROTOCOL,
                };
                let n = match inner.backend.refresh() {
                    Ok(()) => inner
                        .agent
                        .handle(request, &mut inner.backend, &mut inner.tx),
                    Err(status) => encode_frame(
                        &Envelope {
                            version: VERSION,
                            kind: Kind::Response,
                            session: inner.agent.session_id(),
                            id: request.id,
                            op: request.op,
                            status,
                            payload: &[],
                        },
                        &mut inner.tx,
                    ),
                };
                match n {
                    Ok(n) => {
                        inner.tx_len = n;
                        return ES_OK;
                    }
                    Err(_) => return ES_PROTOCOL,
                }
            }
            ES_OK
        })
    }
}

/// Move at most one queued log event to the TX slot. Busy preserves the
/// pending frame; take_tx must drain it before polling again.
///
/// # Safety
/// `context` is a live initialized context and all its calls are serialized.
#[no_mangle]
pub unsafe extern "C" fn escope_poll(context: *mut c_void) -> i32 {
    // SAFETY: context follows the function's initialization/lifetime contract.
    unsafe {
        with_inner(context, |inner| {
            if inner.tx_len != 0 {
                return ES_BUSY;
            }
            match inner.agent.poll_event(&mut inner.tx) {
                Ok(Some(n)) => {
                    inner.tx_len = n;
                    ES_OK
                }
                Ok(None) => ES_OK,
                Err(_) => ES_PROTOCOL,
            }
        })
    }
}

/// Copy the entire pending frame. Capacity failure retains the frame and
/// returns its required size through `written`. Empty TX returns written=0.
///
/// # Safety
/// `context` is initialized. Nonempty output and `written` identify writable,
/// nonoverlapping allocations outside context. Serialize context calls.
#[no_mangle]
pub unsafe extern "C" fn escope_take_tx(
    context: *mut c_void,
    out: *mut u8,
    capacity: u32,
    written: *mut u32,
) -> i32 {
    if !valid_output(context, written)
        || !valid_range(out, capacity as usize)
        || overlaps_context(context, out, capacity as usize)
        || overlaps(
            out,
            capacity as usize,
            written.cast(),
            mem::size_of::<u32>(),
        )
    {
        return ES_INVALID_ARGUMENT;
    }
    // SAFETY: checked output parameter remains writable for this call.
    unsafe { written.write(0) };
    // SAFETY: with_inner verifies and serializes the context.
    unsafe {
        with_inner(context, |inner| {
            written.write(inner.tx_len as u32);
            if inner.tx_len > capacity as usize {
                return ES_CAPACITY;
            }
            if inner.tx_len != 0 {
                // SAFETY: output has enough storage and cannot overlap context.
                ptr::copy_nonoverlapping(inner.tx.as_ptr(), out, inner.tx_len);
                inner.tx_len = 0;
            }
            ES_OK
        })
    }
}

/// Reset RX/TX, subscriptions, input/capture state, and advance the session.
///
/// # Safety
/// `context` is a live initialized context and all calls are serialized.
#[no_mangle]
pub unsafe extern "C" fn escope_reset(context: *mut c_void) -> i32 {
    // SAFETY: context follows initialization and single-thread requirements.
    unsafe {
        with_inner(context, |inner| {
            inner.agent.reset(&mut inner.backend);
            inner.decoder.reset();
            inner.tx_len = 0;
            ES_OK
        })
    }
}

/// Queue a structured UTF-8 record. Overflow follows Agent's bounded/drop
/// policy; target/message truncation is reported in the event.
///
/// # Safety
/// `context` is initialized; nonempty text slices remain readable for the
/// call and do not overlap context. All calls on a context are serialized.
#[no_mangle]
pub unsafe extern "C" fn escope_push_log(
    context: *mut c_void,
    timestamp_ms: u64,
    level: u8,
    target: EsBytes,
    message: EsBytes,
) -> i32 {
    if overlaps_context(context, target.data, target.len as usize)
        || overlaps_context(context, message.data, message.len as usize)
    {
        return ES_INVALID_ARGUMENT;
    }
    // SAFETY: text storage follows the function contract; null and range
    // errors are validated by borrowed before any slice construction.
    let target = match unsafe { borrowed(target) }
        .and_then(|b| core::str::from_utf8(b).map_err(|_| ES_INVALID_ARGUMENT))
    {
        Ok(value) => value,
        Err(error) => return error,
    };
    // SAFETY: as for target above.
    let message = match unsafe { borrowed(message) }
        .and_then(|b| core::str::from_utf8(b).map_err(|_| ES_INVALID_ARGUMENT))
    {
        Ok(value) => value,
        Err(error) => return error,
    };
    // SAFETY: initialized context and serial calls follow the caller contract.
    unsafe {
        with_inner(context, |inner| {
            match inner.agent.push_log(timestamp_ms, level, target, message) {
                Ok(()) => ES_OK,
                Err(_) => ES_INVALID_ARGUMENT,
            }
        })
    }
}

fn aligned<T>(pointer: *const T, alignment: usize) -> bool {
    (pointer as usize).is_multiple_of(alignment)
}

fn valid_range(pointer: *const u8, len: usize) -> bool {
    len == 0
        || (!pointer.is_null()
            && (pointer as usize).checked_add(len).is_some()
            && len <= isize::MAX as usize)
}

fn overlaps(a: *const u8, a_len: usize, b: *const u8, b_len: usize) -> bool {
    if a_len == 0 || b_len == 0 {
        return false;
    }
    let (Some(a_end), Some(b_end)) = (
        (a as usize).checked_add(a_len),
        (b as usize).checked_add(b_len),
    ) else {
        return true;
    };
    (a as usize) < b_end && (b as usize) < a_end
}

fn overlaps_context(context: *mut c_void, pointer: *const u8, len: usize) -> bool {
    overlaps(context.cast(), mem::size_of::<Context>(), pointer, len)
}

fn valid_output(context: *mut c_void, output: *mut u32) -> bool {
    !output.is_null()
        && aligned(output, mem::align_of::<u32>())
        && valid_range(output.cast(), mem::size_of::<u32>())
        && !overlaps_context(context, output.cast(), mem::size_of::<u32>())
}

unsafe fn with_inner(context: *mut c_void, run: impl FnOnce(&mut Inner) -> i32) -> i32 {
    if context.is_null()
        || !aligned(context, mem::align_of::<Context>())
        || !valid_range(context.cast(), mem::size_of::<Context>())
    {
        return ES_INVALID_ARGUMENT;
    }
    let raw = context.cast::<Context>();
    // SAFETY: callers provide storage initialized by escope_init. Reading
    // magic alone also rejects a zero-filled, uninitialized allocation.
    if unsafe { ptr::addr_of!((*raw).magic).read() } != MAGIC {
        return ES_INVALID_ARGUMENT;
    }
    // SAFETY: magic establishes a previously initialized Context and its
    // allocation remains live; only shared access occurs outside UnsafeCell.
    let context = unsafe { &*raw };
    if context.busy.replace(true) {
        return ES_BUSY;
    }
    // SAFETY: busy grants the only mutable access to inner. A callback can
    // reenter, but it observes busy and never creates another mutable borrow.
    let result = run(unsafe { &mut *context.inner.get() });
    context.busy.set(false);
    result
}
