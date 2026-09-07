pkg dyn.vector;

// 手写动态数组（M14 最小形态：切片 + 手动管理，见提案 §5.6）。
// 本模块演示动态内存跨函数、跨模块的显式生命周期管理。

pub struct Vector {
    data: []u8,
    len: i32,
}

// 创建容量为 capacity 的空向量。
pub fn make(capacity: i32): Vector {
    return Vector(allocate(capacity as u64), 0);
}

// 追加一个字节，返回追加后的长度；容量不足由调用方保证。
pub fn push(v: *Vector, value: u8): i32 {
    // 通过指针读取 data 切片字段（浅拷贝，共享底层缓冲），再原地写入。
    var data = v->data;
    data[v->len] = value;
    v->len += 1;
    return v->len;
}

// 求和向量内全部字节。
pub fn total(v: Vector): i32 {
    var sum = 0;
    var i = 0;
    while i < v.len {
        sum += v.data[i] as i32;
        i += 1;
    }
    return sum;
}

// 释放向量占用的底层内存（释放责任由最后一个持有者承担）。
pub fn release(v: Vector) {
    free(v.data);
}
