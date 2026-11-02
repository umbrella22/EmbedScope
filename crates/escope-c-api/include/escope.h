#ifndef EMBEDSCOPE_ESCOPE_H
#define EMBEDSCOPE_ESCOPE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ABI return codes. Remote operation status codes are separate, below. */
#define ES_OK 0
#define ES_INVALID_ARGUMENT 1
#define ES_CAPACITY 2
#define ES_BUSY 3
#define ES_PROTOCOL 4
#define ES_CALLBACK 5

#define ES_STATUS_OK 0
#define ES_STATUS_INVALID_ARGUMENT 1
#define ES_STATUS_UNSUPPORTED 2
#define ES_STATUS_SESSION 3
#define ES_STATUS_BUSY 4
#define ES_STATUS_STALE 5
#define ES_STATUS_CAPACITY 6
#define ES_STATUS_INTERNAL 7
#define ES_STATUS_VERSION 8
#define ES_STATUS_CANCELLED 9

#define ES_CAP_LOGS (1u << 0)
#define ES_CAP_METRICS (1u << 1)
#define ES_CAP_UI_TREE (1u << 2)
#define ES_CAP_INPUT (1u << 3)
#define ES_CAP_CAPTURE (1u << 4)
#define ES_CAP_CRASH (1u << 5)

typedef struct {
    const uint8_t *data;
    uint32_t len;
} EsBytes;

typedef struct {
    /* UTF-8 capacities in bytes: name 64, model 64, firmware 32, build_id 64.
       Oversized or invalid UTF-8 identity fields become protocol errors.
       Strings remain readable until the enclosing EmbedScope call returns;
       they are copied into context storage before further callbacks run. */
    EsBytes name;
    EsBytes model;
    EsBytes firmware;
    EsBytes build_id;
    uint32_t boot_id;
    uint64_t uptime_ms;
} EsDeviceInfo;

typedef uint32_t (*EsCapabilitiesFn)(void *user);
typedef int32_t (*EsInfoFn)(void *user, EsDeviceInfo *info);
/* Payload/reply use the CBOR schema at
   https://umbrella22.github.io/EmbedScope_website/en/reference/protocol
   Set reply_len only to initialized reply bytes. Never retain borrowed
   pointers or exceed reply_capacity. Return ES_STATUS_*, including
   Unsupported for unknown op.
   UI callbacks must execute on the application's UI thread. */
typedef uint16_t (*EsHandleFn)(void *user, uint16_t operation,
                             const uint8_t *payload, uint32_t payload_len,
                             uint8_t *reply, uint32_t reply_capacity,
                             uint32_t *reply_len);
typedef void (*EsResetFn)(void *user);

typedef struct {
    void *user;
    EsCapabilitiesFn capabilities; /* required */
    EsInfoFn info;                 /* required */
    EsHandleFn handle;             /* optional */
    EsResetFn reset;               /* optional: release capture/input */
} EsCallbacks;

/* Allocate size bytes at alignment, call init once, then keep that storage at
   the same address until all calls complete. The library does not allocate.
   Callback code/user data outlive the context. No destructor is required;
   call reset before releasing storage when backend resources are active.

   Serialize ALL calls per context, including log publishing from interrupts
   or other tasks. Callback reentry returns Busy; callbacks cannot unwind.
   Init must not reinitialize a live or currently executing context.

   Every nonempty input/output points to an initialized/readable or writable
   allocation of the stated size. Buffers/output counters/identity strings
   do not overlap the opaque context; feed input and consumed do not overlap,
   and take_tx output and written do not overlap. Zero-length byte slices may
   use NULL. Scalar output pointers are required and correctly aligned.
   Null, alignment, overlap, capacity and encoding errors are checked. The
   library cannot validate arbitrary non-null addresses or C callback writes.
   Except init/size/alignment, operations require initialized context storage. */
uint32_t escope_context_size(void);
uint32_t escope_context_alignment(void);
int32_t escope_init(void *storage, uint32_t storage_len,
                    const EsCallbacks *callbacks, uint32_t session_seed);

/* Feed any byte-stream fragments. Advance the source by consumed, including
   when Protocol is returned. Stops after the first completed frame.
   A full TX slot returns Busy and consumed=0. Drain TX before retrying the
   unconsumed suffix. Bad/oversized frames return Protocol at their delimiter;
   the next feed starts a fresh frame. Operation errors arrive as responses. */
int32_t escope_feed(void *context, const uint8_t *data, uint32_t len,
                    uint32_t *consumed);

/* Transfers at most one queued log event into the empty TX slot.
   Returns Busy when a response/event is already pending. */
int32_t escope_poll(void *context);

/* Copies a whole framed response/event. Capacity retains the pending frame
   and sets written to the required size. No pending frame gives written=0.
   A caller may query size with out=NULL, capacity=0. */
int32_t escope_take_tx(void *context, uint8_t *out, uint32_t capacity,
                       uint32_t *written);

/* Clears partial RX, pending TX and logs, releases backend capture/input,
   and advances the session. A new Hello is required after reset. */
int32_t escope_reset(void *context);

/* Error=1, Warn=2, Info=3, Debug=4, Trace=5. Strings are UTF-8. The Agent's
   FIFO holds 16 records; target/message prefixes are at most 48/192 bytes.
   A truncated record carries truncated=true. Overflow discards new records
   and the next delivered event reports cumulative dropped records. */
int32_t escope_push_log(void *context, uint64_t timestamp_ms, uint8_t level,
                        EsBytes target, EsBytes message);

#ifdef __cplusplus
}
#endif
#endif
