//! H18-03 饱和数值转换回归（CAST-01..02）。
//!
//! CAST-02 的期望由 Rust 自身的浮点到整数 `as` 语义生成（截断、饱和、NaN→0），
//! 不调用待测后端作为 oracle；每个组合在可用后端 × Debug/Release 上运行。

mod support;

use support::assert_runs;

const CAST_01: &str = r#"
fn convert(x: f64): i32 { return x as i32; }

fn main() {
    println("{}", convert(100000000000000000000.0));
    return 0;
}
"#;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    I8,
    I16,
    I32,
    I64,
    Isize,
    U8,
    U16,
    U32,
    U64,
    Usize,
}

impl Target {
    const ALL: [Target; 10] = [
        Target::I8,
        Target::I16,
        Target::I32,
        Target::I64,
        Target::Isize,
        Target::U8,
        Target::U16,
        Target::U32,
        Target::U64,
        Target::Usize,
    ];

    fn name(self) -> &'static str {
        match self {
            Target::I8 => "i8",
            Target::I16 => "i16",
            Target::I32 => "i32",
            Target::I64 => "i64",
            Target::Isize => "isize",
            Target::U8 => "u8",
            Target::U16 => "u16",
            Target::U32 => "u32",
            Target::U64 => "u64",
            Target::Usize => "usize",
        }
    }

    fn bits(self) -> u32 {
        match self {
            Target::I8 | Target::U8 => 8,
            Target::I16 | Target::U16 => 16,
            Target::I32 | Target::U32 => 32,
            Target::I64 | Target::U64 | Target::Isize | Target::Usize => 64,
        }
    }

    fn is_signed(self) -> bool {
        matches!(
            self,
            Target::I8 | Target::I16 | Target::I32 | Target::I64 | Target::Isize
        )
    }
}

/// Rust 的浮点到整数 `as`：向零截断、越界饱和、NaN→0，与合同表格一致。
fn rust_cast(target: Target, value: f64) -> String {
    match target {
        Target::I8 => (value as i8).to_string(),
        Target::I16 => (value as i16).to_string(),
        Target::I32 => (value as i32).to_string(),
        Target::I64 | Target::Isize => (value as i64).to_string(),
        Target::U8 => (value as u8).to_string(),
        Target::U16 => (value as u16).to_string(),
        Target::U32 => (value as u32).to_string(),
        Target::U64 | Target::Usize => (value as u64).to_string(),
    }
}

struct Fixture {
    expr: String,
    value: f64,
}

fn literal(value: f64) -> String {
    format!("{value:.1}_f64")
}

fn fixtures(target: Target) -> Vec<Fixture> {
    let bits = target.bits();
    let mut items: Vec<(String, f64)> = vec![
        (literal(1.9), 1.9),
        (literal(-1.9), -1.9),
        (literal(2.5), 2.5),
        (literal(-2.5), -2.5),
        (literal(0.0), 0.0),
        ("fneg(0.0_f64)".to_string(), -0.0),
        ("fdiv(0.0_f64, 0.0_f64)".to_string(), f64::NAN),
        ("fdiv(1.0_f64, 0.0_f64)".to_string(), f64::INFINITY),
        ("fdiv(-1.0_f64, 0.0_f64)".to_string(), f64::NEG_INFINITY),
        (literal(-1.0), -1.0),
        (literal(1e20), 1e20),
    ];
    if target.is_signed() {
        let (near_hi, over_hi, near_lo, under_lo) = match bits {
            64 => (
                9223372036854774784.0,
                9223372036854775808.0,
                -9223372036854775808.0,
                -9223372036854777856.0,
            ),
            32 => (2147483647.0, 2147483648.0, -2147483648.0, -2147483649.0),
            16 => (32767.0, 32768.0, -32768.0, -32769.0),
            _ => (127.0, 128.0, -128.0, -129.0),
        };
        items.push((literal(near_hi), near_hi));
        items.push((literal(over_hi), over_hi));
        items.push((literal(near_lo), near_lo));
        items.push((literal(under_lo), under_lo));
    } else {
        let near_hi = if bits == 64 {
            9223372036854775808.0
        } else {
            2f64.powi(bits as i32) - 1.0
        };
        let over_hi = 2f64.powi(bits as i32);
        items.push((literal(near_hi), near_hi));
        items.push((literal(over_hi), over_hi));
    }
    items
        .into_iter()
        .map(|(expr, value)| Fixture { expr, value })
        .collect()
}

fn matrix_source() -> String {
    let mut source = String::from(
        "fn to_f32(x: f64): f32 { return x as f32; }\n\
         fn fdiv(a: f64, b: f64): f64 { return a / b; }\n\
         fn fneg(x: f64): f64 { return -x; }\n",
    );
    for target in Target::ALL {
        let name = target.name();
        source.push_str(&format!(
            "fn cast_f64_{name}(x: f64): {name} {{ return x as {name}; }}\n"
        ));
        source.push_str(&format!(
            "fn cast_f32_{name}(x: f32): {name} {{ return x as {name}; }}\n"
        ));
    }
    source.push_str("\nfn main() {\n");
    for target in Target::ALL {
        let fx = fixtures(target);
        let placeholders = vec!["{}"; fx.len()].join(" ");
        for source_name in ["f64", "f32"] {
            let arguments: Vec<String> = fx
                .iter()
                .map(|fixture| {
                    let expr = if source_name == "f32" {
                        format!("to_f32({})", fixture.expr)
                    } else {
                        fixture.expr.clone()
                    };
                    format!("cast_{source_name}_{}({expr})", target.name())
                })
                .collect();
            source.push_str(&format!(
                "    println(\"{source_name} {} {placeholders}\", {});\n",
                target.name(),
                arguments.join(", ")
            ));
        }
    }
    source.push_str("    return 0;\n}\n");
    source
}

fn matrix_expected() -> String {
    let mut expected = String::new();
    for target in Target::ALL {
        let fx = fixtures(target);
        for source_name in ["f64", "f32"] {
            let values: Vec<String> = fx
                .iter()
                .map(|fixture| {
                    let value = if source_name == "f32" {
                        f64::from(fixture.value as f32)
                    } else {
                        fixture.value
                    };
                    rust_cast(target, value)
                })
                .collect();
            expected.push_str(&format!(
                "{source_name} {} {}\n",
                target.name(),
                values.join(" ")
            ));
        }
    }
    expected
}

#[test]
fn cast_01_float_to_int_saturates() {
    assert_runs(&[("src/main.do", CAST_01)], "2147483647\n", 0);
}

#[test]
fn cast_02_full_matrix_saturates_with_nan_zero() {
    let source = matrix_source();
    let expected = matrix_expected();
    assert_runs(&[("src/main.do", source.as_str())], &expected, 0);
}
