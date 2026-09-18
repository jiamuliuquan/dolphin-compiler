# M15：泛型、标准库与库包分发（M15-A…F）

演示 M15 已落地的完整能力：

- 泛型函数 `identity<T>`，显式类型实参 `identity<i32>(...)` 与由实参推断 `identity(22)`；
- 泛型结构体 `Pair<T>` 与其参数化 `impl<T> Pair<T>` 方法 `swapped`（按值接收者）；
- 泛型枚举 `Maybe<T>`、由声明类型补齐的 `Maybe.Nothing`，以及在 `impl<T>` 中 `match self`；
- 跨模块泛型类型 `util.pair.Pair<T>` 与字段 `pub` 可见性；
- trait `Head` 关联类型 `type Item = T;` 与 `Self::Item` 返回类型，静态分派 `q.head()`；
- 编译器单态化：每个 `(包身份, 限定名, 类型实参)` 只生成一次实例；
- **本地 path 依赖（M15-C）**：`examples/m15/mathlib` 是一个 `[lib]` 库，根应用用
  `math = { path = "mathlib" }` 声明别名，并 `use math;` 调用 `math.twice<i32>` / `math.add`；
  第一次构建会生成 `examples/m15/dolphin.lock`；
- 源码标准库 `std.collections.Vec<T>`：`Vec<i32>::init()`、`push`、`iter`、显式 `deinit`；
- `for` 迭代器协议与范围：`for value in numbers.iter()`、`for index in 0..=2`；
- prelude `Option`/`Result`/`Iterator` 无需 `use` 即可使用。

## 运行

```bash
./target/release/dc build examples/m15
./examples/m15/target/m15
```

输出：

```text
42 22 true
```

## 库发布（M15-D/E/F）

库项目只用 `dc build <lib> --lib`（或 `dc package`）即可产出
`target/package/<name>-<version>.dlib` 与 `.dlib.sha256`。把它们放到仓库 URL 的固定路径：

```text
<base>/<group 目录>/<name>/<version>/<name>-<version>.dlib
<base>/<group>/<name>/<version>/<name>-<version>.dlib.sha256
```

消费端在 `dolphin.toml` 里声明 `[repositories]` 与精确坐标依赖，`dc fetch` / `dc build`
会自动下载、按 SHA-256 校验、缓存并单态化源码。`dc publish <lib> --repository <id>`
使用 `If-None-Match: *` 条件 PUT（`file://` 仓库为不覆盖的原子写入）。
`dolphin.lock` 固定整个传递闭包，`dc build --locked --offline` 可在无网络时复现构建。
