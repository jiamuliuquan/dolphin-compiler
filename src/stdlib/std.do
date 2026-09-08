// Dolphin 标准库（内置源码，随编译器分发，见提案 §6.1）。
// 本文件作为 `std` 模块自动注入到每个编译单元。

pkg std;

// 可选值（提案 §11.3：`?T` 是 `Option<T>` 的语法糖）。
pub enum Option<T> {
    Some(T),
    None,
}

// 错误值（提案 §8：`Result<T, E>` 表达成功或失败）。
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}

// ── string 工具（基于 `s.bytes()` 零成本视图，M15）────────────────────────

// 字符串是否为空。
pub fn is_empty(s: string): bool {
    return length(s) == 0;
}

// 字符串是否以 prefix 开头。
pub fn starts_with(s: string, prefix: string): bool {
    var s_bytes = s.bytes();
    var p_bytes = prefix.bytes();
    val p_len = length(p_bytes);
    if length(s_bytes) < p_len {
        return false;
    }
    var i = 0;
    while i < p_len {
        if s_bytes[i] != p_bytes[i] {
            return false;
        }
        i += 1;
    }
    return true;
}

// 字符串是否以 suffix 结尾。
pub fn ends_with(s: string, suffix: string): bool {
    var s_bytes = s.bytes();
    var e_bytes = suffix.bytes();
    val s_len = length(s_bytes);
    val e_len = length(e_bytes);
    if s_len < e_len {
        return false;
    }
    var i = 0;
    while i < e_len {
        if s_bytes[s_len - e_len + i] != e_bytes[i] {
            return false;
        }
        i += 1;
    }
    return true;
}

// 字符串是否包含子串 sub。
pub fn contains(s: string, sub: string): bool {
    var s_bytes = s.bytes();
    var sub_bytes = sub.bytes();
    val s_len = length(s_bytes);
    val sub_len = length(sub_bytes);
    if sub_len == 0 {
        return true;
    }
    if s_len < sub_len {
        return false;
    }
    var i = 0;
    while i <= s_len - sub_len {
        var j = 0;
        var ok = true;
        while j < sub_len {
            if s_bytes[i + j] != sub_bytes[j] {
                ok = false;
                break;
            }
            j += 1;
        }
        if ok {
            return true;
        }
        i += 1;
    }
    return false;
}

// 取子串 s[start, start+len)，返回新分配的 string（调用者负责 free）。
pub fn substring(s: string, start: i32, len: i32): string {
    var src = s.bytes();
    var buf = allocate(len as u64);
    var i = 0;
    while i < len {
        buf[i] = src[start + i];
        i += 1;
    }
    return string.from_bytes(buf);
}

// 去除首尾 ASCII 空格（32），返回新分配的 string（调用者负责 free）。
pub fn trim(s: string): string {
    var src = s.bytes();
    val s_len = length(src);
    var start = 0;
    var end = s_len;
    while start < end && src[start] == 32_u8 {
        start += 1;
    }
    while end > start && src[end - 1] == 32_u8 {
        end -= 1;
    }
    val len = end - start;
    var buf = allocate(len as u64);
    var i = 0;
    while i < len {
        buf[i] = src[start + i];
        i += 1;
    }
    return string.from_bytes(buf);
}

// ── 迭代器契约（关联类型表达产出类型，M15）───────────────────────────────

// 迭代器：`next()` 返回 `Option<Self::Item>`，产出类型由关联类型 `Item` 表达。
pub trait Iterator {
    type Item;
    fn next(self: *Self): Option<Self::Item>;
}
