#define _POSIX_C_SOURCE 200112L
#include "escope.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct { const uint8_t *data; size_t len; size_t at; } Reader;
typedef struct {
    uint32_t session;
    uint32_t id;
    uint32_t operation;
    uint32_t status;
    uint32_t kind;
    const uint8_t *payload;
    uint32_t payload_len;
} Response;
typedef struct {
    void *context;
    uint32_t resets;
    uint32_t infos;
    uint32_t handled;
    uint32_t bad_length;
    uint32_t bad_info;
    uint32_t bad_status;
} State;

static EsBytes text(const char *s) {
    EsBytes value = { (const uint8_t *)s, (uint32_t)strlen(s) };
    return value;
}

static uint32_t crc32(const uint8_t *data, size_t len) {
    uint32_t crc = UINT32_MAX;
    for (size_t i = 0; i < len; ++i) {
        crc ^= data[i];
        for (unsigned bit = 0; bit < 8; ++bit)
            crc = (crc >> 1) ^ (0xedb88320u & (0u - (crc & 1u)));
    }
    return ~crc;
}

static void put_uint(uint8_t *out, size_t *n, uint8_t major, uint32_t value) {
    if (value < 24) out[(*n)++] = (uint8_t)((major << 5) | value);
    else if (value <= UINT8_MAX) { out[(*n)++] = (uint8_t)((major << 5) | 24); out[(*n)++] = (uint8_t)value; }
    else if (value <= UINT16_MAX) {
        out[(*n)++] = (uint8_t)((major << 5) | 25);
        out[(*n)++] = (uint8_t)(value >> 8); out[(*n)++] = (uint8_t)value;
    } else {
        out[(*n)++] = (uint8_t)((major << 5) | 26);
        for (int shift = 24; shift >= 0; shift -= 8) out[(*n)++] = (uint8_t)(value >> shift);
    }
}

static size_t request(uint32_t session, uint32_t operation, uint8_t *out) {
    uint8_t packet[128]; size_t n = 0;
    packet[n++] = 0xa7;
    const uint32_t values[] = { 1, 0, session, 1, operation, 0 };
    for (uint32_t key = 0; key < 6; ++key) { put_uint(packet, &n, 0, key); put_uint(packet, &n, 0, values[key]); }
    packet[n++] = 6; packet[n++] = 0x40;
    uint32_t checksum = crc32(packet, n);
    for (unsigned shift = 0; shift < 32; shift += 8) packet[n++] = (uint8_t)(checksum >> shift);
    size_t block = 0, written = 1; uint8_t code = 1;
    for (size_t i = 0; i < n; ++i) {
        if (packet[i] == 0) { out[block] = code; block = written++; code = 1; }
        else { out[written++] = packet[i]; if (++code == 255) { out[block] = code; block = written++; code = 1; } }
    }
    out[block] = code; out[written++] = 0;
    return written;
}

static uint64_t header(Reader *reader, uint8_t *major) {
    assert(reader->at < reader->len);
    uint8_t byte = reader->data[reader->at++]; *major = byte >> 5;
    uint8_t extra = byte & 31; if (extra < 24) return extra;
    unsigned count = extra == 24 ? 1 : extra == 25 ? 2 : extra == 26 ? 4 : extra == 27 ? 8 : 0;
    assert(count && reader->at + count <= reader->len);
    uint64_t value = 0;
    for (unsigned i = 0; i < count; ++i) value = (value << 8) | reader->data[reader->at++];
    return value;
}

static uint32_t number(Reader *reader) {
    uint8_t major; uint64_t value = header(reader, &major);
    assert(major == 0 && value <= UINT32_MAX); return (uint32_t)value;
}

static void skip(Reader *reader) {
    uint8_t major; uint64_t n = header(reader, &major);
    if (major == 2 || major == 3) { assert(n <= reader->len - reader->at); reader->at += (size_t)n; }
    else if (major == 4 || major == 5) { if (major == 5) n *= 2; for (uint64_t i = 0; i < n; ++i) skip(reader); }
    else assert(major == 0 || major == 1 || major == 7);
}

static Reader field(const Response *response, uint32_t wanted) {
    Reader r = { response->payload, response->payload_len, 0 }; uint8_t major;
    uint64_t n = header(&r, &major); assert(major == 5);
    for (uint64_t i = 0; i < n; ++i) { uint32_t key = number(&r); if (key == wanted) return r; skip(&r); }
    assert(!"missing CBOR map field"); return r;
}

