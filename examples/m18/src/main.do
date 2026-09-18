// M18 组合回归示例：泛型容器/枚举、跨函数聚合与指针别名写、defer 清理。
use std.collections.Vec;
use std.mem;

struct Pair {
    x: i32,
    y: i32,
}

// 指针别名写：RHS 通过指针修改 y，简单字段赋值不得用旧聚合快照覆盖它（H18-01）。
fn mutate(p: *Pair): i32 {
    p->y = 9;
    return 7;
}

// 按值聚合参数与返回（sret 路径）。
fn shifted(pair: Pair, delta: i32): Pair {
    return Pair(pair.x + delta, pair.y + delta);
}

struct Box<T> {
    value: T,
}

impl<T> Box<T> {
    fn get(self: *const Self): T {
        return self->value;
    }
}

enum Maybe<T> {
    Just(T),
    Nothing,
}

fn unwrap_or(value: Maybe<i32>, fallback: i32): i32 {
    return match value {
        Maybe.Just(inner) => inner,
        Maybe.Nothing => fallback,
    };
}

// 切片跨函数传递（只读视图）。
fn sum(values: []const i32): i32 {
    var total = 0;
    for value in values {
        total += value;
    }
    return total;
}

fn main() {
    // 1) 指针别名写：p.x = mutate(&p) 之后 y 仍必须是 9。
    var pair = Pair(1, 2);
    pair.x = mutate(&pair);

    // 2) 泛型容器与枚举。
    val boxed = Box<i32>(40);
    val maybe = Maybe.Just(boxed.get());

    // 3) 按值聚合参数/返回。
    val moved = shifted(pair, 1);

    // 4) 源码标准库容器 + defer 清理。
    var numbers = Vec<i32>::init();
    defer numbers.deinit();
    numbers.push(1);
    numbers.push(2);
    numbers.push(3);

    // 5) 原始分配 + defer 释放，切片跨函数求和。
    val buffer = mem.alloc<i32>(3_usize);
    defer mem.free(buffer);
    buffer[0] = 1;
    buffer[1] = 2;
    buffer[2] = 3;

    val total = sum(buffer) + sum(numbers.as_slice());

    println("alias = {} {}", pair.x, pair.y);
    println("generic = {}", unwrap_or(maybe, -1));
    println("shifted = {} {}", moved.x, moved.y);
    println("sum = {}", total);

    if pair.x == 7 && pair.y == 9 && unwrap_or(maybe, -1) == 40 && moved.x == 8 && moved.y == 10 {
        if total == 12 {
            return 0;
        }
    }
    return 1;
}
