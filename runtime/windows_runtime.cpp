#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <windows.h>
#include <shellapi.h>

// M19/H19-01：`CommandLineToArgvW` 取自 shell32；用默认库指令让 MSVC 对象
// 带上依赖，rust-lld（COFF）与 link.exe 都会按 /DEFAULTLIB 解析。
#pragma comment(lib, "shell32.lib")

#define DOLPHIN_EXIT_TRAP 101
#define DOLPHIN_EXIT_ALLOC 102
#define DOLPHIN_EXIT_INVALID_FREE 103
#define DOLPHIN_EXIT_UTF8 104

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

static void dolphin_write_error(const char *data, size_t length) {
    const HANDLE err = GetStdHandle(STD_ERROR_HANDLE);
    while (length > 0) {
        DWORD written;
        if (!WriteFile(err, data, (DWORD)length, &written, NULL) || written == 0) {
            return;
        }
        data += written;
        length -= (size_t)written;
    }
}

/// Terminate the process with the given exit code. The runtime does not unwind
/// the Dolphin stack, so `defer` is not executed.
static void dolphin_exit_with(int code, const char *message) {
    dolphin_write_error(message, strlen(message));
    ExitProcess((UINT)code);
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
    ExitProcess(DOLPHIN_EXIT_TRAP);
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

/* ---------------------------------------------------------------------------
 * Allocator and Debug live-allocation table (M14-C/R04).
 * See unix_runtime.c for the rationale.
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

extern "C" void dolphin_runtime_finish(void) {
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

extern "C" void dolphin_runtime_finish(void) {}
#endif

static uintptr_t dolphin_allocations = 0;

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

extern "C" void *dolphin_alloc(uintptr_t count, uintptr_t elem_size, uintptr_t align) {
    if (count == 0 || elem_size == 0) {
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

extern "C" void dolphin_free(void *pointer, uintptr_t bytes) {
    if (pointer == NULL) {
        return;
    }
    if (!dolphin_unregister_alloc(pointer, (size_t)bytes)) {
        dolphin_exit_with(DOLPHIN_EXIT_INVALID_FREE, "Dolphin runtime error: invalid free\n");
    }
    free(pointer);
}

extern "C" void dolphin_copy(void *destination, const void *source, uintptr_t bytes) {
    if (bytes != 0) {
        memmove(destination, source, (size_t)bytes);
    }
}

extern "C" void dolphin_check_align(void *pointer, uintptr_t align) {
    if (align != 0 && ((uintptr_t)pointer % align) != 0) {
        dolphin_exit_with(DOLPHIN_EXIT_TRAP, "Dolphin runtime error: misaligned pointer\n");
    }
}

extern "C" void dolphin_check_view(const void *pointer, uintptr_t length) {
    if (pointer == NULL && length != 0) {
        dolphin_exit_with(DOLPHIN_EXIT_TRAP,
                          "Dolphin runtime error: null pointer with non-zero view length\n");
    }
}

extern "C" uint8_t dolphin_is_valid_utf8(const uint8_t *bytes, uintptr_t length) {
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

extern "C" void dolphin_check_utf8(const uint8_t *bytes, uintptr_t length) {
    if (!dolphin_is_valid_utf8(bytes, length)) {
        dolphin_exit_with(DOLPHIN_EXIT_UTF8, "Dolphin runtime error: invalid UTF-8\n");
    }
}

/* ---------------------------------------------------------------------------
 * 进程参数与环境（M19/H19-01）。
 *
 * Windows 的 CRT `main` 只提供 ANSI argv，会丢失非 ANSI 字符；这里改用
 * `GetCommandLineW` + `CommandLineToArgvW` 取宽字符参数并惰性转成 UTF-8 缓存，
 * 视图有效到进程结束。环境表由 `GetEnvironmentStringsW` 同样转换。
 * 错误类别编号与规格 `std.error.ErrorKind` 的稳定判别值一致（0..6）。
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

/// 把宽字符串转为 UTF-8（含结尾 NUL）；非法 UTF-16 返回 NULL。
static char *dolphin_utf16_to_utf8(const wchar_t *text) {
    int needed = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, text, -1, NULL, 0, NULL, NULL);
    if (needed <= 0) {
        return NULL;
    }
    char *converted = (char *)malloc((size_t)needed);
    if (converted == NULL) {
        return NULL;
    }
    int written = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, text, -1, converted, needed,
                                      NULL, NULL);
    if (written != needed) {
        free(converted);
        return NULL;
    }
    return converted;
}

static char **dolphin_utf8_args = NULL;
static int *dolphin_utf8_arg_valid = NULL;
static int dolphin_utf8_argc = 0;
static int dolphin_args_ready = 0;

static void dolphin_build_args(void) {
    if (dolphin_args_ready) {
        return;
    }
    dolphin_args_ready = 1;
    int argc = 0;
    wchar_t **wide = CommandLineToArgvW(GetCommandLineW(), &argc);
    if (wide == NULL || argc <= 0) {
        return;
    }
    dolphin_utf8_args = (char **)calloc((size_t)argc, sizeof(char *));
    dolphin_utf8_arg_valid = (int *)calloc((size_t)argc, sizeof(int));
    if (dolphin_utf8_args == NULL || dolphin_utf8_arg_valid == NULL) {
        LocalFree(wide);
        return;
    }
    dolphin_utf8_argc = argc;
    for (int index = 0; index < argc; index += 1) {
        char *converted = dolphin_utf16_to_utf8(wide[index]);
        dolphin_utf8_args[index] = converted;
        dolphin_utf8_arg_valid[index] = converted != NULL;
    }
    LocalFree(wide);
}

/// 保存入口参数。Windows 参数从宽字符 API 读取，因此忽略 ANSI argv。
extern "C" void dolphin_init_args(int argc, char **argv) {
    (void)argc;
    (void)argv;
    dolphin_set_error(DOLPHIN_ERR_OTHER, 0);
}

extern "C" uintptr_t dolphin_arg_count(void) {
    dolphin_build_args();
    return (uintptr_t)dolphin_utf8_argc;
}

extern "C" const uint8_t *dolphin_arg(uintptr_t index, uintptr_t *out_len) {
    dolphin_build_args();
    if (index >= (uintptr_t)dolphin_utf8_argc) {
        dolphin_set_error(DOLPHIN_ERR_NOT_FOUND, 0);
        return NULL;
    }
    if (!dolphin_utf8_arg_valid[index] || dolphin_utf8_args[index] == NULL) {
        dolphin_set_error(DOLPHIN_ERR_INVALID, 0);
        return NULL;
    }
    *out_len = (uintptr_t)strlen(dolphin_utf8_args[index]);
    return (const uint8_t *)dolphin_utf8_args[index];
}

static char **dolphin_utf8_env = NULL;
static int dolphin_env_ready = 0;

static void dolphin_build_env(void) {
    if (dolphin_env_ready) {
        return;
    }
    dolphin_env_ready = 1;
    wchar_t *block = GetEnvironmentStringsW();
    if (block == NULL) {
        return;
    }
    int count = 0;
    for (wchar_t *entry = block; *entry != L'\0'; entry += wcslen(entry) + 1) {
        count += 1;
    }
    dolphin_utf8_env = (char **)calloc((size_t)count + 1, sizeof(char *));
    if (dolphin_utf8_env == NULL) {
        FreeEnvironmentStringsW(block);
        return;
    }
    int index = 0;
    for (wchar_t *entry = block; *entry != L'\0'; entry += wcslen(entry) + 1) {
        // `=C:` 之类的特殊条目没有名字，跳过。
        if (*entry == L'=') {
            continue;
        }
        char *converted = dolphin_utf16_to_utf8(entry);
        if (converted != NULL) {
            dolphin_utf8_env[index] = converted;
            index += 1;
        }
    }
    FreeEnvironmentStringsW(block);
}

extern "C" const uint8_t *dolphin_env(const uint8_t *name, uintptr_t name_len, uintptr_t *out_len) {
    dolphin_build_env();
    if (dolphin_utf8_env == NULL) {
        dolphin_set_error(DOLPHIN_ERR_OTHER, 0);
        return NULL;
    }
    for (int index = 0; dolphin_utf8_env[index] != NULL; index += 1) {
        const char *entry = dolphin_utf8_env[index];
        const char *equals = strchr(entry, '=');
        if (equals == NULL) {
            continue;
        }
        size_t entry_name_len = (size_t)(equals - entry);
        if (entry_name_len != (size_t)name_len) {
            continue;
        }
        if (_strnicmp(entry, (const char *)name, entry_name_len) != 0) {
            continue;
        }
        const char *value = equals + 1;
        *out_len = (uintptr_t)strlen(value);
        return (const uint8_t *)value;
    }
    dolphin_set_error(DOLPHIN_ERR_NOT_FOUND, 0);
    return NULL;
}

extern "C" int dolphin_last_error_kind(void) {
    return dolphin_error_kind;
}

extern "C" int dolphin_last_error_code(void) {
    return dolphin_error_code;
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
