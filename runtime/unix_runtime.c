#include <stddef.h>
#include <stdint.h>
#include <unistd.h>
#include <stdio.h>
#include <string.h>
#include <signal.h>
#include <stdlib.h>

#define DOLPHIN_EXIT_TRAP 101
#define DOLPHIN_EXIT_ALLOC 102
#define DOLPHIN_EXIT_INVALID_FREE 103
#define DOLPHIN_EXIT_UTF8 104

static void dolphin_write_all(const char *data, size_t length) {
    while (length > 0) {
        ssize_t written = write(1, data, length);
        if (written <= 0) {
            return;
        }
        data += written;
        length -= (size_t)written;
    }
}

static void dolphin_write_error(const char *data, size_t length) {
    while (length > 0) {
        ssize_t written = write(2, data, length);
        if (written <= 0) {
            return;
        }
        data += written;
        length -= (size_t)written;
    }
}

/// 直接以指定退出码结束进程。运行时不展开 Dolphin 栈，因此不执行 defer。
static void dolphin_exit_with(int code, const char *message) {
    dolphin_write_error(message, strlen(message));
    _Exit(code);
}

static void dolphin_runtime_trap(int signal_number) {
    (void)signal_number;
    const char message[] = "Dolphin runtime error: overflow, division by zero, or array bounds violation\n";
    write(2, message, sizeof(message) - 1);
    _Exit(DOLPHIN_EXIT_TRAP);
}

__attribute__((constructor)) static void dolphin_install_traps(void) {
    signal(SIGILL, dolphin_runtime_trap);
    signal(SIGFPE, dolphin_runtime_trap);
    signal(SIGTRAP, dolphin_runtime_trap);
}

/* ---------------------------------------------------------------------------
 * 分配器与 Debug 存活分配登记表（M14-C/R04）。
 *
 * Debug 版维护独立链表，记录地址、字节数与对齐；free 先查表再调用系统 free，
 * 因此不会读取用户地址或已释放内存的 header。Release 版不追踪。
 * ------------------------------------------------------------------------- */

#ifdef DOLPHIN_DEBUG_RUNTIME
struct dolphin_alloc_record {
    void *pointer;
    size_t bytes;
    size_t align;
    struct dolphin_alloc_record *next;
};

static struct dolphin_alloc_record *dolphin_live_allocs = NULL;

static void dolphin_register_alloc(void *pointer, size_t bytes, size_t align) {
    struct dolphin_alloc_record *record =
        (struct dolphin_alloc_record *)malloc(sizeof(*record));
    if (record == NULL) {
        dolphin_exit_with(DOLPHIN_EXIT_ALLOC,
                          "Dolphin runtime error: out of memory (allocation table)\n");
    }
    record->pointer = pointer;
    record->bytes = bytes;
    record->align = align;
    record->next = dolphin_live_allocs;
    dolphin_live_allocs = record;
}

/// 返回 1 表示找到并移除了匹配记录；0 表示未知地址或长度不匹配。
static int dolphin_unregister_alloc(void *pointer, size_t bytes) {
    struct dolphin_alloc_record **link = &dolphin_live_allocs;
    while (*link != NULL) {
        if ((*link)->pointer == pointer) {
            if ((*link)->bytes != bytes) {
                return 0;
            }
            struct dolphin_alloc_record *dead = *link;
            *link = dead->next;
            free(dead);
            return 1;
        }
        link = &(*link)->next;
    }
    return 0;
}

