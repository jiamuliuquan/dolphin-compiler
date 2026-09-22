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
#define DOLPHIN_EXIT_TEST 106

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

/* 句柄状态（M19/H19-02、H19-03）：标准流借用，文件流自有并登记在
 * `dolphin_owned_streams`；登记表供 release/from_raw 校验与 Debug 报告。 */
struct dolphin_stream_state {
    int open;
    int owned;
    HANDLE native;
    struct dolphin_stream_state *next;
};

static struct dolphin_stream_state *dolphin_owned_streams = NULL;

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
    if (leaked > 0) {
        char buffer[160];
        int length = snprintf(buffer, sizeof(buffer),
                              "Dolphin: %llu allocation(s) leaked at exit\n",
                              (unsigned long long)leaked);
        if (length > 0) {
            dolphin_write_error(buffer, (size_t)length);
        }
        for (struct dolphin_alloc_record *record = dolphin_live_allocs; record != NULL;
             record = record->next) {
            length = snprintf(buffer, sizeof(buffer), "  address=%p bytes=%llu\n",
                              record->pointer, (unsigned long long)record->bytes);
            if (length > 0) {
                dolphin_write_error(buffer, (size_t)length);
            }
        }
    }
    size_t open_handles = 0;
    for (struct dolphin_stream_state *stream = dolphin_owned_streams; stream != NULL;
         stream = stream->next) {
        if (stream->open) {
            open_handles += 1;
        }
    }
    if (open_handles > 0) {
        char handle_buffer[96];
        int handle_length = snprintf(handle_buffer, sizeof(handle_buffer),
                                     "Dolphin: %llu open handle(s) not closed at exit\n",
                                     (unsigned long long)open_handles);
        if (handle_length > 0) {
            dolphin_write_error(handle_buffer, (size_t)handle_length);
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

/* ---------------------------------------------------------------------------
 * 字节流（M19/H19-02）。
 *
 * 标准流是借用状态（owned=0），`close` 返回 NotOwned 且不关闭 OS 句柄；
 * 句柄副本共享同一状态，`close` 幂等（H19-03 的文件流沿用同一状态机）。
 * Windows 的 flush 使用 FlushFileBuffers；读写失败保留 GetLastError。
 * ------------------------------------------------------------------------- */

static struct dolphin_stream_state dolphin_stdin_state = {1, 0, NULL, NULL};
static struct dolphin_stream_state dolphin_stdout_state = {1, 0, NULL, NULL};
static struct dolphin_stream_state dolphin_stderr_state = {1, 0, NULL, NULL};

extern "C" uintptr_t dolphin_stream_stdin(void) {
    if (dolphin_stdin_state.native == NULL) {
        dolphin_stdin_state.native = GetStdHandle(STD_INPUT_HANDLE);
    }
    return (uintptr_t)&dolphin_stdin_state;
}

extern "C" uintptr_t dolphin_stream_stdout(void) {
    if (dolphin_stdout_state.native == NULL) {
        dolphin_stdout_state.native = GetStdHandle(STD_OUTPUT_HANDLE);
    }
    return (uintptr_t)&dolphin_stdout_state;
}

extern "C" uintptr_t dolphin_stream_stderr(void) {
    if (dolphin_stderr_state.native == NULL) {
        dolphin_stderr_state.native = GetStdHandle(STD_ERROR_HANDLE);
    }
    return (uintptr_t)&dolphin_stderr_state;
}

static int dolphin_kind_from_win32(DWORD code) {
    switch (code) {
        case ERROR_FILE_NOT_FOUND:
        case ERROR_PATH_NOT_FOUND:
            return DOLPHIN_ERR_NOT_FOUND;
        case ERROR_ACCESS_DENIED:
        case ERROR_SHARING_VIOLATION:
            return DOLPHIN_ERR_PERMISSION;
        case ERROR_DIRECTORY:
            return DOLPHIN_ERR_IS_DIR;
        case ERROR_INVALID_NAME:
        case ERROR_INVALID_PARAMETER:
        case ERROR_INVALID_HANDLE:
            return DOLPHIN_ERR_INVALID;
        default:
            return DOLPHIN_ERR_OTHER;
    }
}

extern "C" int dolphin_stream_read(uintptr_t id, uint8_t *buffer, uintptr_t length,
                                   uintptr_t *out_read) {
    struct dolphin_stream_state *state = (struct dolphin_stream_state *)id;
    if (state == NULL || !state->open) {
        dolphin_set_error(DOLPHIN_ERR_CLOSED, 0);
        return (int)ERROR_INVALID_HANDLE;
    }
    DWORD to_read = length > 0xFFFFFFFFu ? 0xFFFFFFFFu : (DWORD)length;
    DWORD read_bytes = 0;
    if (!ReadFile(state->native, buffer, to_read, &read_bytes, NULL)) {
        DWORD code = GetLastError();
        // 匿名管道写端关闭后 ReadFile 返回 ERROR_BROKEN_PIPE；按 EOF 处理，
        // 与 Unix read(2) 返回 0 的契约一致（`Ok(0) = EOF`）。
        if (code == ERROR_BROKEN_PIPE) {
            *out_read = 0;
            return 0;
        }
        dolphin_set_error(dolphin_kind_from_win32(code), (int)code);
        return (int)code;
    }
    *out_read = (uintptr_t)read_bytes;
    return 0;
}

extern "C" int dolphin_stream_write(uintptr_t id, const uint8_t *bytes, uintptr_t length,
                                    uintptr_t *out_written) {
    struct dolphin_stream_state *state = (struct dolphin_stream_state *)id;
    if (state == NULL || !state->open) {
        dolphin_set_error(DOLPHIN_ERR_CLOSED, 0);
        return (int)ERROR_INVALID_HANDLE;
    }
    DWORD to_write = length > 0xFFFFFFFFu ? 0xFFFFFFFFu : (DWORD)length;
    DWORD written = 0;
    if (!WriteFile(state->native, bytes, to_write, &written, NULL)) {
        DWORD code = GetLastError();
        dolphin_set_error(dolphin_kind_from_win32(code), (int)code);
        return (int)code;
    }
    *out_written = (uintptr_t)written;
    return 0;
}

extern "C" int dolphin_stream_flush(uintptr_t id) {
    struct dolphin_stream_state *state = (struct dolphin_stream_state *)id;
    if (state == NULL || !state->open) {
        dolphin_set_error(DOLPHIN_ERR_CLOSED, 0);
        return (int)ERROR_INVALID_HANDLE;
    }
    if (!FlushFileBuffers(state->native)) {
        DWORD code = GetLastError();
        dolphin_set_error(dolphin_kind_from_win32(code), (int)code);
        return (int)code;
    }
    return 0;
}

extern "C" int dolphin_stream_close(uintptr_t id) {
    struct dolphin_stream_state *state = (struct dolphin_stream_state *)id;
    if (state == NULL || !state->open) {
        return 0;
    }
    if (!state->owned) {
        dolphin_set_error(DOLPHIN_ERR_NOT_OWNED, 0);
        return (int)ERROR_ACCESS_DENIED;
    }
    if (!CloseHandle(state->native)) {
        DWORD code = GetLastError();
        state->open = 0;
        dolphin_set_error(dolphin_kind_from_win32(code), (int)code);
        return (int)code;
    }
    state->open = 0;
    return 0;
}

extern "C" uint8_t dolphin_stream_is_open(uintptr_t id) {
    struct dolphin_stream_state *state = (struct dolphin_stream_state *)id;
    return (uint8_t)(state != NULL && state->open);
}

/* ---------------------------------------------------------------------------
 * 文件流（M19/H19-03）。
 *
 * mode：0=Read、1=Write（创建/截断）、2=Append（创建/追加）。路径按 UTF-8 解码，
 * 非法编码或内部 NUL 返回 InvalidArgument；自有句柄登记供 release/from_raw
 * 校验与 Debug 报告。
 * ------------------------------------------------------------------------- */

static int dolphin_stream_open_wide(const wchar_t *path, int mode, uintptr_t *out_handle) {
    DWORD access;
    DWORD disposition;
    switch (mode) {
        case 0:
            access = GENERIC_READ;
            disposition = OPEN_EXISTING;
            break;
        case 1:
            access = GENERIC_WRITE;
            disposition = CREATE_ALWAYS;
            break;
        case 2:
            access = FILE_APPEND_DATA;
            disposition = OPEN_ALWAYS;
            break;
        default:
            dolphin_set_error(DOLPHIN_ERR_INVALID, 0);
            return ERROR_INVALID_PARAMETER;
    }
    // Windows 的目录按规格报 InvalidArgument（不是 PermissionDenied）。
    DWORD attributes = GetFileAttributesW(path);
    if (attributes != INVALID_FILE_ATTRIBUTES && (attributes & FILE_ATTRIBUTE_DIRECTORY)) {
        dolphin_set_error(DOLPHIN_ERR_INVALID, (int)ERROR_DIRECTORY);
        return (int)ERROR_DIRECTORY;
    }
    HANDLE native = CreateFileW(path, access, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL,
                                disposition, FILE_ATTRIBUTE_NORMAL, NULL);    if (native == INVALID_HANDLE_VALUE) {
        DWORD code = GetLastError();
        dolphin_set_error(dolphin_kind_from_win32(code), (int)code);
        return (int)code;
    }
    struct dolphin_stream_state *state =
        (struct dolphin_stream_state *)malloc(sizeof(*state));
    if (state == NULL) {
        CloseHandle(native);
        dolphin_set_error(DOLPHIN_ERR_OTHER, 0);
        return ERROR_NOT_ENOUGH_MEMORY;
    }
    state->open = 1;
    state->owned = 1;
    state->native = native;
    state->next = dolphin_owned_streams;
    dolphin_owned_streams = state;
    *out_handle = (uintptr_t)state;
    dolphin_set_error(DOLPHIN_ERR_OTHER, 0);
    return 0;
}

extern "C" int dolphin_stream_open(const uint8_t *path, uintptr_t length, int mode,
                                   uintptr_t *out_handle) {
    for (uintptr_t index = 0; index < length; index += 1) {
        if (path[index] == 0) {
            dolphin_set_error(DOLPHIN_ERR_INVALID, 0);
            return ERROR_INVALID_PARAMETER;
        }
    }
    if (length > 0x7FFFFFFFu) {
        dolphin_set_error(DOLPHIN_ERR_INVALID, 0);
        return ERROR_INVALID_PARAMETER;
    }
    int needed = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, (const char *)path,
                                     (int)length, NULL, 0);
    if (needed <= 0) {
        dolphin_set_error(DOLPHIN_ERR_INVALID, 0);
        return ERROR_INVALID_PARAMETER;
    }
    wchar_t *wide = (wchar_t *)malloc(((size_t)needed + 1) * sizeof(wchar_t));
    if (wide == NULL) {
        dolphin_set_error(DOLPHIN_ERR_OTHER, 0);
        return ERROR_NOT_ENOUGH_MEMORY;
    }
    int written = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, (const char *)path,
                                      (int)length, wide, needed);
    if (written != needed) {
        free(wide);
        dolphin_set_error(DOLPHIN_ERR_INVALID, 0);
        return ERROR_INVALID_PARAMETER;
    }
    wide[needed] = L'\0';
    int result = dolphin_stream_open_wide(wide, mode, out_handle);
    free(wide);
    return result;
}