static Response response(const uint8_t *frame, uint32_t len, uint8_t *decoded) {
    assert(len > 1 && frame[len - 1] == 0); size_t at = 0, n = 0;
    while (at < len - 1) {
        uint8_t code = frame[at++]; assert(code && at + code - 1 <= len - 1);
        for (unsigned i = 1; i < code; ++i) decoded[n++] = frame[at++];
        if (code < 255 && at < len - 1) decoded[n++] = 0;
    }
    assert(n >= 4); n -= 4;
    uint32_t checksum = (uint32_t)decoded[n] | ((uint32_t)decoded[n + 1] << 8)
        | ((uint32_t)decoded[n + 2] << 16) | ((uint32_t)decoded[n + 3] << 24);
    assert(checksum == crc32(decoded, n));
    Reader r = { decoded, n, 0 }; uint8_t major; uint64_t fields = header(&r, &major); assert(major == 5);
    Response result = { 0 };
    for (uint64_t i = 0; i < fields; ++i) {
        uint32_t key = number(&r);
        if (key == 0) assert(number(&r) == 1);
        else if (key == 1) result.kind = number(&r);
        else if (key == 2) result.session = number(&r);
        else if (key == 3) result.id = number(&r);
        else if (key == 4) result.operation = number(&r);
        else if (key == 5) result.status = number(&r);
        else if (key == 6) {
            uint64_t bytes = header(&r, &major); assert(major == 2 && bytes <= r.len - r.at);
            result.payload = r.data + r.at; result.payload_len = (uint32_t)bytes; r.at += (size_t)bytes;
        } else skip(&r);
    }
    assert(r.at == r.len); return result;
}

static uint32_t capabilities(void *user) { (void)user; return ES_CAP_LOGS | ES_CAP_METRICS; }
static int32_t info(void *user, EsDeviceInfo *output) {
    State *state = user; ++state->infos;
    assert(escope_poll(state->context) == ES_BUSY); /* safe callback reentry */
    output->name = text("C harness"); output->model = text("native-c");
    output->firmware = text("0.1.0"); output->build_id = text("c-harness-build");
    output->boot_id = 7; output->uptime_ms = 42;
    if (state->bad_info) output->name.len = 65;
    return ES_OK;
}
static uint16_t handle(void *user, uint16_t operation, const uint8_t *payload, uint32_t payload_len,
                       uint8_t *reply, uint32_t capacity, uint32_t *written) {
    State *state = user; ++state->handled; (void)payload; (void)payload_len;
    if (state->bad_status) return UINT16_MAX;
    if (state->bad_length) { *written = capacity + 1; return ES_STATUS_OK; }
    if (operation != 3) return ES_STATUS_UNSUPPORTED;
    const uint8_t metrics[] = { 0xa2, 0, 0x18, 42, 7, 0 };
    assert(capacity >= sizeof(metrics)); memcpy(reply, metrics, sizeof(metrics)); *written = sizeof(metrics);
    return ES_STATUS_OK;
}
static void reset(void *user) { ++((State *)user)->resets; }

static Response exchange(State *state, uint32_t session, uint32_t op, uint8_t *decoded) {
    uint8_t input[128], output[2048]; uint32_t consumed, written;
    size_t len = request(session, op, input);
    /* Every request arrives as one-byte fragments. */
    for (size_t i = 0; i < len; ++i) {
        assert(escope_feed(state->context, input + i, 1, &consumed) == ES_OK && consumed == 1);
    }
    assert(escope_take_tx(state->context, output, sizeof(output), &written) == ES_OK);
    return response(output, written, decoded);
}

