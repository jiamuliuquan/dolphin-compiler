pkg std.fs;

// 文件与打开模式（M19/H19-03）。
//
// - `Read` 只读打开（不存在 → NotFound；目录 → Unix IsADirectory / Windows
//   InvalidArgument）；
// - `Write` 创建或截断，只写（Unix 权限 0644，受 umask 影响）；
// - `Append` 创建或追加，只写，每次写入前定位到末尾。
// - 路径必须是合法 UTF-8；含内部 NUL 时在调用运行时前返回 InvalidArgument，
//   不会截断路径打开错误文件。Unix 非 UTF-8 路径 M19 无法表达（argv 层已返回
//   NotUtf8），Windows 编码转换失败返回 InvalidArgument。
// - 返回的 `Stream` 是自有句柄：用 `close`/`defer close_abort` 释放；重绑定前
//   必须先 `close` 或 `release`（显式转交），否则 Debug 会报告未关闭句柄。

use std.error.Error;
use std.error.ErrorKind;
use std.error.from_last_error;
use std.io.Stream;
use std.io.from_raw;

pub enum OpenMode {
    Read,
    Write,
    Append,
}

extern "C" {
    fn dolphin_stream_open(path: *const u8, path_len: usize, mode: i32, out_handle: *usize): i32;
}

pub fn open(path: string, mode: OpenMode): Result<Stream, Error> {
    val bytes = path.bytes();
    var index = 0_usize;
    while index < bytes.len {
        if bytes[index] == 0_u8 {
            return Result.Err(Error::new(ErrorKind.InvalidArgument, 0_i32));
        }
        index += 1_usize;
    }
    val mode_code = match mode {
        OpenMode.Read => 0_i32,
        OpenMode.Write => 1_i32,
        OpenMode.Append => 2_i32,
    };
    var handle = 0_usize;
    val code = dolphin_stream_open(bytes.ptr, bytes.len, mode_code, &handle);
    if code == 0_i32 {
        return Result.Ok(from_raw(handle));
    }
    return Result.Err(from_last_error());
}
