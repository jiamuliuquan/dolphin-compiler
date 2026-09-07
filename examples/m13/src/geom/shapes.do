pkg geom.shapes;

pub struct Point {
    x: i32,
    y: i32,
}

pub enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
    Empty,
}
