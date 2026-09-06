fn main() {
    var total = 0;
    var i = 0;

    // while 在每次迭代前检查 bool 条件。
    while i < 10 {
        i += 1;

        // continue 跳到下一次条件检查，只累加奇数。
        if i % 2 == 0 {
            continue;
        }
        total += i;
    }

    var loops = 0;
    loop {
        loops += 1;
        if loops >= 3 {
            break;
        }
    }

    val totals_match: bool = total == 25 && loops == 3;
    val short_circuit = false && 1 / 0 == 0;

    if totals_match && !short_circuit {
        return total + loops;
    } else {
        return 1;
    }
}