void dolphin_runtime_finish(void) {
    size_t leaked = 0;
    for (struct dolphin_alloc_record *record = dolphin_live_allocs; record != NULL;
         record = record->next) {
        leaked += 1;
    }
    if (leaked == 0) {
        return;
    }
    char buffer[160];
    int length = snprintf(buffer, sizeof(buffer),
                          "Dolphin: %llu allocation(s) leaked at exit\n",
                          (unsigned long long)leaked);
    if (length > 0) {
        dolphin_write_error(buffer, (size_t)length);
    }
    for (struct dolphin_alloc_record *record = dolphin_live_allocs; record != NULL;
         record = record->next) {
        length = snprintf(buffer, sizeof(buffer), "  address=%p bytes=%llu\n", record->pointer,
                          (unsigned long long)record->bytes);
        if (length > 0) {
            dolphin_write_error(buffer, (size_t)length);
        }
    }
}
#else
static void dolphin_register_alloc(void *pointer, size_t bytes, size_t align) {
    (void)pointer;
    (void)bytes;
    (void)align;
}

static int dolphin_unregister_alloc(void *pointer, size_t bytes) {
    (void)pointer;
    (void)bytes;
    return 1;
}

void dolphin_runtime_finish(void) {}
#endif

static uintptr_t dolphin_allocations = 0;

/// 测试专用分配失败注入：`DOLPHIN_TEST_ALLOC_LIMIT=N` 时第 N 次起分配失败。
/// 未设置时返回 -1，不启用。
static long dolphin_test_alloc_limit(void) {
    static int initialized = 0;
    static long limit = -1;
    if (!initialized) {
        const char *value = getenv("DOLPHIN_TEST_ALLOC_LIMIT");
        limit = value != NULL ? atol(value) : -1;
        initialized = 1;
    }
    return limit;
}

void *dolphin_alloc(uintptr_t count, uintptr_t elem_size, uintptr_t align) {
    if (count == 0 || elem_size == 0) {
        /* 规范空分配 `{null, 0}`，不登记。 */
        return NULL;
    }
    if (count > UINTPTR_MAX / elem_size) {
        dolphin_exit_with(DOLPHIN_EXIT_ALLOC,
                          "Dolphin runtime error: allocation size overflow\n");
    }
    long limit = dolphin_test_alloc_limit();
    if (limit >= 0 && dolphin_allocations >= (uintptr_t)limit) {
        dolphin_exit_with(DOLPHIN_EXIT_ALLOC, "Dolphin runtime error: out of memory\n");
    }
    dolphin_allocations += 1;
    size_t bytes = (size_t)(count * elem_size);
    void *pointer = malloc(bytes);
    if (pointer == NULL) {
        dolphin_exit_with(DOLPHIN_EXIT_ALLOC, "Dolphin runtime error: out of memory\n");
    }
    dolphin_register_alloc(pointer, bytes, (size_t)align);
    return pointer;
}

void dolphin_free(void *pointer, uintptr_t bytes) {
    if (pointer == NULL) {
        /* `free({null, 0})` 与 `destroy(null)` 是无操作。 */
        return;
    }
    if (!dolphin_unregister_alloc(pointer, (size_t)bytes)) {
        dolphin_exit_with(DOLPHIN_EXIT_INVALID_FREE, "Dolphin runtime error: invalid free\n");
    }
    free(pointer);
}

void dolphin_copy(void *destination, const void *source, uintptr_t bytes) {
    if (bytes != 0) {
        memmove(destination, source, (size_t)bytes);
    }
}

void dolphin_check_align(void *pointer, uintptr_t align) {
    if (align != 0 && ((uintptr_t)pointer % align) != 0) {
        dolphin_exit_with(DOLPHIN_EXIT_TRAP, "Dolphin runtime error: misaligned pointer\n");
    }
}

void dolphin_check_view(const void *pointer, uintptr_t length) {
    if (pointer == NULL && length != 0) {
        dolphin_exit_with(DOLPHIN_EXIT_TRAP,
                          "Dolphin runtime error: null pointer with non-zero view length\n");
    }
}

