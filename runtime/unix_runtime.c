#include <stddef.h>
#include <stdint.h>
#include <unistd.h>
#include <stdio.h>
#include <string.h>
#include <signal.h>
#include <stdlib.h>

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

static void dolphin_runtime_trap(int signal_number) {
    (void)signal_number;
    const char message[] = "Dolphin runtime error: overflow, division by zero, or array bounds violation\n";
    write(2, message, sizeof(message) - 1);
    _Exit(101);
}

/* -------------------------------------------------------------------------
 * M14 内存模型：显式分配 / 释放，配合 header 标记 + 存活登记表 + 退出钩子。
 *
 * 首版实现取舍：运行时目标文件在编译编译器时一次性内嵌（build.rs），无法
 * 按 Debug/Release 分别链接，因此检测分配器始终启用。Release 下的「关闭检测」
 * 作为未来演进项，不在此里程碑实现（见提案 §12.6/§12.9）。
 * ---------------------------------------------------------------------- */

typedef struct {
    uint64_t magic;   /* 存活魔数：区分「存活 / 已释放」以捕获双重释放 */
    uint64_t size;    /* 用户数据区字节数 */
} dolphin_block_header;

#define DOLPHIN_MAGIC_LIVE 0xD01F1F0CA11C0FFEULL
#define DOLPHIN_MAGIC_DEAD 0xDEADBEEFDEADBEEFULL

/* 存活块登记表（单向链表）。 */
typedef struct dolphin_live_node {
    void *data;
    uint64_t size;
    struct dolphin_live_node *next;
} dolphin_live_node;

static dolphin_live_node *dolphin_live_blocks = NULL;

/* 显式退出：分配失败(102)与双重释放(103)经普通函数路径退出，直接产生进程退出码。 */
static void dolphin_runtime_exit(int code) {
    exit(code);
}

void *dolphin_allocate(uint64_t n) {
    dolphin_block_header *header =
        (dolphin_block_header *)malloc(sizeof(dolphin_block_header) + (size_t)n);
    if (header == NULL) {
        dolphin_runtime_exit(102);
    }
    header->magic = DOLPHIN_MAGIC_LIVE;
    header->size = n;
    dolphin_live_node *node = (dolphin_live_node *)malloc(sizeof(dolphin_live_node));
    if (node == NULL) {
        dolphin_runtime_exit(102);
    }
    node->data = (char *)header + sizeof(dolphin_block_header);
    node->size = n;
    node->next = dolphin_live_blocks;
    dolphin_live_blocks = node;
    return node->data;
}

void dolphin_free(void *ptr, uint64_t len) {
    (void)len;
    if (ptr == NULL) {
        return;
    }
    dolphin_block_header *header =
        (dolphin_block_header *)((char *)ptr - sizeof(dolphin_block_header));
    if (header->magic != DOLPHIN_MAGIC_LIVE) {
        const char message[] = "Dolphin runtime error: double free detected\n";
        write(2, message, sizeof(message) - 1);
        dolphin_runtime_exit(103);
    }
    header->magic = DOLPHIN_MAGIC_DEAD;
    /* 从登记表移除。 */
    dolphin_live_node **link = &dolphin_live_blocks;
    while (*link != NULL) {
        if ((*link)->data == ptr) {
            dolphin_live_node *removed = *link;
            *link = removed->next;
            free(removed);
            break;
        }
        link = &(*link)->next;
    }
    free(header);
}

/* 字符串拼接：隐式分配目标缓冲，返回新缓冲区指针（长度由调用方用 a_len + b_len 计算）。 */
void *dolphin_string_concat(
    const char *a, uintptr_t a_len,
    const char *b, uintptr_t b_len) {
    uint64_t total = (uint64_t)a_len + (uint64_t)b_len;
    char *buffer = (char *)dolphin_allocate(total);
    if (a_len > 0) {
        memcpy(buffer, a, (size_t)a_len);
    }
    if (b_len > 0) {
        memcpy(buffer + a_len, b, (size_t)b_len);
    }
    return buffer;
}

/* 泄漏检测：程序退出时报告未释放块（打印地址与大小到 stderr）。 */
static void dolphin_report_leaks(void) {
    dolphin_live_node *node = dolphin_live_blocks;
    if (node == NULL) {
        return;
    }
    write(2, "Dolphin runtime error: leaked blocks:\n", 38);
    char buffer[128];
    while (node != NULL) {
        int length = snprintf(buffer, sizeof(buffer), "  address=%p size=%llu\n",
                              node->data, (unsigned long long)node->size);
        if (length > 0) {
            write(2, buffer, (size_t)length);
        }
        node = node->next;
    }
}

__attribute__((constructor)) static void dolphin_install_traps(void) {
    signal(SIGILL, dolphin_runtime_trap);
    signal(SIGFPE, dolphin_runtime_trap);
    signal(SIGTRAP, dolphin_runtime_trap);
}

__attribute__((destructor)) static void dolphin_report_leaks_at_exit(void) {
    dolphin_report_leaks();
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
