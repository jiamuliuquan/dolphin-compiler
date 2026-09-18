pkg std.collections;

// 容器与迭代器（M15-B/R12、R13）。业务逻辑全部为 Dolphin 源码；
// 编译器只把数组/切片/范围语法适配到同一 Iterator 协议。

use std.Option;
use std.Iterator;
use std.mem;

pub struct SliceIter<T> {
    storage: []const T,
    index: usize,
}

impl<T> SliceIter<T> {
    pub fn init(storage: []const T): SliceIter<T> {
        return SliceIter<T>(storage, 0_usize);
    }

    pub fn len(self: *const Self): usize {
        return self->storage.len - self->index;
    }

    pub fn is_empty(self: *const Self): bool {
        return self->index >= self->storage.len;
    }
}

impl<T> Iterator for SliceIter<T> {
    type Item = T;

    fn next(self: *Self): Option<T> {
        if self->index >= self->storage.len {
            return Option.None;
        }
        val current = self->index;
        val storage = self->storage;
        self->index = current + 1_usize;
        return Option.Some(storage[current]);
    }
}

pub struct Range {
    current: i32,
    end: i32,
    inclusive: bool,
    finished: bool,
}

impl Range {
    pub fn exclusive(start: i32, end: i32): Range {
        return Range(start, end, false, false);
    }

    pub fn inclusive(start: i32, end: i32): Range {
        return Range(start, end, true, false);
    }
}

impl Iterator for Range {
    type Item = i32;

    fn next(self: *Self): Option<i32> {
        if self->finished {
            return Option.None;
        }
        if self->current > self->end {
            return Option.None;
        }
        if self->current == self->end {
            // 仅当闭区间时产出最后一个端点，绝不越过 i32::MAX 再加一。
            self->finished = true;
            if self->inclusive {
                return Option.Some(self->end);
            }
            return Option.None;
        }
        val value = self->current;
        self->current = value + 1;
        return Option.Some(value);
    }
}

pub struct Vec<T> {
    storage: []T,
    used: usize,
}

impl<T> Vec<T> {
    pub fn init(): Vec<T> {
        return Vec<T>(mem.alloc<T>(0_usize), 0_usize);
    }

    pub fn with_capacity(capacity: usize): Vec<T> {
        return Vec<T>(mem.alloc<T>(capacity), 0_usize);
    }

    pub fn len(self: *const Self): usize {
        return self->used;
    }

    pub fn capacity(self: *const Self): usize {
        return self->storage.len;
    }

    pub fn is_empty(self: *const Self): bool {
        return self->used == 0_usize;
    }

    pub fn reserve(self: *Self, additional: usize) {
        // 加法在降低时会对溢出 trap（101）；这里先做无溢出的上界检查，
        // 再请求必然触发运行时尺寸检查的分配，从而按规格报 102。
        if additional > 18446744073709551615_usize - self->used {
            val overflow = mem.alloc<T>(additional);
            mem.free<T>(overflow);
            return;
        }
        val required = self->used + additional;
        if required <= self->storage.len {
            return;
        }
        self.grow(required);
    }

    fn grow(self: *Self, required: usize) {
        val current = self->storage.len;
        var next = 4_usize;
        if current >= 4_usize {
            // `current * 2` 也可能溢出；超限时走 102 通道而不是整数 trap。
            if current > 9223372036854775807_usize {
                val overflow = mem.alloc<T>(current);
                mem.free<T>(overflow);
                return;
            }
            next = current * 2_usize;
        }
        if next < required {
            next = required;
        }
        var replacement = mem.alloc<T>(next);
        val previous = self->storage;
        val count = self->used;
        if count > 0_usize {
            val target = replacement.slice(0_usize, count);
            val source = previous.slice(0_usize, count);
            mem.copy<T>(target, source);
        }
        mem.free<T>(previous);
        self->storage = replacement;
    }

    pub fn push(self: *Self, value: T) {
        if self->used >= self->storage.len {
            self.reserve(1_usize);
        }
        val storage = self->storage;
        storage[self->used] = value;
        self->used = self->used + 1_usize;
    }

    pub fn get(self: *const Self, index: usize): Option<T> {
        if index >= self->used {
            return Option.None;
        }
        val storage = self->storage;
        return Option.Some(storage[index]);
    }

    pub fn set(self: *Self, index: usize, value: T) {
        val storage = self.as_mut_slice();
        storage[index] = value;
    }

    pub fn pop(self: *Self): Option<T> {
        if self->used == 0_usize {
            return Option.None;
        }
        val next = self->used - 1_usize;
        self->used = next;
        val storage = self->storage;
        return Option.Some(storage[next]);
    }

    pub fn as_slice(self: *const Self): []const T {
        val storage = self->storage;
        return storage.slice(0_usize, self->used);
    }

    pub fn as_mut_slice(self: *Self): []T {
        val storage = self->storage;
        return storage.slice(0_usize, self->used);
    }

    pub fn iter(self: *const Self): SliceIter<T> {
        return SliceIter<T>::init(self.as_slice());
    }

    pub fn clone(self: *const Self): Vec<T> {
        var result = Vec<T>::with_capacity(self->used);
        val count = self->used;
        if count > 0_usize {
            val storage = self->storage;
            val source = storage.slice(0_usize, count);
            var target = result.storage;
            val destination = target.slice(0_usize, count);
            mem.copy<T>(destination, source);
        }
        result.used = count;
        return result;
    }

    pub fn clear(self: *Self) {
        self->used = 0_usize;
    }

    pub fn deinit(self: *Self) {
        mem.free<T>(self->storage);
        self->storage = mem.alloc<T>(0_usize);
        self->used = 0_usize;
    }
}
