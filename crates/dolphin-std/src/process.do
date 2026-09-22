pkg std.process;

// 进程参数与环境（M19/H19-01）。
//
// 所有返回值都是**借用视图**，有效到进程结束：调用者不得 `deinit`、不得写入。
// Windows 上参数从宽字符 API 转换，含未配对代理项的条目返回 `ArgError.NotUtf8`；
// Unix 上原始 argv 字节不是合法 UTF-8 时返回 `ArgError.NotUtf8`，绝不替换字符。
// 环境项用三态区分“缺项”和“存在但非 UTF-8”。
//
// 运行时错误类别编号与规格 `std.error.ErrorKind` 的稳定判别值一致；
// `NotFound = 1` 用于区分环境缺项。

use std.mem;

pub enum ArgError {
    OutOfRange,
    NotUtf8,
}

pub enum EnvLookup {
    Found(string),
    Missing,
    NotUtf8,
}

extern "C" {
    fn dolphin_arg_count(): usize;
    fn dolphin_arg(index: usize, out_len: *usize): *Unit;
    fn dolphin_env(name: *const u8, name_len: usize, out_len: *usize): *Unit;
    fn dolphin_last_error_kind(): i32;
}

pub fn arg_count(): usize {
    return dolphin_arg_count();
}

pub fn arg(index: usize): Result<string, ArgError> {
    if index >= arg_count() {
        return Result.Err(ArgError.OutOfRange);
    }
    var out_len = 0_usize;
    val pointer = dolphin_arg(index, &out_len);
    if pointer == null {
        return Result.Err(ArgError.NotUtf8);
    }
    // 运行时已校验 UTF-8，这里只做视图构造。
    val bytes = mem.view_const<u8>(mem.cast_const_ptr<u8>(pointer), out_len);
    return Result.Ok(string.from_bytes(bytes));
}

pub fn program_name(): Result<string, ArgError> {
    return arg(0_usize);
}

pub fn env(name: string): EnvLookup {
    val name_bytes = name.bytes();
    var out_len = 0_usize;
    val pointer = dolphin_env(name_bytes.ptr, name_bytes.len, &out_len);
    if pointer == null {
        if dolphin_last_error_kind() == 1 {
            return EnvLookup.Missing;
        }
        return EnvLookup.NotUtf8;
    }
    val value = mem.view_const<u8>(mem.cast_const_ptr<u8>(pointer), out_len);
    return EnvLookup.Found(string.from_bytes(value));
}
