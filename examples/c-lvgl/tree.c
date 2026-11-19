#include "tree.h"
#include <stdbool.h>
#include <string.h>

/* Only the CBOR map/uint input and map/int/text output needed by UiTree. */
typedef struct { const uint8_t *next; uint32_t left; } Reader;
typedef struct { uint8_t *next; uint32_t left; } Writer;

static bool read_uint(Reader *r, uint32_t *value) {
    if (r->left == 0 || (*r->next >> 5) != 0) return false;
    const uint8_t info = *r->next++;
    r->left--;
    if (info < 24) { *value = info; return true; }
    if (info > 26) return false;
    uint32_t bytes = 1u << (info - 24);
    if (r->left < bytes) return false;
    *value = 0;
    r->left -= bytes;
    while (bytes-- != 0) *value = (*value << 8) | *r->next++;
    return true;
}

static bool put(Writer *w, uint8_t byte) {
    if (w->left == 0) return false;
    *w->next++ = byte;
    w->left--;
    return true;
}

static bool number(Writer *w, uint8_t major, uint32_t value) {
    if (value < 24) return put(w, major | (uint8_t)value);
    unsigned bytes = value <= UINT8_MAX ? 1 : value <= UINT16_MAX ? 2 : 4;
    if (!put(w, major | (bytes == 1 ? 24 : bytes == 2 ? 25 : 26))) return false;
    while (bytes != 0) {
        if (!put(w, (uint8_t)(value >> (8 * --bytes)))) return false;
    }
    return true;
}

static bool integer(Writer *w, int32_t value) {
    return value < 0 ? number(w, 0x20, (uint32_t)(-1 - value)) : number(w, 0, (uint32_t)value);
}

static bool text(Writer *w, const char *value) {
    size_t len = strlen(value);
    if (len > w->left || !number(w, 0x60, (uint32_t)len) || len > w->left) return false;
    memcpy(w->next, value, len);
    w->next += len;
    w->left -= (uint32_t)len;
    return true;
}

uint16_t tree_reply(const DemoNode *nodes, uint32_t count, uint32_t revision,
                    const uint8_t *request, uint32_t len, uint8_t *reply,
                    uint32_t capacity, uint32_t *written) {
    /* TreeQuery map {0: cursor, 1: revision}. The SDK emits a two-entry map. */
    if (len == 0 || request[0] != 0xa2) return ES_STATUS_INVALID_ARGUMENT;
    Reader reader = { request + 1, len - 1 };
    uint32_t a, b, first, second;
    if (!read_uint(&reader, &a) || !read_uint(&reader, &first) ||
        !read_uint(&reader, &b) || !read_uint(&reader, &second) ||
        a > 1 || b > 1 || a == b || reader.left != 0) return ES_STATUS_INVALID_ARGUMENT;
    uint32_t cursor = a == 0 ? first : second;
    uint32_t expected = a == 1 ? first : second;
    if (expected != 0 && expected != revision) return ES_STATUS_STALE;
    if (cursor >= count) return ES_STATUS_INVALID_ARGUMENT;

    const DemoNode *node = &nodes[cursor];
    lv_area_t bounds;
    lv_obj_get_coords(node->object, &bounds);
    Writer out = { reply, capacity };
    bool ok = put(&out, 0xab) &&
        put(&out, 0) && number(&out, 0, cursor + 1) &&
        put(&out, 1) && number(&out, 0, node->parent) &&
        put(&out, 2) && text(&out, node->kind) &&
        put(&out, 3) && text(&out, node->name) &&
        put(&out, 4) && integer(&out, bounds.x1) &&
        put(&out, 5) && integer(&out, bounds.y1) &&
        put(&out, 6) && number(&out, 0, (uint32_t)lv_obj_get_width(node->object)) &&
        put(&out, 7) && number(&out, 0, (uint32_t)lv_obj_get_height(node->object)) &&
        put(&out, 8) && number(&out, 0, lv_obj_get_state(node->object)) &&
        put(&out, 9) && number(&out, 0, revision) && put(&out, 10) &&
        (cursor + 1 < count ? number(&out, 0, cursor + 1) : put(&out, 0xf6));
    if (!ok) return ES_STATUS_CAPACITY;
    *written = capacity - out.left;
    return ES_STATUS_OK;
}
