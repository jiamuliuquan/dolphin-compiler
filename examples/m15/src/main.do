use math;
use std.collections.Vec;
use util.pair;
use util.pair.Head;

struct Local<T> {
    value: T,
}

impl<T> Local<T> {
    fn get(self: *const Self): T {
        return self->value;
    }
}

fn identity<T>(value: T): T {
    return value;
}

enum Maybe<T> {
    Just(T),
    Nothing,
}

impl<T> Maybe<T> {
    fn is_nothing(self): bool {
        return match self {
            Maybe.Just(value) => false,
            Maybe.Nothing => true,
        };
    }
}

fn main() {
    val p = pair.Pair<i32>(identity<i32>(20), identity(22));
    val q = p.swapped();
    val empty: Maybe<i32> = Maybe.Nothing;
    val local = Local<i32>(q.head());

    // 跨包泛型：mathlib 的 add 与 twice 在消费端单态化。
    if math.twice<i32>(21) != 42 {
        return 1;
    }
    if math.add(q.first, q.second) != 42 {
        return 2;
    }

    // 源码标准库：Vec<T> + Iterator 协议 + prelude Option。
    var numbers = Vec<i32>::init();
    defer numbers.deinit();
    numbers.push(1);
    numbers.push(2);
    numbers.push(3);
    var total = 0;
    for value in numbers.iter() {
        total += value;
    }
    for index in 0..=2 {
        total += index;
    }
    val found: Option<i32> = Option.Some(total);
    if found.is_none() {
        return 3;
    }

    println("{} {} {}", math.add(q.first, q.second), local.get(), empty.is_nothing());
    return 0;
}
