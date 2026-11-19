#include "tree.h"
#include <arpa/inet.h>
#include <errno.h>
#include <signal.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

#if LVGL_VERSION_MAJOR != 9 || LVGL_VERSION_MINOR != 6
#error This reference application targets LVGL 9.6
#endif

enum { WIDTH = 320, HEIGHT = 240, UI_TREE = 6, NODE_COUNT = 4 };
typedef struct { DemoNode nodes[NODE_COUNT]; uint32_t boot; uint64_t started; } Demo;
#define BYTES(s) ((EsBytes){ (const uint8_t *)(s), sizeof(s) - 1 })

static uint64_t ms(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) abort();
    return (uint64_t)ts.tv_sec * 1000 + (uint64_t)ts.tv_nsec / 1000000;
}

static uint32_t capabilities(void *user) { (void)user; return ES_CAP_UI_TREE | ES_CAP_LOGS; }

static int32_t info(void *user, EsDeviceInfo *out) {
    Demo *demo = user;
    *out = (EsDeviceInfo){ BYTES("C LVGL display settings"), BYTES("LVGL 9.6 / C host"),
        BYTES("reference"), BYTES("c-lvgl-example"), demo->boot, ms() - demo->started };
    return ES_OK;
}

static uint16_t handle(void *user, uint16_t op, const uint8_t *request, uint32_t len,
                       uint8_t *reply, uint32_t capacity, uint32_t *written) {
    Demo *demo = user;
    if (op != UI_TREE) return ES_STATUS_UNSUPPORTED;
    /* The read-only application has no tree or geometry mutation after mount. */
    return tree_reply(demo->nodes, NODE_COUNT, 1, request, len, reply, capacity, written);
}

static void flush(lv_display_t *display, const lv_area_t *area, uint8_t *pixels) {
    (void)area;
    (void)pixels;
    lv_display_flush_ready(display);
}

static lv_display_t *mount(Demo *demo) {
    _Alignas(LV_DRAW_BUF_ALIGN) static uint8_t pixels[WIDTH * 16 * 2];
    lv_display_t *display = lv_display_create(WIDTH, HEIGHT);
    if (display == NULL) return NULL;
    lv_display_set_color_format(display, LV_COLOR_FORMAT_RGB565);
    lv_display_set_buffers(display, pixels, NULL, sizeof(pixels), LV_DISPLAY_RENDER_MODE_PARTIAL);
    lv_display_set_flush_cb(display, flush);
    lv_obj_t *screen = lv_display_get_screen_active(display);
    if (screen == NULL) goto fail;
    lv_obj_remove_style_all(screen);
    lv_obj_set_scrollable(screen, false);
    lv_obj_t *caption = lv_label_create(screen);
    lv_obj_t *button = lv_button_create(screen);
    if (caption == NULL || button == NULL) goto fail;
    lv_obj_t *label = lv_label_create(button);
    if (label == NULL) goto fail;
    lv_label_set_text(caption, "Display settings");
    lv_obj_set_pos(caption, 20, 20);
    lv_obj_set_pos(button, 20, 60);
    lv_obj_set_size(button, 140, 40);
    lv_label_set_text(label, "Close");
    lv_obj_center(label);
    lv_obj_update_layout(screen);
    lv_refr_now(display);
    demo->nodes[0] = (DemoNode){ screen, 0, "screen", "settings" };
    demo->nodes[1] = (DemoNode){ caption, 1, "label", "settings.title" };
    demo->nodes[2] = (DemoNode){ button, 1, "button", "settings.close" };
    demo->nodes[3] = (DemoNode){ label, 3, "label", "settings.close-label" };
    return display;
fail:
    lv_display_delete(display);
    return NULL;
}

static bool send_pending(int fd, void *context) {
    uint8_t tx[2048]; /* The v1 protocol frame is smaller than this buffer. */
    uint32_t written;
    if (escope_take_tx(context, tx, sizeof(tx), &written) != ES_OK) return false;
    size_t sent = 0;
    while (sent < written) {
        ssize_t n = send(fd, tx + sent, written - sent, 0);
        if (n < 0 && errno == EINTR) continue;
        if (n <= 0) return false;
        sent += (size_t)n;
    }
    return true;
}

static bool serve(int fd, void *context, Demo *demo) {
    uint8_t rx[1024];
    for (;;) {
        ssize_t n = recv(fd, rx, sizeof(rx), 0);
        if (n < 0 && errno == EINTR) continue;
        if (n <= 0) return n == 0;
        uint32_t off = 0;
        while (off < (uint32_t)n) {
            uint32_t used;
            int32_t status = escope_feed(context, rx + off, (uint32_t)n - off, &used);
            if ((status != ES_OK && status != ES_PROTOCOL) || used == 0 ||
                !send_pending(fd, context)) return false;
            off += used;
        }
        /* Logs are accepted only after subscription. This static reference
           does not simulate other device capabilities or produce idle traffic. */
        if (escope_push_log(context, ms() - demo->started, 3,
                            BYTES("c-lvgl"), BYTES("LVGL tree ready")) != ES_OK ||
            escope_poll(context) != ES_OK || !send_pending(fd, context)) return false;
    }
}

int main(int argc, char **argv) {
    bool once = argc == 2 && strcmp(argv[1], "--once") == 0;
    if (argc > 1 && !once) return EXIT_FAILURE;
    if (signal(SIGPIPE, SIG_IGN) == SIG_ERR) return EXIT_FAILURE;

    struct sockaddr_in address = {0};
    address.sin_family = AF_INET;
    address.sin_port = htons(once ? 0 : 9340);
    if (inet_pton(AF_INET, "127.0.0.1", &address.sin_addr) != 1) {
        fputs("could not encode IPv4 loopback address\n", stderr);
        return EXIT_FAILURE;
    }
    int listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) return EXIT_FAILURE;
    socklen_t len = sizeof(address);
    if (bind(listener, (const struct sockaddr *)&address, len) != 0 || listen(listener, 1) != 0 ||
        getsockname(listener, (struct sockaddr *)&address, &len) != 0) {
        perror("loopback listener");
        close(listener);
        return EXIT_FAILURE;
    }

    Demo demo = { .started = ms() };
    demo.boot = ((uint32_t)demo.started ^ (uint32_t)getpid()) | 1;
    uint32_t alignment = escope_context_alignment();
    if (alignment < sizeof(void *)) alignment = sizeof(void *);
    uint32_t size = escope_context_size();
    void *context = NULL;
    int result = EXIT_FAILURE;
    lv_init();
    lv_display_t *display = mount(&demo);
    const EsCallbacks callbacks = { &demo, capabilities, info, handle, NULL };
    if (display != NULL && posix_memalign(&context, alignment, size) == 0 &&
        escope_init(context, size, &callbacks, demo.boot) == ES_OK) {
        printf("LISTEN tcp://127.0.0.1:%u\n", (unsigned)ntohs(address.sin_port));
        fflush(stdout);
        do {
            int client = accept(listener, NULL, NULL);
            if (client < 0) break;
            bool clean = serve(client, context, &demo);
            close(client);
            if (escope_reset(context) != ES_OK) break;
            if (once && clean) result = EXIT_SUCCESS;
        } while (!once);
    }
    free(context);
    if (display != NULL) lv_display_delete(display);
    lv_deinit();
    close(listener);
    return result;
}
