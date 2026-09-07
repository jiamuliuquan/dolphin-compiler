fn main() {
    val small: i8 = -8_i8;
    val medium: i16 = 1600_i16;
    val large: i64 = 64000_i64;

    val byte: u8 = 8_u8;
    val word: u16 = 1600_u16;
    val count: u32 = 32000_u32;
    val huge: u64 = 64000_u64;

    val ratio: f32 = 1.5_f32;
    val precise: f64 = 2.25_f64;
    val symbol: char = '海';

    val samples: [u16; 3] = [10_u16, 20_u16, 30_u16];
    val narrowed: i32 = large as i32;

    println("signed = {}, {}, {}", small, medium, large);
    println("unsigned = {}, {}, {}, {}", byte, word, count, huge);
    println("float = {}, {}", ratio, precise);
    println("char = {}, samples = {}, {}, {}", symbol, samples[0], samples[1], samples[2]);
    println("string equal = {}, bytes = {}", "海豚" == "海豚", length("海豚"));
    println("cast result = {}", narrowed);

    return narrowed / 1000;
}