/// 显式转交：返回可被 `from_raw` 接管的 id；借用/已关闭/未知 id 返回 0。
extern "C" uintptr_t dolphin_stream_release(uintptr_t id) {
    for (struct dolphin_stream_state *state = dolphin_owned_streams; state != NULL;
         state = state->next) {
        if ((uintptr_t)state == id) {
            return (state->open && state->owned) ? id : 0;
        }
    }
    return 0;
}

/// 接管 id：仅接受登记表中的 open 自有流；非法/已关闭/借用 id 返回 0。
extern "C" uintptr_t dolphin_stream_from_raw(uintptr_t id) {
    if (id == 0) {
        return 0;
    }
    for (struct dolphin_stream_state *state = dolphin_owned_streams; state != NULL;
         state = state->next) {
        if ((uintptr_t)state == id) {
            return state->open ? id : 0;
        }
    }
    return 0;
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

// 测试断言失败（M19/H19-05b）：写固定文本并以 106 退出；与 trap 一样不展开
// Dolphin 栈、不执行 defer、不运行 Debug 收尾报告。
extern "C" void dolphin_test_fail(void) {
    const char message[] = "Dolphin test assertion failed\n";
    DWORD written;
    (void)WriteFile(GetStdHandle(STD_ERROR_HANDLE), message, sizeof(message) - 1, &written, NULL);
    ExitProcess(DOLPHIN_EXIT_TEST);
}
