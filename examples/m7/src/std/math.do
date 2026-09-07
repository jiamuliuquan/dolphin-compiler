pkg std.math;

pub fn min(a: i32, b: i32): i32 {
    if a < b {
        return a;
    }
    return b;
}

fn max(a: i32, b: i32): i32 {
    if a > b {
        return a;
    }
    return b;
}

pub fn clamp(value: i32, lower: i32, upper: i32): i32 {
    return min(max(value, lower), upper);
}