uint8_t dolphin_is_valid_utf8(const uint8_t *bytes, uintptr_t length) {
    uintptr_t index = 0;
    while (index < length) {
        uint8_t lead = bytes[index];
        if (lead < 0x80) {
            index += 1;
            continue;
        }
        uintptr_t continuation;
        uint32_t codepoint;
        uint32_t minimum;
        if ((lead & 0xe0) == 0xc0) {
            continuation = 1;
            codepoint = lead & 0x1f;
            minimum = 0x80;
        } else if ((lead & 0xf0) == 0xe0) {
            continuation = 2;
            codepoint = lead & 0x0f;
            minimum = 0x800;
        } else if ((lead & 0xf8) == 0xf0) {
            continuation = 3;
            codepoint = lead & 0x07;
            minimum = 0x10000;
        } else {
            return 0;
        }
        if (index + continuation >= length) {
            return 0;
        }
        for (uintptr_t offset = 1; offset <= continuation; offset += 1) {
            uint8_t next = bytes[index + offset];
            if ((next & 0xc0) != 0x80) {
                return 0;
            }
            codepoint = (codepoint << 6) | (next & 0x3f);
        }
        if (codepoint < minimum || codepoint > 0x10ffff) {
            return 0;
        }
        if (codepoint >= 0xd800 && codepoint <= 0xdfff) {
            return 0;
        }
        index += continuation + 1;
    }
    return 1;
}

void dolphin_check_utf8(const uint8_t *bytes, uintptr_t length) {
    if (!dolphin_is_valid_utf8(bytes, length)) {
        dolphin_exit_with(DOLPHIN_EXIT_UTF8, "Dolphin runtime error: invalid UTF-8\n");
    }
}

/* ---------------------------------------------------------------------------
 * 进程参数与环境（M19/H19-01）。
 *
 * 生成的 C 入口 `main(i32, void *)` 在函数入口调用 `dolphin_init_args`，因此
 * `dolphin_argv` 指向进程原始 argv，视图有效到进程结束。错误类别编号与
 * 规格 `std.error.ErrorKind` 的稳定判别值一致（0..6）。
 * ------------------------------------------------------------------------- */

#define DOLPHIN_ERR_OTHER 0
#define DOLPHIN_ERR_NOT_FOUND 1
#define DOLPHIN_ERR_PERMISSION 2
#define DOLPHIN_ERR_IS_DIR 3
#define DOLPHIN_ERR_INVALID 4
#define DOLPHIN_ERR_NOT_OWNED 5
#define DOLPHIN_ERR_CLOSED 6

static int dolphin_error_kind = 0;
static int dolphin_error_code = 0;

static void dolphin_set_error(int kind, int code) {
    dolphin_error_kind = kind;
    dolphin_error_code = code;
}

static int dolphin_argc = 0;
static char **dolphin_argv = NULL;

void dolphin_init_args(int argc, char **argv) {
    dolphin_argc = argc;
    dolphin_argv = argv;
    dolphin_set_error(DOLPHIN_ERR_OTHER, 0);
}

uintptr_t dolphin_arg_count(void) {
    return (uintptr_t)dolphin_argc;
}

const uint8_t *dolphin_arg(uintptr_t index, uintptr_t *out_len) {
    if (dolphin_argv == NULL || index >= (uintptr_t)dolphin_argc) {
        dolphin_set_error(DOLPHIN_ERR_NOT_FOUND, 0);
        return NULL;
    }
    const char *argument = dolphin_argv[index];
    size_t length = strlen(argument);
    if (!dolphin_is_valid_utf8((const uint8_t *)argument, (uintptr_t)length)) {
        dolphin_set_error(DOLPHIN_ERR_INVALID, 0);
        return NULL;
    }
    *out_len = (uintptr_t)length;
    return (const uint8_t *)argument;
}

