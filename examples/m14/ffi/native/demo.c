#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>

typedef struct { double x; double y; } CPoint;

int32_t demo_add(int32_t a, int32_t b) { return a + b; }

int32_t demo_fill(uint8_t *out, size_t len) {
    for (size_t i = 0; i < len; i++) out[i] = (uint8_t)(i + 1);
    return 0;
}

void *demo_create(void) { return malloc(16); }
void demo_destroy(void *handle) { free(handle); }

double demo_translate_x(CPoint *point, double dx) {
    point->x += dx;
    return point->x;
}

size_t demo_sizeof_point(void) { return sizeof(CPoint); }
