pkg std.error;

// 稳定错误类别与错误值（M19/H19-02）。
//
// 类别判别值与运行时 ABI 的 kind 编号 0..6 一致；`code` 保留平台原生错误码
// （Unix `errno` / Windows `GetLastError`），纯 API 错误为 0。错误值是值类型，
// 不拥有内存，不需要释放；不提供字符串消息或格式化输出。

pub enum ErrorKind {
    Other,
    NotFound,
    PermissionDenied,
    IsADirectory,
    InvalidArgument,
    NotOwned,
    Closed,
}

pub struct Error {
    kind: ErrorKind,
    code: i32,
}

impl Error {
    pub fn new(kind: ErrorKind, code: i32): Error {
        return Error(kind, code);
    }

    pub fn kind(self: *const Self): ErrorKind {
        return self->kind;
    }

    pub fn code(self: *const Self): i32 {
        return self->code;
    }
}

extern "C" {
    fn dolphin_last_error_kind(): i32;
    fn dolphin_last_error_code(): i32;
}

pub fn from_last_error(): Error {
    val kind = kind_from_runtime(dolphin_last_error_kind());
    return Error(kind, dolphin_last_error_code());
}

/// 运行时 kind 编号 -> 稳定类别（与第 13 节 ABI 表一致）。
fn kind_from_runtime(value: i32): ErrorKind {
    if value == 1_i32 {
        return ErrorKind.NotFound;
    }
    if value == 2_i32 {
        return ErrorKind.PermissionDenied;
    }
    if value == 3_i32 {
        return ErrorKind.IsADirectory;
    }
    if value == 4_i32 {
        return ErrorKind.InvalidArgument;
    }
    if value == 5_i32 {
        return ErrorKind.NotOwned;
    }
    if value == 6_i32 {
        return ErrorKind.Closed;
    }
    return ErrorKind.Other;
}