const uint8_t *dolphin_env(const uint8_t *name, uintptr_t name_len, uintptr_t *out_len) {
    char stack_buffer[256];
    char *buffer = stack_buffer;
    if (name_len + 1 > sizeof(stack_buffer)) {
        buffer = (char *)malloc((size_t)name_len + 1);
        if (buffer == NULL) {
            dolphin_set_error(DOLPHIN_ERR_OTHER, 0);
            return NULL;
        }
    }
    if (name_len > 0) {
        memcpy(buffer, name, (size_t)name_len);
    }
    buffer[name_len] = '\0';
    const char *value = getenv(buffer);
    if (buffer != stack_buffer) {
        free(buffer);
    }
    if (value == NULL) {
        dolphin_set_error(DOLPHIN_ERR_NOT_FOUND, 0);
        return NULL;
    }
    size_t length = strlen(value);
    if (!dolphin_is_valid_utf8((const uint8_t *)value, (uintptr_t)length)) {
        dolphin_set_error(DOLPHIN_ERR_INVALID, 0);
        return NULL;
    }
    *out_len = (uintptr_t)length;
    return (const uint8_t *)value;
}

int dolphin_last_error_kind(void) {
    return dolphin_error_kind;
}

int dolphin_last_error_code(void) {
    return dolphin_error_code;
}

void dolphin_print_string(const char *data, uintptr_t length) {
    dolphin_write_all(data, (size_t)length);
}

void dolphin_print_bool(uint8_t value) {
    if (value) {
        dolphin_write_all("true", 4);
    } else {
        dolphin_write_all("false", 5);
    }
}

void dolphin_print_i32(int32_t value) {
    char buffer[12];
    char *cursor = buffer + sizeof(buffer);
    int64_t number = value;
    uint64_t magnitude = number < 0 ? (uint64_t)(-number) : (uint64_t)number;

    do {
        *--cursor = (char)('0' + magnitude % 10);
        magnitude /= 10;
    } while (magnitude != 0);

    if (number < 0) {
        *--cursor = '-';
    }
    dolphin_write_all(cursor, (size_t)((buffer + sizeof(buffer)) - cursor));
}

void dolphin_print_i64(int64_t value) {
    char buffer[32];
    int length = snprintf(buffer, sizeof(buffer), "%lld", (long long)value);
    if (length > 0) dolphin_write_all(buffer, (size_t)length);
}

void dolphin_print_u64(uint64_t value) {
    char buffer[32];
    int length = snprintf(buffer, sizeof(buffer), "%llu", (unsigned long long)value);
    if (length > 0) dolphin_write_all(buffer, (size_t)length);
}

void dolphin_print_f32(float value) {
    char buffer[64];
    int length = snprintf(buffer, sizeof(buffer), "%.9g", (double)value);
    if (length > 0) dolphin_write_all(buffer, (size_t)length);
}

void dolphin_print_f64(double value) {
    char buffer[64];
    int length = snprintf(buffer, sizeof(buffer), "%.17g", value);
    if (length > 0) dolphin_write_all(buffer, (size_t)length);
}

void dolphin_print_char(uint32_t value) {
    char bytes[4];
    size_t length;
    if (value <= 0x7f) { bytes[0] = (char)value; length = 1; }
    else if (value <= 0x7ff) {
        bytes[0] = (char)(0xc0 | (value >> 6));
        bytes[1] = (char)(0x80 | (value & 0x3f)); length = 2;
    } else if (value <= 0xffff) {
        bytes[0] = (char)(0xe0 | (value >> 12));
        bytes[1] = (char)(0x80 | ((value >> 6) & 0x3f));
        bytes[2] = (char)(0x80 | (value & 0x3f)); length = 3;
    } else {
        bytes[0] = (char)(0xf0 | (value >> 18));
        bytes[1] = (char)(0x80 | ((value >> 12) & 0x3f));
        bytes[2] = (char)(0x80 | ((value >> 6) & 0x3f));
        bytes[3] = (char)(0x80 | (value & 0x3f)); length = 4;
    }
    dolphin_write_all(bytes, length);
}

uint8_t dolphin_string_equal(const char *a, uintptr_t a_length, const char *b, uintptr_t b_length) {
    return a_length == b_length && memcmp(a, b, (size_t)a_length) == 0;
}
