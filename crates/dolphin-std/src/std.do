pkg std;

// 最小 prelude：Option、Result、Iterator 指向这里的同一定义（M15-B/R12）。
// 编译器只提供内存/布局/视图/UTF-8/C ABI 底座；这三个协议的类型写 Dolphin 源码。

pub enum Option<T> {
    Some(T),
    None,
}

pub enum Result<T, E> {
    Ok(T),
    Err(E),
}

impl<T> Option<T> {
    pub fn is_some(self: *const Self): bool {
        return match *self {
            Option.Some(value) => true,
            Option.None => false,
        };
    }

    pub fn is_none(self: *const Self): bool {
        return match *self {
            Option.Some(value) => false,
            Option.None => true,
        };
    }
}

impl<T, E> Result<T, E> {
    pub fn is_ok(self: *const Self): bool {
        return match *self {
            Result.Ok(value) => true,
            Result.Err(error) => false,
        };
    }

    pub fn is_err(self: *const Self): bool {
        return match *self {
            Result.Ok(value) => false,
            Result.Err(error) => true,
        };
    }
}

pub trait Iterator {
    type Item;
    fn next(self: *Self): Option<Self::Item>;
}
