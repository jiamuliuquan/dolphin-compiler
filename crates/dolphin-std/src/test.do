pkg std.test;

// 用户测试断言（M19/H19-05b）。
//
// `expect(false)` / `fail()` 写 stderr 固定文本 `Dolphin test assertion failed`
// 并以 106 退出（与 trap 一样不展开 Dolphin 栈、不执行 defer）。不提供带消息的
// 断言（无格式化依赖）；需要上下文时先 `println`。`101`–`104` 的运行时错误语义不变。

extern "C" {
    fn dolphin_test_fail();
}

pub fn expect(condition: bool) {
    if !condition {
        dolphin_test_fail();
    }
}

pub fn fail() {
    dolphin_test_fail();
}
