use geom.shapes;
use result.parse;

fn main() {
    // 跨模块结构体：位置构造 + 字段访问。
    var p = shapes.Point(3, 4);
    println("point = ({}, {})", p.x, p.y);
    val manhattan = p.x + p.y;

    // 跨模块枚举：携带数据 + 无字段的枚举项。
    val circle = shapes.Shape.Circle(2.0);
    val circle_area = match circle {
        shapes.Shape.Circle(r) => 3.14 * r * r,
        shapes.Shape.Rectangle(w, h) => w * h,
        shapes.Shape.Empty => 0.0,
    };
    println("circle area = {}", circle_area);

    // 错误处理样例：解析结果枚举，match 区分成功与失败。
    val parsed = parse.ParseResult.Ok(42);
    val value = match parsed {
        parse.ParseResult.Ok(v) => v,
        parse.ParseResult.Error(_) => -1,
    };
    println("parsed value = {}", value);

    val failed = parse.ParseResult.Error(1);
    val fallback = match failed {
        parse.ParseResult.Ok(v) => v,
        parse.ParseResult.Error(_) => -1,
    };
    println("fallback = {}", fallback);

    return manhattan + value;
}
