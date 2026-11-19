#ifndef ESCOPE_C_LVGL_TREE_H
#define ESCOPE_C_LVGL_TREE_H

#include <lvgl/lvgl.h>
#include "escope.h"

typedef struct {
    lv_obj_t *object;
    uint32_t parent;
    const char *kind;
    const char *name;
} DemoNode;

uint16_t tree_reply(const DemoNode *nodes, uint32_t count, uint32_t revision,
                    const uint8_t *request, uint32_t len, uint8_t *reply,
                    uint32_t capacity, uint32_t *written);

#endif
