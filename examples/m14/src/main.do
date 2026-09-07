use dyn.vector;

struct Point {
    x: i32,
    y: i32,
}

fn translate(p: Point, dx: i32, dy: i32): Point {
    return Point(p.x + dx, p.y + dy);
}

fn build_buffer(): []u8 {
    var buf = allocate(3);
    buf[0] = 1_u8;
    buf[1] = 2_u8;
    buf[2] = 3_u8;
    return buf;
}

fn main() {
    // 结构体按值传递（M14 回溯 M13）。
    var origin = Point(10, 20);
    var moved = translate(origin, 5, 7);
    println("moved = ({}, {})", moved.x, moved.y);

    // 显式指针：取址 + `->` 字段写入。
    var q: *Point = &origin;
    q->x = 99;
    println("origin.x via ptr = {}", origin.x);

    // 动态切片：allocate + 索引读写 + length + free。
    var buf = build_buffer();
    println("buf = {} {} {}, len = {}", buf[0], buf[1], buf[2], length(buf));
    free(buf);

    // try 语法糖：局部资源，出块自动释放。
    try (var scratch = allocate(2)) {
        scratch[0] = 7_u8;
        scratch[1] = 8_u8;
        println("scratch = {} {}", scratch[0], scratch[1]);
    }

    // defer：确定性释放。
    var one = allocate(1);
    defer free(one);
    one[0] = 42_u8;

    // 字符串拼接：隐式分配，释放责任交接收者（中间结果同样需显式释放）。
    var part = "hello" + ", ";
    var greeting = part + "dolphin";
    println("{}", greeting);
    free(part);
    free(greeting);

    // 跨模块容器：手写动态数组，显式生命周期。
    var v = vector.make(4);
    var total_len = vector.push(&v, 1_u8);
    total_len = vector.push(&v, 2_u8);
    total_len = vector.push(&v, 3_u8);
    val sum = vector.total(v);
    println("vector sum = {}, len = {}", sum, total_len);
    vector.release(v);

    return moved.x + sum;
}
