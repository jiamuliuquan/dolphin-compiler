use std.mem;

extern struct CPoint {
    x: f64,
    y: f64,
}

extern "C" {
    pub fn demo_add(a: i32, b: i32): i32;
    pub fn demo_fill(out: *u8, len: usize): i32;
    pub fn demo_create(): *Unit;
    pub fn demo_destroy(handle: *Unit);
    pub fn demo_translate_x(point: *CPoint, dx: f64): f64;
    pub fn demo_sizeof_point(): usize;
}

fn main() {
    val bytes = mem.alloc<u8>(4);
    defer mem.free(bytes);
    val status = demo_fill(bytes.ptr, bytes.len);
    if status != 0 { return status; }

    val handle = demo_create();
    if handle == null { return 1; }
    defer demo_destroy(handle);

    val point = mem.create<CPoint>(CPoint(1.0, 2.0));
    defer mem.destroy(point);
    val moved = demo_translate_x(point, 10.0);
    val size_matches = mem.size_of<CPoint>() == demo_sizeof_point();

    println(
    "bytes={} {} {} {} moved={} size={}",
    bytes[0], bytes[1], bytes[2], bytes[3], moved, size_matches
    );
    return demo_add(20, 22);
}
