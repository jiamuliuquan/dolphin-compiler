use std.mem;

struct Record {
    id: u32,
    score: f64,
}

fn fill(records: []Record) {
    var index: usize = 0_usize;
    while index < records.len {
        records[index] = Record(index as u32, (index as f64) * 1.5);
        index += 1_usize;
    }
}

fn main() {
    val records = mem.alloc<Record>(3);
    defer mem.free(records);
    fill(records);

    var total: f64 = 0.0;
    var index: usize = 0_usize;
    while index < records.len {
        total += records[index].score;
        index += 1_usize;
    }

    val text = "dolphin";
    val bytes = text.bytes();
    val again = string.from_bytes(bytes);
    println("records={} total={} text={}", records.len, total, again == text);
    return 0;
}
