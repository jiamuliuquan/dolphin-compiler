pkg std.ffi;

// C 字符串（M15-B/R14）：拥有型 NUL 结尾缓冲，ptr() 交给 C，deinit 释放。

use std.mem;

pub enum CStringError {
    InteriorNul,
}

pub struct CString {
    bytes: []u8,
}

impl CString {
    pub fn empty(): CString {
        var buffer = mem.alloc<u8>(1_usize);
        val slot = buffer.slice(0_usize, 1_usize);
        slot[0] = 0_u8;
        return CString(buffer);
    }

    pub fn from(s: string): Result<CString, CStringError> {
        val source = s.bytes();
        val count = source.len;
        if has_interior_nul(source) {
            return Result.Err(CStringError.InteriorNul);
        }
        var buffer = mem.alloc<u8>(count + 1_usize);
        if count > 0_usize {
            val target = buffer.slice(0_usize, count);
            mem.copy<u8>(target, source);
        }
        buffer[count] = 0_u8;
        return Result.Ok(CString(buffer));
    }

    pub fn ptr(self: *const Self): *const c_char {
        return mem.cast_const_ptr<c_char>(self->bytes.ptr);
    }

    pub fn deinit(self: *Self) {
        mem.free<u8>(self->bytes);
        self->bytes = mem.alloc<u8>(0_usize);
    }
}

fn has_interior_nul(bytes: []const u8): bool {
    var index = 0_usize;
    while index < bytes.len {
        if bytes[index] == 0_u8 {
            return true;
        }
        index = index + 1_usize;
    }
    return false;
}
