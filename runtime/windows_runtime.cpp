#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <windows.h>

static void dolphin_write_all(const char *data, size_t length) {
    const HANDLE out = GetStdHandle(STD_OUTPUT_HANDLE);
    while (length > 0) {
        DWORD written;
        if (!WriteFile(out, data, (DWORD)length, &written, NULL) || written == 0) {
            return;
        }
        data += written;
        length -= (size_t)written;
    }
}

static LONG WINAPI dolphin_runtime_trap(PEXCEPTION_POINTERS info) {
    const PEXCEPTION_RECORD record = info->ExceptionRecord;
    if (record->ExceptionCode != EXCEPTION_INT_DIVIDE_BY_ZERO &&
        record->ExceptionCode != EXCEPTION_ILLEGAL_INSTRUCTION) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    const char message[] = "Dolphin runtime error: overflow, division by zero, or array bounds violation\n";
    DWORD written;
    (void)WriteFile(GetStdHandle(STD_ERROR_HANDLE), message, sizeof(message) - 1, &written, NULL);
    ExitProcess(101);
}

static void dolphin_install_traps(void) {
    (void)AddVectoredExceptionHandler(1, dolphin_runtime_trap);
    (void)SetConsoleOutputCP(65001);
}

namespace {
struct TrapInstaller {
    TrapInstaller() { dolphin_install_traps(); }
};
TrapInstaller trap_installer;
}

extern "C" void dolphin_print_string(const char *data, uintptr_t length) {
    dolphin_write_all(data, (size_t)length);
}

extern "C" void dolphin_print_bool(uint8_t value) {
    if (value) {
        dolphin_write_all("true", 4);
    } else {
        dolphin_write_all("false", 5);
    }
}

extern "C" void dolphin_print_i32(int32_t value) {
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

extern "C" void dolphin_print_i64(int64_t value) {
    char buffer[32];
    int length = snprintf(buffer, sizeof(buffer), "%lld", (long long)value);
    if (length > 0) dolphin_write_all(buffer, (size_t)length);
}

extern "C" void dolphin_print_u64(uint64_t value) {
    char buffer[32];
    int length = snprintf(buffer, sizeof(buffer), "%llu", (unsigned long long)value);
    if (length > 0) dolphin_write_all(buffer, (size_t)length);
}

extern "C" void dolphin_print_f32(float value) {
    char buffer[64];
    int length = snprintf(buffer, sizeof(buffer), "%.9g", (double)value);
    if (length > 0) dolphin_write_all(buffer, (size_t)length);
}

extern "C" void dolphin_print_f64(double value) {
    char buffer[64];
    int length = snprintf(buffer, sizeof(buffer), "%.17g", value);
    if (length > 0) dolphin_write_all(buffer, (size_t)length);
}

extern "C" void dolphin_print_char(uint32_t value) {
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

extern "C" uint8_t dolphin_string_equal(const char *a, uintptr_t a_length, const char *b, uintptr_t b_length) {
    return a_length == b_length && memcmp(a, b, (size_t)a_length) == 0;
}

/* -------------------------------------------------------------------------
 * M14 内存模型：显式分配 / 释放，配合 header 标记 + 存活登记表 + 退出钩子。
 * 首版实现取舍：运行时目标文件在编译编译器时一次性内嵌，无法按 Debug/Release
 * 分别链接，因此检测分配器始终启用（见提案 §12.6/§12.9）。
 * ---------------------------------------------------------------------- */

namespace {

struct BlockHeader {
    uint64_t magic;
    uint64_t size;
};

struct LiveNode {
    void *data;
    uint64_t size;
    LiveNode *next;
};

const uint64_t kMagicLive = 0xD01F1F0CA11C0FFEULL;
const uint64_t kMagicDead = 0xDEADBEEFDEADBEEFULL;

LiveNode *g_live_blocks = nullptr;

void runtime_exit(int code) {
    ExitProcess((UINT)code);
}

void report_leaks() {
    LiveNode *node = g_live_blocks;
    if (node == nullptr) {
        return;
    }
    const char header[] = "Dolphin runtime error: leaked blocks:\n";
    DWORD written;
    (void)WriteFile(GetStdHandle(STD_ERROR_HANDLE), header, sizeof(header) - 1, &written, NULL);
    char buffer[128];
    while (node != nullptr) {
        int length = snprintf(buffer, sizeof(buffer), "  address=%p size=%llu\n",
                              node->data, (unsigned long long)node->size);
        if (length > 0) {
            (void)WriteFile(GetStdHandle(STD_ERROR_HANDLE), buffer, (DWORD)length, &written, NULL);
        }
        node = node->next;
    }
}

struct LeakReporter {
    ~LeakReporter() { report_leaks(); }
};

LeakReporter g_leak_reporter;

} // namespace

extern "C" void *dolphin_allocate(uint64_t n) {
    BlockHeader *header = (BlockHeader *)malloc(sizeof(BlockHeader) + (size_t)n);
    if (header == nullptr) {
        runtime_exit(102);
    }
    header->magic = kMagicLive;
    header->size = n;
    LiveNode *node = (LiveNode *)malloc(sizeof(LiveNode));
    if (node == nullptr) {
        runtime_exit(102);
    }
    node->data = (char *)header + sizeof(BlockHeader);
    node->size = n;
    node->next = g_live_blocks;
    g_live_blocks = node;
    return node->data;
}

extern "C" void dolphin_free(void *ptr, uint64_t len) {
    (void)len;
    if (ptr == nullptr) {
        return;
    }
    BlockHeader *header = (BlockHeader *)((char *)ptr - sizeof(BlockHeader));
    if (header->magic != kMagicLive) {
        const char message[] = "Dolphin runtime error: double free detected\n";
        DWORD written;
        (void)WriteFile(GetStdHandle(STD_ERROR_HANDLE), message, sizeof(message) - 1, &written, NULL);
        runtime_exit(103);
    }
    header->magic = kMagicDead;
    LiveNode **link = &g_live_blocks;
    while (*link != nullptr) {
        if ((*link)->data == ptr) {
            LiveNode *removed = *link;
            *link = removed->next;
            free(removed);
            break;
        }
        link = &(*link)->next;
    }
    free(header);
}

extern "C" void *dolphin_string_concat(
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
