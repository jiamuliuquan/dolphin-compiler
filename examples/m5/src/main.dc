fn state(enabled: bool): string {
    if enabled {
        return "ready";
    }
    return "stopped";
}

fn announce(name: string, count: i32) {
    // print 不自动换行，后面的 println 完成这一行。
    print("Hello, ");
    println("{}!", name);

    // 每个 {} 按从左到右的顺序对应一个参数。
    println("count = {}, enabled = {}", count, true);
}

fn main() {
    val name: string = "海豚";
    announce(name, 3);

    // 字符串可以作为函数返回值并直接参与格式化。
    println("state = {}", state(true));

    // 双写花括号会输出单个字面量花括号。
    println("escaped braces: {{}}");
    println("minimum i32 = {}", -2147483648);
}
