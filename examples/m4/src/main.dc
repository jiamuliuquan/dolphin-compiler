// main 可以调用定义在源码后面的函数。
fn main() {
    no_operation();

    if is_base_case(5) {
        return 1;
    }
    return factorial(5);
}

// 省略返回类型表示 Unit，只能使用 return; 或自然执行到末尾。
fn no_operation() {
    return;
}

fn is_base_case(value: i32): bool {
    return value <= 1;
}

fn factorial(value: i32): i32 {
    if is_base_case(value) {
        return 1;
    }
    return value * factorial(value - 1);
}
