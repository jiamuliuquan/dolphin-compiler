pkg geom;

pub struct Point {
    pub x: i32,
    pub y: i32,
}

pub enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
    Empty,
}
