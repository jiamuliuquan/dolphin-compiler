pkg result.parse;

// 解析结果的非泛型错误处理样例：枚举表示成功或失败。
pub enum ParseResult {
    Ok(i32),
    Error(i32),
}
