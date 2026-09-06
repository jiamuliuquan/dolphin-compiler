fn make_numbers(): [i32; 4] {
    return [1, 2, 3, 4];
}

fn sum(values: [i32; 4]): i32 {
    var total = 0;
    for value in values {
        total += value;
    }
    return total;
}

fn main() {
    // 数组按值传入和返回。修改 numbers 不会修改 make_numbers 中的值。
    var numbers = make_numbers();
    numbers[1] = 10;
    numbers[2] += 5;

    var total = sum(numbers);

    // 半开区间不包含右端点；continue 仍会进入下一次迭代。
    for i in 0..5 {
        if i == 2 {
            continue;
        }
        total += i;
    }

    // 闭区间包含右端点。
    for i in 1..=3 {
        total += i;
    }

    // 重复初始化表达式只求值一次。
    val repeated: [i32; 3] = [2; 3];
    for value in repeated {
        total += value;
    }

    println("numbers = {}, {}, {}, {}", numbers[0], numbers[1], numbers[2], numbers[3]);
    println("total = {}", total);
    return total;
}
