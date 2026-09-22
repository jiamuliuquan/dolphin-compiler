pkg std.io;

// 字节流与标准流（M19/H19-02）。
//
// - `Stream` 是句柄值；副本共享运行时状态，不存在“第二份关闭权”。
// - 标准流（stdin/stdout/stderr）是**借用句柄**：`close` 返回 Err(NotOwned)
//   且不关闭 OS 句柄，`close_abort` 是无操作。
// - `read` 返回实际字节数，`Ok(0)` 表示 EOF；短读是正常结果，调用方必须循环。
// - `write` 返回实际写入数；`write_all` 循环写完全部字节，写入 0 或出错返回 Err
//   （此时可能已写入前缀）。
// - 成功类返回值使用 bool（`true`），因为当前语言不能表达 `Result<(), E>`
//   （Unit payload 会触发诊断，见 H19-02）；错误走 `std.error.Error`。
// - `release` / `from_raw` 与 `std.fs.open` 一起在 H19-03 引入。

use std.error.Error;
use std.error.ErrorKind;
use std.error.from_last_error;

pub struct Stream {
    handle: usize,
}

extern "C" {
    fn dolphin_stream_stdin(): usize;
    fn dolphin_stream_stdout(): usize;
    fn dolphin_stream_stderr(): usize;
    fn dolphin_stream_read(stream: usize, buffer: *u8, length: usize, out_read: *usize): i32;
    fn dolphin_stream_write(stream: usize, bytes: *const u8, length: usize, out_written: *usize): i32;
    fn dolphin_stream_flush(stream: usize): i32;
    fn dolphin_stream_close(stream: usize): i32;
    fn dolphin_stream_is_open(stream: usize): u8;
}

pub fn stdin(): Stream {
    return Stream(dolphin_stream_stdin());
}

pub fn stdout(): Stream {
    return Stream(dolphin_stream_stdout());
}

pub fn stderr(): Stream {
    return Stream(dolphin_stream_stderr());
}

impl Stream {
    pub fn read(self: *const Self, buffer: []u8): Result<usize, Error> {
        var count = 0_usize;
        val code = dolphin_stream_read(self->handle, buffer.ptr, buffer.len, &count);
        if code == 0_i32 {
            return Result.Ok(count);
        }
        return Result.Err(from_last_error());
    }

    pub fn write(self: *const Self, bytes: []const u8): Result<usize, Error> {
        var count = 0_usize;
        val code = dolphin_stream_write(self->handle, bytes.ptr, bytes.len, &count);
        if code == 0_i32 {
            return Result.Ok(count);
        }
        return Result.Err(from_last_error());
    }

    pub fn write_all(self: *const Self, bytes: []const u8): Result<bool, Error> {
        var offset = 0_usize;
        while offset < bytes.len {
            val remaining = bytes.slice(offset, bytes.len);
            val result = self.write(remaining);
            if result.is_err() {
                val error = match result {
                    Result.Ok(value) => Error::new(ErrorKind.Other, 0_i32),
                    Result.Err(value) => value,
                };
                return Result.Err(error);
            }
            val written = match result {
                Result.Ok(value) => value,
                Result.Err(error) => 0_usize,
            };
            if written == 0_usize {
                return Result.Err(Error::new(ErrorKind.Other, 0_i32));
            }
            offset += written;
        }
        return Result.Ok(true);
    }

    pub fn flush(self: *const Self): Result<bool, Error> {
        val code = dolphin_stream_flush(self->handle);
        if code == 0_i32 {
            return Result.Ok(true);
        }
        return Result.Err(from_last_error());
    }

    pub fn close(self: *Self): Result<bool, Error> {
        val code = dolphin_stream_close(self->handle);
        if code == 0_i32 {
            return Result.Ok(true);
        }
        return Result.Err(from_last_error());
    }

    /// `defer` 清理入口：失败写 stderr 固定前缀并保留原退出码；借用/已关闭句柄无操作。
    pub fn close_abort(self: *Self) {
        val result = self.close();
        if result.is_err() {
            val error = match result {
                Result.Ok(value) => Error::new(ErrorKind.Other, 0_i32),
                Result.Err(value) => value,
            };
            val report = match error.kind() {
                ErrorKind.NotOwned => false,
                ErrorKind.Closed => false,
                _ => true,
            };
            if report {
                val message = "Dolphin cleanup error: close failed\n";
                eprint(message.bytes());
            }
        }
    }

    pub fn is_open(self: *const Self): bool {
        return dolphin_stream_is_open(self->handle) != 0_u8;
    }
}

pub fn eprint(bytes: []const u8): Result<bool, Error> {
    val stream = stderr();
    return stream.write_all(bytes);
}