int main(void) {
    uint32_t size = escope_context_size(), alignment = escope_context_alignment();
    assert(size > 0 && alignment > 0); void *context = NULL;
    size_t allocator_alignment = alignment < sizeof(void *) ? sizeof(void *) : alignment;
    assert(posix_memalign(&context, allocator_alignment, size + alignment) == 0);
    memset(context, 0, size + alignment);
    State state = { .context = context };
    EsCallbacks callbacks = { &state, capabilities, info, handle, reset };
    uint32_t consumed = 99, written = 99;
    assert(escope_poll(NULL) == ES_INVALID_ARGUMENT);
    assert(escope_poll(context) == ES_INVALID_ARGUMENT);
    assert(escope_init(NULL, size, &callbacks, 1) == ES_INVALID_ARGUMENT);
    assert(escope_init(context, size - 1, &callbacks, 1) == ES_INVALID_ARGUMENT);
    assert(escope_init((uint8_t *)context + 1, size, &callbacks, 1) == ES_INVALID_ARGUMENT);
    assert(escope_init(context, size, NULL, 1) == ES_INVALID_ARGUMENT);
    assert(escope_init(context, size, &callbacks, 0) == ES_INVALID_ARGUMENT);
    assert(escope_init(context, size, &callbacks, 1) == ES_OK);
    assert(escope_feed(context, NULL, 1, &consumed) == ES_INVALID_ARGUMENT);
    assert(escope_feed(context, NULL, 0, &consumed) == ES_OK && consumed == 0);
    assert(escope_feed(context, NULL, 0, NULL) == ES_INVALID_ARGUMENT);
    assert(escope_take_tx(context, NULL, 1, &written) == ES_INVALID_ARGUMENT);
    assert(escope_feed(context, context, 1, &consumed) == ES_INVALID_ARGUMENT);

    uint8_t decoded[2048], input[128], output[2048];
    Response hello = exchange(&state, 0, 1, decoded);
    assert(hello.kind == 1 && hello.status == ES_STATUS_OK && hello.session == 1 && state.resets == 1);
    Reader r = field(&hello, 0); assert(number(&r) == 1);
    r = field(&hello, 1); assert(number(&r) == (ES_CAP_LOGS | ES_CAP_METRICS));

    size_t len = request(1, 2, input);
    assert(escope_feed(context, input, (uint32_t)len, &consumed) == ES_OK && consumed == len);
    assert(escope_feed(context, input, (uint32_t)len, &consumed) == ES_BUSY && consumed == 0);
    assert(escope_poll(context) == ES_BUSY);
    assert(escope_take_tx(context, NULL, 0, &written) == ES_CAPACITY && written > 0);
    uint32_t required = written;
    assert(escope_take_tx(context, output, required - 1, &written) == ES_CAPACITY && written == required);
    assert(escope_take_tx(context, output, sizeof(output), &written) == ES_OK && written == required);
    Response device = response(output, written, decoded); assert(device.status == ES_STATUS_OK && device.operation == 2);
    r = field(&device, 0); uint8_t major; assert(header(&r, &major) == 9 && major == 3);
    assert(memcmp(r.data + r.at, "C harness", 9) == 0);
    r = field(&device, 4); assert(number(&r) == 7);
    assert(escope_take_tx(context, output, sizeof(output), &written) == ES_OK && written == 0);

    const uint8_t bad[] = { 2, 0 };
    assert(escope_feed(context, bad, sizeof(bad), &consumed) == ES_PROTOCOL && consumed == sizeof(bad));
    Response metrics = exchange(&state, 1, 3, decoded);
    assert(metrics.status == ES_STATUS_OK && state.handled == 1);
    r = field(&metrics, 0); assert(number(&r) == 42);
    state.bad_length = 1; metrics = exchange(&state, 1, 3, decoded);
    assert(metrics.status == ES_STATUS_INTERNAL); state.bad_length = 0;
    state.bad_status = 1; metrics = exchange(&state, 1, 3, decoded);
    assert(metrics.status == ES_STATUS_INTERNAL); state.bad_status = 0;
    state.bad_info = 1; device = exchange(&state, 1, 2, decoded);
    assert(device.status == ES_STATUS_CAPACITY); state.bad_info = 0;
    device = exchange(&state, 1, 2, decoded); assert(device.status == ES_STATUS_OK);

    uint8_t burst[256]; size_t first_len = request(1, 2, burst);
    size_t second_len = request(1, 2, burst + first_len);
    assert(escope_feed(context, burst, (uint32_t)(first_len + second_len), &consumed) == ES_OK && consumed == first_len);
    assert(escope_take_tx(context, output, sizeof(output), &written) == ES_OK && written > 0);
    assert(escope_feed(context, burst + first_len, (uint32_t)second_len, &consumed) == ES_OK && consumed == second_len);
    assert(escope_take_tx(context, output, sizeof(output), &written) == ES_OK && written > 0);

    Response subscription = exchange(&state, 1, 4, decoded); assert(subscription.status == ES_STATUS_OK);
    assert(escope_push_log(context, 42, 3, text("c"), text("hello from C")) == ES_OK);
    const uint8_t invalid_utf8[] = { 0xff }; EsBytes invalid = { invalid_utf8, 1 };
    assert(escope_push_log(context, 42, 3, invalid, text("invalid")) == ES_INVALID_ARGUMENT);
    assert(escope_poll(context) == ES_OK);
    assert(escope_take_tx(context, output, sizeof(output), &written) == ES_OK);
    Response event = response(output, written, decoded); assert(event.kind == 2 && event.id == 0 && event.operation == 13);
    r = field(&event, 4); assert(header(&r, &major) == 12 && major == 3);
    assert(memcmp(r.data + r.at, "hello from C", 12) == 0);

    assert(escope_reset(context) == ES_OK && state.resets == 2);
    Response stale = exchange(&state, 1, 2, decoded); assert(stale.status == ES_STATUS_SESSION);
    hello = exchange(&state, 0, 1, decoded); assert(hello.status == ES_STATUS_OK && hello.session == 2);
    device = exchange(&state, 2, 2, decoded); assert(device.status == ES_STATUS_OK);
    assert(escope_reset(context) == ES_OK); free(context);
    printf("C ABI harness passed: Hello/Info/Metrics/logs, fragments, backpressure, invalid args, callback reentry, reset (%u bytes, align %u)\n", size, alignment);
    return 0;
}
