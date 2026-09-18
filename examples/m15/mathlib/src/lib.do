// M15-C 本地库：通过 `[dependencies]` 的 path 依赖被根应用消费。
// 库作者只声明 lib 目标；泛型在消费端单态化。

pub fn add(a: i32, b: i32): i32 {
    return a + b;
}

pub fn twice<T>(value: T): T {
    return value + value;
}
