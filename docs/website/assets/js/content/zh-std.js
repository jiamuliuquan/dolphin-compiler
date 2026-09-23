window.DolphinDocsContent = window.DolphinDocsContent || {};
window.DolphinDocsContent["zh-CN"] = window.DolphinDocsContent["zh-CN"] || { groups: [] };
window.DolphinDocsContent["zh-CN"].groups.push({
  id: "std",
  title: "标准库参考",
  pages: [
    {
      id: "std/overview",
      title: "标准库概览",
      body: `
<h1>标准库概览</h1>
<p>Dolphin 标准库由两部分组成：编译器内建入口，以及随编译器分发的源码标准库。内建部分负责内存、布局、视图、UTF-8 与 C ABI 底座；容器与文本协议则完全用 Dolphin 源码实现，因此你也可以阅读并扩展它们。</p>

<h2>1. 模块列表</h2>
<table>
  <thead><tr><th>模块</th><th>导入方式</th><th>内容</th></tr></thead>
  <tbody>
    <tr><td><code>std</code></td><td>自动 prelude</td><td><code>Option&lt;T&gt;</code>、<code>Result&lt;T, E&gt;</code>、<code>Iterator</code></td></tr>
    <tr><td><code>std.mem</code></td><td><code>use std.mem;</code></td><td>分配、释放、复制、布局查询、视图与指针转换（内建）</td></tr>
    <tr><td><code>std.collections</code></td><td><code>use std.collections.Vec;</code></td><td><code>Vec&lt;T&gt;</code>、<code>SliceIter&lt;T&gt;</code>、<code>Range</code></td></tr>
    <tr><td><code>std.text</code></td><td><code>use std.text;</code></td><td><code>String</code>、<code>lines</code>、<code>Builder</code>、<code>parse_i64</code>/<code>parse_u64</code>、<code>concat</code>、<code>trim</code>、<code>substring</code>、<code>from_utf8</code> 等</td></tr>
    <tr><td><code>std.process</code></td><td><code>use std.process.arg;</code></td><td>进程参数与环境</td></tr>
    <tr><td><code>std.error</code></td><td><code>use std.error.Error;</code></td><td><code>Error</code>、<code>ErrorKind</code></td></tr>
    <tr><td><code>std.io</code></td><td><code>use std.io.Stream;</code></td><td><code>Stream</code>、标准流、字节读写、<code>eprint</code></td></tr>
    <tr><td><code>std.fs</code></td><td><code>use std.fs.open;</code></td><td><code>open</code>、<code>OpenMode</code></td></tr>
    <tr><td><code>std.test</code></td><td><code>use std.test.expect;</code></td><td><code>expect</code>、<code>fail</code>（供 <code>dc test</code>）</td></tr>
    <tr><td><code>std.ffi</code></td><td><code>use std.ffi.CString;</code></td><td><code>CString</code>、<code>CStringError</code></td></tr>
  </tbody>
</table>
<div class="callout">
  <p><strong>命名空间保留：</strong><code>std</code> 由标准库独占，用户模块不能占用该命名空间。标准库的泛型在消费端单态化，且只在首次使用时生成代码。</p>
</div>

<h2>2. 最小 prelude</h2>
<p>以下三个协议无需导入即可使用：</p>
<pre><code>enum Option&lt;T&gt; {
    Some(T),
    None,
}

enum Result&lt;T, E&gt; {
    Ok(T),
    Err(E),
}

trait Iterator {
    type Item;
    fn next(self: *Self): Option&lt;Self::Item&gt;;
}</code></pre>
<p>当当前模块定义或显式导入了同名类型时，prelude 中的定义会被遮蔽。详见 <a href="#/std/prelude">Option、Result 与 Iterator</a>。</p>

<h2>3. 内建函数与类型</h2>
<p>以下名字由编译器直接提供：</p>
<table>
  <thead><tr><th>名称</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>print</code> / <code>println</code></td><td>格式化输出（内建函数，不能被用户函数覆盖）</td></tr>
    <tr><td><code>length(s)</code></td><td>返回 UTF-8 字节数，类型为 <code>usize</code></td></tr>
    <tr><td><code>string.from_bytes(bytes)</code></td><td>校验 UTF-8 后返回 <code>string</code> 视图</td></tr>
    <tr><td><code>s.bytes()</code></td><td>返回只读字节视图 <code>[]const u8</code></td></tr>
    <tr><td><code>mem.*</code></td><td>见 <a href="#/std/mem">std.mem</a></td></tr>
  </tbody>
</table>
<p>完整的内建函数、格式化规则与诊断类别见 <a href="#/std/builtins">内建函数与格式化</a>。</p>

<h2>4. 如何阅读 API 文档</h2>
<p>本参考中的签名使用 Dolphin 语法。接收者 <code>self: *Self</code> 表示方法可修改对象，<code>self: *const Self</code> 表示只读；对可寻址的局部变量与方法返回值调用时会自动取址。拥有型资源（如 <code>Vec</code>、<code>String</code>、<code>CString</code>）必须显式调用 <code>deinit</code> 释放。</p>
<pre><code>use std.collections.Vec;

fn main() {
    var values = Vec&lt;i32&gt;::init();
    defer values.deinit();     // 显式释放

    values.push(1);
    println("len = {}", values.len());
}</code></pre>

<h2>5. 标准库的源码位置</h2>
<p>源码标准库位于编译器仓库的 <code>crates/dolphin-std/src/</code> 目录：</p>
<ul>
  <li><code>std.do</code>：<code>Option</code>、<code>Result</code>、<code>Iterator</code>。</li>
  <li><code>collections.do</code>：<code>Vec</code>、<code>SliceIter</code>、<code>Range</code>。</li>
  <li><code>text.do</code>：<code>String</code>、<code>lines</code>、<code>Builder</code> 与解析器。</li>
  <li><code>process.do</code>、<code>error.do</code>、<code>io.do</code>、<code>fs.do</code>、<code>test.do</code>：参数/环境、错误、流、文件与测试断言。</li>
  <li><code>ffi.do</code>：<code>CString</code>。</li>
</ul>
<p>这些文件在每次构建时与用户源码一起解析，携带保留身份 <code>PackageId::STD</code>。</p>

<h2>6. 相关章节</h2>
<ul>
  <li><a href="#/std/prelude">Option、Result 与 Iterator</a></li>
  <li><a href="#/std/mem">std.mem 内存 API</a></li>
  <li><a href="#/std/collections">std.collections 容器</a></li>
  <li><a href="#/std/text">std.text 文本处理</a></li>
  <li><a href="#/std/io">进程、流与文件</a></li>
  <li><a href="#/std/ffi">std.ffi C 字符串</a></li>
  <li><a href="#/tutorial/testing">测试代码</a></li>
</ul>
`
    },

    {
      id: "std/prelude",
      title: "Option、Result 与 Iterator",
      body: `
<h1>Option、Result 与 Iterator</h1>
<p>这三个类型定义在 <code>std</code> 模块中，并作为最小 prelude 自动可用。</p>

<h2>1. Option&lt;T&gt;</h2>
<p><code>Option&lt;T&gt;</code> 表示「可能有值」。语言不提供隐式 <code>null</code>，需要表达缺失时使用 <code>Option</code>：</p>
<pre><code>enum Option&lt;T&gt; {
    Some(T),
    None,
}</code></pre>
<p>构造与匹配：</p>
<pre><code>val maybe: Option&lt;i32&gt; = Option.Some(42);

val text = match maybe {
    Option.Some(value) =&gt; "got a value",
    Option.None =&gt; "nothing",
};</code></pre>
<h3>方法</h3>
<table>
  <thead><tr><th>签名</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>is_some(self: *const Self): bool</code></td><td>是否为 <code>Some</code></td></tr>
    <tr><td><code>is_none(self: *const Self): bool</code></td><td>是否为 <code>None</code></td></tr>
  </tbody>
</table>
<pre><code>val found: Option&lt;i32&gt; = Option.Some(7);
if found.is_some() {
    println("found");
}
if found.is_none() {
    println("missing");
}</code></pre>

<h2>2. Result&lt;T, E&gt;</h2>
<p><code>Result&lt;T, E&gt;</code> 表示可恢复的成功或失败，<code>Ok</code> 携带成功值，<code>Err</code> 携带错误值：</p>
<pre><code>enum Result&lt;T, E&gt; {
    Ok(T),
    Err(E),
}</code></pre>
<pre><code>fn parse(value: i32): Result&lt;i32, TextError&gt; {
    if value &lt; 0 {
        return Result.Err(TextError.InvalidBoundary);
    }
    return Result.Ok(value);
}</code></pre>
<h3>方法</h3>
<table>
  <thead><tr><th>签名</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>is_ok(self: *const Self): bool</code></td><td>是否为 <code>Ok</code></td></tr>
    <tr><td><code>is_err(self: *const Self): bool</code></td><td>是否为 <code>Err</code></td></tr>
  </tbody>
</table>
<div class="callout warn">
  <p><strong>当前限制：</strong>语言尚未提供 <code>?</code> 错误传播运算符，必须用 <code>match</code> 显式处理两个分支。</p>
</div>

<h2>3. Iterator</h2>
<p><code>for x in expr</code> 要求 <code>expr</code> 实现 <code>Iterator</code> 协议：</p>
<pre><code>pub trait Iterator {
    type Item;
    fn next(self: *Self): Option&lt;Self::Item&gt;;
}</code></pre>
<p>实现一个迭代器：</p>
<pre><code>struct Countdown {
    remaining: i32,
}

impl Iterator for Countdown {
    type Item = i32;

    fn next(self: *Self): Option&lt;i32&gt; {
        if self-&gt;remaining &lt;= 0 {
            return Option.None;
        }
        self-&gt;remaining -= 1;
        return Option.Some(self-&gt;remaining + 1);
    }
}

fn main() {
    var it = Countdown(3);
    for value in it {
        println("{}", value);   // 3、2、1
    }
}</code></pre>
<h3>内建适配</h3>
<ul>
  <li>数组与切片经只读切片适配到 <code>SliceIter&lt;T&gt;</code>。</li>
  <li><code>start..end</code> / <code>start..=end</code> 降低为 <code>Range</code>。</li>
  <li><code>s.iter()</code> 零分配返回切片迭代器。</li>
  <li><code>break</code> / <code>continue</code> 与每轮 <code>defer</code> 走同一清理路径。</li>
</ul>

<h2>4. 组合示例</h2>
<pre><code>use std.collections.Vec;

fn main() {
    var values = Vec&lt;i32&gt;::init();
    defer values.deinit();
    values.push(4);
    values.push(7);

    val first = values.get(0_usize);
    val label = match first {
        Option.Some(value) =&gt; "has first",
        Option.None =&gt; "empty",
    };
    println("{}", label);
}</code></pre>
`
    },

    {
      id: "std/mem",
      title: "std.mem 内存 API",
      body: `
<h1>std.mem 内存 API</h1>
<p><code>std.mem</code> 是编译器内建入口，无需磁盘标准库。它提供显式分配、释放、复制、布局查询与视图/指针转换。泛型 API 需要显式类型实参，例如 <code>mem.alloc&lt;i32&gt;(n)</code>。</p>

<h2>1. 分配与释放</h2>
<table>
  <thead><tr><th>API</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>mem.alloc&lt;T&gt;(count: usize): []T</code></td><td>分配 <code>count</code> 个未初始化 <code>T</code> 的连续存储。返回可写切片；调用者负责初始化并在之后交给 <code>mem.free</code>。</td></tr>
    <tr><td><code>mem.free&lt;T&gt;(buffer: []T)</code></td><td>释放由 <code>alloc</code> 返回的完整原始切片。不递归释放元素，也不会深拷贝。</td></tr>
    <tr><td><code>mem.create&lt;T&gt;(value: T): *T</code></td><td>分配并初始化单个对象，返回其指针。</td></tr>
    <tr><td><code>mem.destroy&lt;T&gt;(ptr: *T)</code></td><td>释放由 <code>create</code> 创建的对象。</td></tr>
  </tbody>
</table>
<pre><code>use std.mem;

fn main() {
    val buffer = mem.alloc&lt;i32&gt;(4_usize);
    defer mem.free(buffer);

    var index: usize = 0_usize;
    while index &lt; buffer.len {
        buffer[index] = index as i32;
        index += 1_usize;
    }
    println("len = {}", buffer.len);
}</code></pre>
<div class="callout warn">
  <p><strong>释放规则：</strong>只能释放 <code>alloc</code> 返回的完整切片，不能释放切片子视图或字符串字面量。长度不匹配、未知地址或重复释放会产生运行时诊断（见下文）。</p>
</div>

<h2>2. 布局常量</h2>
<table>
  <thead><tr><th>API</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>mem.size_of&lt;T&gt;(): usize</code></td><td>类型 <code>T</code> 的字节大小，编译期常量</td></tr>
    <tr><td><code>mem.align_of&lt;T&gt;(): usize</code></td><td>类型 <code>T</code> 的对齐要求，编译期常量</td></tr>
  </tbody>
</table>
<pre><code>struct Point { x: f64, y: f64 }

println("size = {}, align = {}", mem.size_of&lt;Point&gt;(), mem.align_of&lt;Point&gt;());</code></pre>

<h2>3. 复制与视图</h2>
<table>
  <thead><tr><th>API</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>mem.copy&lt;T&gt;(dst: []T, src: []const T)</code></td><td>等长按值复制，使用 <code>memmove</code> 语义，允许区间重叠；不分配，也不深拷贝</td></tr>
    <tr><td><code>mem.is_valid_utf8(bytes: []const u8): bool</code></td><td>只校验 UTF-8 编码，不分配、不 trap</td></tr>
    <tr><td><code>mem.view&lt;T&gt;(ptr: *T, len: usize): []T</code></td><td>从 C 指针创建可写视图，不取得释放权</td></tr>
    <tr><td><code>mem.view_const&lt;T&gt;(ptr: *const T, len: usize): []const T</code></td><td>从 C 指针创建只读视图</td></tr>
    <tr><td><code>mem.cast_ptr&lt;T&gt;(ptr: *Unit): *T</code></td><td>把 <code>void*</code> 转为对象指针，校验对齐</td></tr>
    <tr><td><code>mem.cast_const_ptr&lt;T&gt;(ptr: *const Unit): *const T</code></td><td>把 <code>const void*</code> 转为只读对象指针，校验对齐</td></tr>
  </tbody>
</table>
<pre><code>val source = [1, 2, 3];
val destination = mem.alloc&lt;i32&gt;(3_usize);
defer mem.free(destination);
mem.copy&lt;i32&gt;(destination, source);</code></pre>

<h2>4. 切片子视图</h2>
<p>对切片调用 <code>slice(start, end)</code> 得到子视图，始终检查 <code>0 &lt;= start &lt;= end &lt;= len</code>：</p>
<pre><code>val buffer = mem.alloc&lt;u8&gt;(8_usize);
defer mem.free(buffer);
val middle = buffer.slice(2_usize, 6_usize);
println("middle len = {}", middle.len);</code></pre>
<p>子视图不拥有内存，不能单独释放。切片字段 <code>.ptr</code> 与 <code>.len</code> 是只读的；<code>string</code> 的 <code>.len</code> 表示 UTF-8 字节数。</p>

<h2>5. defer 与所有权</h2>
<p><code>defer</code> 是唯一的作用域清理语法，块退出时按注册逆序执行。调用参数在退出时求值，名称绑定注册处的局部变量身份：</p>
<pre><code>fn process() {
    val buffer = mem.alloc&lt;u32&gt;(16_usize);
    defer mem.free(buffer);
    // ... 使用 buffer，函数返回时自动释放
}</code></pre>
<p><code>return</code>、<code>break</code>、<code>continue</code> 出块时都会清理实际退出到的作用域；<code>return expr</code> 先求值并保存返回值再清理。trap 或 OOM 不展开 Dolphin 栈，因此不执行 <code>defer</code>。</p>

<h2>6. 失败模式</h2>
<table>
  <thead><tr><th>退出码</th><th>含义</th></tr></thead>
  <tbody>
    <tr><td><code>101</code></td><td>算术 trap、切片越界、无效切片区间、<code>null</code> 视图 / 对齐检查失败</td></tr>
    <tr><td><code>102</code></td><td>分配失败 / 分配尺寸溢出</td></tr>
    <tr><td><code>103</code></td><td>无效释放（Debug 检测；Release 不保证）</td></tr>
    <tr><td><code>104</code></td><td>UTF-8 校验失败</td></tr>
    <tr><td><code>106</code></td><td><code>std.test.expect(false)</code> 或 <code>fail()</code> 的测试断言失败</td></tr>
  </tbody>
</table>
<p>Debug 构建会链接带存活分配登记表的检测版运行时，正常退出时若存在泄漏会在 stderr 报告，但保留程序退出码。</p>

<h2>7. 与源码标准库的关系</h2>
<p><code>std.mem</code> 只提供底座。<code>Vec&lt;T&gt;</code>、<code>String</code>、<code>CString</code> 等拥有型容器都由 Dolphin 源码在其上实现，并显式调用 <code>mem.alloc</code> / <code>mem.free</code>。阅读它们的源码可以学习如何构建安全的抽象。</p>
`
    },

    {
      id: "std/collections",
      title: "std.collections 容器",
      body: `
<h1>std.collections 容器</h1>
<p><code>std.collections</code> 提供动态数组 <code>Vec&lt;T&gt;</code> 以及切片与范围迭代器 <code>SliceIter&lt;T&gt;</code>、<code>Range</code>。</p>
<pre><code>use std.collections.Vec;</code></pre>

<h2>1. Vec&lt;T&gt;</h2>
<p><code>Vec&lt;T&gt;</code> 是一个可增长的拥有型连续数组。它拥有底层缓冲，必须显式 <code>deinit</code>。</p>
<pre><code>pub struct Vec&lt;T&gt; {
    storage: []T,
    used: usize,
}</code></pre>

<h3>构造</h3>
<table>
  <thead><tr><th>签名</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>Vec&lt;T&gt;::init(): Vec&lt;T&gt;</code></td><td>创建空向量，初始容量为 0</td></tr>
    <tr><td><code>Vec&lt;T&gt;::with_capacity(capacity: usize): Vec&lt;T&gt;</code></td><td>预分配至少 <code>capacity</code> 个元素的空间</td></tr>
  </tbody>
</table>

<h3>容量与长度</h3>
<table>
  <thead><tr><th>签名</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>len(self: *const Self): usize</code></td><td>已用元素个数</td></tr>
    <tr><td><code>capacity(self: *const Self): usize</code></td><td>当前缓冲可容纳的元素个数</td></tr>
    <tr><td><code>is_empty(self: *const Self): bool</code></td><td>是否为空</td></tr>
    <tr><td><code>reserve(self: *Self, additional: usize)</code></td><td>确保至少能容纳 <code>additional</code> 个额外元素，按需扩容</td></tr>
  </tbody>
</table>

<h3>元素访问与修改</h3>
<table>
  <thead><tr><th>签名</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>push(self: *Self, value: T)</code></td><td>在末尾追加元素，必要时扩容（初始按 4、之后翻倍）</td></tr>
    <tr><td><code>pop(self: *Self): Option&lt;T&gt;</code></td><td>移除并返回末尾元素，空时返回 <code>None</code></td></tr>
    <tr><td><code>get(self: *const Self, index: usize): Option&lt;T&gt;</code></td><td>按下标读取副本，越界返回 <code>None</code></td></tr>
    <tr><td><code>set(self: *Self, index: usize, value: T)</code></td><td>按下标写入，越界触发运行时检查</td></tr>
    <tr><td><code>as_slice(self: *const Self): []const T</code></td><td>只读视图，范围为已用元素</td></tr>
    <tr><td><code>as_mut_slice(self: *Self): []T</code></td><td>可写视图，范围为已用元素</td></tr>
  </tbody>
</table>

<h3>迭代、复制与清理</h3>
<table>
  <thead><tr><th>签名</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>iter(self: *const Self): SliceIter&lt;T&gt;</code></td><td>返回零分配迭代器，可直接用于 <code>for</code></td></tr>
    <tr><td><code>clone(self: *const Self): Vec&lt;T&gt;</code></td><td>浅复制所有已用元素，返回新的拥有型向量</td></tr>
    <tr><td><code>clear(self: *Self)</code></td><td>清空元素，保留容量</td></tr>
    <tr><td><code>deinit(self: *Self)</code></td><td>释放底层缓冲并重置为空</td></tr>
  </tbody>
</table>
<div class="callout warn">
  <p><strong>扩容与溢出：</strong>当容量或元素数接近 <code>usize</code> 上限时，扩容路径会以 <code>102</code> 安全终止，而不会发生整数回绕。</p>
</div>

<h3>完整示例</h3>
<pre><code>use std.collections.Vec;

fn main() {
    var numbers = Vec&lt;i32&gt;::with_capacity(2_usize);
    defer numbers.deinit();

    numbers.push(10);
    numbers.push(20);
    numbers.push(30);

    if numbers.len() == 3_usize &amp;&amp; numbers.capacity() &gt;= 3_usize {
        println("grew as expected");
    }

    val removed = numbers.pop();
    val last = match removed {
        Option.Some(value) =&gt; value,
        Option.None =&gt; 0,
    };
    println("popped = {}", last);

    var total = 0;
    for value in numbers.iter() {
        total += value;
    }
    println("total = {}", total);

    var copy = numbers.clone();
    defer copy.deinit();
    println("copy len = {}", copy.len());
}</code></pre>

<h2>2. SliceIter&lt;T&gt;</h2>
<p>只读切片迭代器，由数组、切片与 <code>Vec::iter()</code> 适配得到。</p>
<pre><code>pub struct SliceIter&lt;T&gt; {
    storage: []const T,
    index: usize,
}</code></pre>
<table>
  <thead><tr><th>签名</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>SliceIter&lt;T&gt;::init(storage: []const T)</code></td><td>从只读切片创建</td></tr>
    <tr><td><code>len(self: *const Self): usize</code></td><td>剩余元素个数</td></tr>
    <tr><td><code>is_empty(self: *const Self): bool</code></td><td>是否已耗尽</td></tr>
    <tr><td><code>next(self: *Self): Option&lt;T&gt;</code></td><td><code>Iterator</code> 实现，逐个产出元素副本</td></tr>
  </tbody>
</table>

<h2>3. Range</h2>
<p>整数范围迭代器，由 <code>start..end</code> 与 <code>start..=end</code> 降低得到。</p>
<table>
  <thead><tr><th>签名</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>Range::exclusive(start: i32, end: i32): Range</code></td><td>半开区间 <code>start..end</code></td></tr>
    <tr><td><code>Range::inclusive(start: i32, end: i32): Range</code></td><td>闭区间 <code>start..=end</code></td></tr>
    <tr><td><code>next(self: *Self): Option&lt;i32&gt;</code></td><td><code>Iterator</code> 实现，安全处理端点，不会越过 <code>i32::MAX</code> 回绕</td></tr>
  </tbody>
</table>
<pre><code>for i in 0..=2 {
    println("{}", i);   // 0、1、2
}</code></pre>
`
    },

    {
      id: "std/text",
      title: "std.text 文本处理",
      body: `
<h1>std.text 文本处理</h1>
<p><code>std.text</code> 提供拥有型 UTF-8 字符串 <code>String</code>、零分配视图工具、增长式 <code>Builder</code> 与整数解析。内建的 <code>string</code> 是只读视图，<code>String</code> 与 <code>Builder</code> 是可释放的拥有型缓冲。</p>
<pre><code>use std.text;</code></pre>

<h2>1. 错误类型</h2>
<pre><code>pub enum TextError {
    InvalidUtf8,
    InvalidBoundary,
    OutOfBounds,
}

pub enum NumberError {
    Empty,
    InvalidDigit,
    Overflow,
}</code></pre>

<h2>2. String</h2>
<pre><code>pub struct String {
    bytes: []u8,
}</code></pre>
<table>
  <thead><tr><th>签名</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>String::from(s: string): String</code></td><td>复制一个 <code>string</code> 视图的内容，得到拥有型缓冲</td></tr>
    <tr><td><code>view(self: *const Self): string</code></td><td>返回零分配的只读视图</td></tr>
    <tr><td><code>clone(self: *const Self): String</code></td><td>深复制，返回新的拥有型字符串</td></tr>
    <tr><td><code>deinit(self: *Self)</code></td><td>释放底层缓冲</td></tr>
  </tbody>
</table>
<pre><code>use std.text;

fn main() {
    var owned = text.String::from("Dolphin");
    defer owned.deinit();

    val view = owned.view();
    println("{}", view);          // Dolphin
    println("len = {}", length(view));
}</code></pre>

<h2>3. 文本函数</h2>
<table>
  <thead><tr><th>签名</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>concat(a: string, b: string): String</code></td><td>拼接两个字符串，返回新的拥有型字符串（溢出时走 <code>102</code> 通道）</td></tr>
    <tr><td><code>trim(s: string): string</code></td><td>去除首尾 ASCII 空白，返回原缓冲上的视图</td></tr>
    <tr><td><code>substring(s: string, start: usize, end: usize): Result&lt;string, TextError&gt;</code></td><td>按字节区间取子串；越界返回 <code>OutOfBounds</code>，切断码点返回 <code>InvalidBoundary</code></td></tr>
    <tr><td><code>from_utf8(bytes: []const u8): Result&lt;string, TextError&gt;</code></td><td>校验 UTF-8 后返回视图；失败返回 <code>InvalidUtf8</code></td></tr>
    <tr><td><code>starts_with(s: string, part: string): bool</code></td><td>是否以 <code>part</code> 开头</td></tr>
    <tr><td><code>ends_with(s: string, part: string): bool</code></td><td>是否以 <code>part</code> 结尾</td></tr>
    <tr><td><code>contains(s: string, part: string): bool</code></td><td>是否包含 <code>part</code>（空串返回 <code>false</code>）</td></tr>
  </tbody>
</table>

<h3>示例</h3>
<pre><code>use std.text;

fn main() {
    val raw = "  Dolphin  ";
    val trimmed = text.trim(raw);
    var combined = text.concat(trimmed, "!");
    defer combined.deinit();

    println("{}", combined.view());            // Dolphin!
    println("{}", text.starts_with(trimmed, "Dol"));
    println("{}", text.ends_with(trimmed, "in"));
    println("{}", text.contains(trimmed, "lph"));

    val part = text.substring("dolphin", 0_usize, 4_usize);
    val label = match part {
        Result.Ok(value) =&gt; value,
        Result.Err(error) =&gt; "invalid",
    };
    println("{}", label);                       // dolp
}</code></pre>
<div class="callout">
  <p><strong>字节与字符：</strong><code>length</code> 与区间参数都以 UTF-8 字节为单位。<code>substring</code> 会检查边界是否落在码点起始处，避免产生无效 UTF-8。</p>
</div>

<h2>4. 行、Builder 与数值</h2>
<table>
  <thead><tr><th>签名</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>lines(bytes: []const u8): Lines</code></td><td>按 <code>\n</code> 切分字节行；紧邻 <code>\n</code> 前的一个 <code>\r</code> 会被去掉，无 <code>\n</code> 的非空尾段算一行。产出视图，零分配。</td></tr>
    <tr><td><code>Builder</code></td><td>增长式字节缓冲：<code>init</code>/<code>with_capacity</code>/<code>append</code>/<code>append_bytes</code>/<code>len</code>/<code>is_empty</code>/<code>view</code>/<code>consume</code>/<code>clear</code>/<code>deinit</code>。<code>view()</code> 在下一次修改后失效。</td></tr>
    <tr><td><code>parse_i64(s: string): Result&lt;i64, NumberError&gt;</code></td><td>可选 <code>-</code>/<code>+</code> 加 ASCII 数字；溢出返回 <code>Overflow</code>，不触发 trap</td></tr>
    <tr><td><code>parse_u64(s: string): Result&lt;u64, NumberError&gt;</code></td><td>可选 <code>+</code> 加 ASCII 数字；<code>-</code> 视为非法字符</td></tr>
  </tbody>
</table>
<pre><code>use std.text;
use std.text.Builder;
use std.text.lines;

fn scan(input: string): usize {
    var total = 0_usize;
    for line in lines(input.bytes()) {
        total += line.len;
    }

    var buffer = Builder::init();
    defer buffer.deinit();
    buffer.append("scan:");
    buffer.append_bytes(input.bytes());
    return total + buffer.len();
}</code></pre>
<pre><code>val parsed = text.parse_i64("-42");
val label = match parsed {
    Result.Ok(value) =&gt; "ok",
    Result.Err(error) =&gt; "invalid",
};</code></pre>
<div class="callout">
  <p><strong>视图与拥有者：</strong><code>lines</code> 产出源字节上的借用视图；<code>Builder.view()</code> 同样是借用视图，在下一次 <code>append</code>、<code>consume</code>、<code>clear</code> 或 <code>deinit</code> 后失效，使用期内应复制或用 <code>from_utf8</code> 校验。</p>
</div>

<h2>5. 与字节视图互操作</h2>
<pre><code>val text_bytes = "ok".bytes();              // []const u8
val parsed = text.from_utf8(text_bytes);
val is_ok = match parsed {
    Result.Ok(value) =&gt; true,
    Result.Err(error) =&gt; false,
};</code></pre>
`
    },


    {
      id: "std/io",
      title: "进程、流与文件",
      body: `
<h1>进程、流与文件</h1>
<p>M19 新增进程、字节流、文件与错误模块。它们用 <code>Result</code> 报告失败而不是 trap，也不会隐式关闭任何资源：自有文件句柄必须显式关闭或通过 <code>defer</code> 关闭。</p>

<h2>1. 参数与环境</h2>
<table>
  <thead><tr><th>API</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>std.process.arg_count(): usize</code></td><td>进程参数个数，包含下标 0 的程序名</td></tr>
    <tr><td><code>std.process.arg(index: usize): Result&lt;string, ArgError&gt;</code></td><td>借用参数视图；失败返回 <code>OutOfRange</code> 或 <code>NotUtf8</code></td></tr>
    <tr><td><code>std.process.program_name(): Result&lt;string, ArgError&gt;</code></td><td>等价于 <code>arg(0)</code></td></tr>
    <tr><td><code>std.process.env(name: string): EnvLookup</code></td><td><code>Found(value)</code>、<code>Missing</code> 或 <code>NotUtf8</code></td></tr>
  </tbody>
</table>
<pre><code>use std.process.arg;
use std.process.arg_count;
use std.process.env;
use std.process.EnvLookup;

fn show_arguments() {
    var index = 1_usize;
    while index &lt; arg_count() {
        val item = arg(index);
        if item.is_ok() {
            val text = match item {
                Result.Ok(value) =&gt; value,
                Result.Err(error) =&gt; "",
            };
            println("arg {} = {}", index, text);
        }
        index += 1_usize;
    }

    val home = env("HOME");
    val label = match home {
        EnvLookup.Found(value) =&gt; value,
        EnvLookup.Missing =&gt; "missing",
        EnvLookup.NotUtf8 =&gt; "not utf8",
    };
    println("HOME = {}", label);
}</code></pre>
<p>返回的字符串都是借用视图，有效到进程结束；不要释放或写入。入口 <code>main</code> 仍不接受参数，应用参数用 <code>dc run . -- &lt;参数&gt;</code> 传入，或直接运行可执行文件。</p>

<h2>2. 错误</h2>
<pre><code>pub enum ErrorKind {
    Other,
    NotFound,
    PermissionDenied,
    IsADirectory,
    InvalidArgument,
    NotOwned,
    Closed,
}

pub struct Error { kind: ErrorKind, code: i32 }</code></pre>
<p>可用 <code>Error::new</code>、<code>kind()</code>、<code>code()</code> 与 <code>from_last_error()</code>；<code>code</code> 保留原生 errno/GetLastError。错误是普通值，不需要释放。</p>

<h2>3. 流</h2>
<table>
  <thead><tr><th>API</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>stdin()</code>、<code>stdout()</code>、<code>stderr()</code></td><td>借用标准流句柄；<code>close</code> 返回 <code>Err(NotOwned)</code></td></tr>
    <tr><td><code>read(buffer: []u8): Result&lt;usize, Error&gt;</code></td><td>至多读 <code>buffer.len</code> 字节；<code>Ok(0)</code> 表示 EOF，短读是正常结果</td></tr>
    <tr><td><code>write(bytes): Result&lt;usize, Error&gt;</code></td><td>返回实际写入字节数</td></tr>
    <tr><td><code>write_all(bytes): Result&lt;bool, Error&gt;</code></td><td>循环写完全部字节；返回 <code>Err</code> 时可能已写入前缀</td></tr>
    <tr><td><code>flush</code>、<code>is_open</code>、<code>close</code>、<code>close_abort</code></td><td><code>close</code> 幂等；<code>close_abort</code> 是 <code>defer</code> 清理入口</td></tr>
    <tr><td><code>release()</code>、<code>from_raw(id)</code></td><td>把自有句柄转交给另一个 <code>Stream</code>，不触发关闭</td></tr>
    <tr><td><code>eprint(bytes)</code></td><td>把字节写到 stderr</td></tr>
  </tbody>
</table>
<pre><code>use std.io.stdin;
use std.io.stdout;
use std.mem;
use std.test.expect;

fn copy_one_chunk() {
    val input = stdin();
    val output = stdout();
    val buffer = mem.alloc&lt;u8&gt;(4096_usize);
    defer mem.free&lt;u8&gt;(buffer);

    val read = input.read(buffer);
    if read.is_err() {
        return;
    }
    val count = match read {
        Result.Ok(value) =&gt; value,
        Result.Err(error) =&gt; 0_usize,
    };
    if count &gt; 0_usize {
        val chunk = buffer.slice(0_usize, count);
        val written = output.write_all(chunk);
        expect(written.is_ok());
    }
}</code></pre>
<p>标准句柄是借用资源，应用不应关闭它们。运行时会内部重试 <code>EINTR</code>，不把 <code>Interrupted</code> 暴露为错误类别。</p>

<h2>4. 文件</h2>
<p><code>std.fs.open(path, mode)</code> 支持 <code>OpenMode.Read</code>、<code>OpenMode.Write</code>（创建或截断）与 <code>OpenMode.Append</code>（创建或追加），返回自有 <code>Stream</code>。含内部 NUL 的路径在调用系统接口前被拒绝，文件不存在返回 <code>NotFound</code>。</p>
<pre><code>use std.fs.open;
use std.fs.OpenMode;
use std.io.stdin;
use std.mem;

fn first_byte(path: string): i32 {
    val opened = open(path, OpenMode.Read);
    if opened.is_err() {
        return -1_i32;
    }
    var stream = match opened {
        Result.Ok(value) =&gt; value,
        Result.Err(error) =&gt; stdin(),
    };
    defer stream.close_abort();

    val buffer = mem.alloc&lt;u8&gt;(1_usize);
    defer mem.free&lt;u8&gt;(buffer);
    val result = stream.read(buffer);
    if result.is_err() {
        return -2_i32;
    }
    val count = match result {
        Result.Ok(value) =&gt; value,
        Result.Err(error) =&gt; 0_usize,
    };
    if count == 0_usize {
        return 0_i32;
    }
    return buffer[0] as i32;
}</code></pre>
<p>自有句柄是单所有者资源：浅复制共享同一运行时状态，只应关闭其中一份。重绑定持有自有句柄的变量前，先 <code>close</code> 或 <code>release</code>；Debug 构建会在退出时报告仍未关闭的句柄。</p>

<h2>5. 测试</h2>
<p><code>std.test.expect(condition)</code> 与 <code>std.test.fail()</code> 支撑 <code>dc test</code>；完整流程见<a href="#/tutorial/testing">测试代码</a>。</p>
`
    },

    {
      id: "std/ffi",
      title: "std.ffi C 字符串",
      body: `
<h1>std.ffi C 字符串</h1>
<p><code>std.ffi</code> 提供拥有型、NUL 结尾的 C 字符串 <code>CString</code>，用于把 Dolphin 字符串传给 C 函数。</p>
<pre><code>use std.ffi.CString;</code></pre>

<h2>1. 类型</h2>
<pre><code>pub enum CStringError {
    InteriorNul,
}

pub struct CString {
    bytes: []u8,
}</code></pre>
<p><code>CString</code> 拥有自己的缓冲，必须以 <code>deinit</code> 释放。<code>ptr()</code> 返回的指针只在 <code>CString</code> 存活期间有效。</p>

<h2>2. API</h2>
<table>
  <thead><tr><th>签名</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>CString::empty(): CString</code></td><td>创建只包含结尾 NUL 的空 C 字符串</td></tr>
    <tr><td><code>CString::from(s: string): Result&lt;CString, CStringError&gt;</code></td><td>复制内容并追加 NUL；若 <code>s</code> 含内部 NUL 则返回 <code>InteriorNul</code></td></tr>
    <tr><td><code>ptr(self: *const Self): *const c_char</code></td><td>返回底层 NUL 结尾缓冲的指针，交给 C 函数</td></tr>
    <tr><td><code>deinit(self: *Self)</code></td><td>释放底层缓冲</td></tr>
  </tbody>
</table>

<h2>3. 示例</h2>
<pre><code>use std.ffi.CString;

extern "C" {
    pub fn strlen(text: *const c_char): usize;
}

fn main() {
    val owned = CString::from("dolphin");
    var result = match owned {
        Result.Ok(value) =&gt; value,
        Result.Err(error) =&gt; CString::empty(),
    };
    defer result.deinit();

    println("{}", strlen(result.ptr()));
}</code></pre>
<div class="callout warn">
  <p><strong>生命周期：</strong><code>CString</code> 释放后 <code>ptr()</code> 返回的指针即悬垂。请确保 C 函数不会在 <code>deinit</code> 之后继续持有该指针。</p>
</div>

<h2>4. 处理内部 NUL</h2>
<pre><code>val bad = CString::from("a\\u{0}b");
var handled = match bad {
    Result.Ok(value) =&gt; value,
    Result.Err(error) =&gt; CString::empty(),
};
defer handled.deinit();</code></pre>

<h2>5. 相关章节</h2>
<ul>
  <li><a href="#/tutorial/memory">内存模型与 C 互操作</a>：<code>extern "C"</code>、<code>c_char</code> 等平台别名与原生链接。</li>
  <li><a href="#/std/mem">std.mem</a>：<code>mem.view</code>、<code>mem.cast_const_ptr</code> 等指针转换。</li>
</ul>
`
    },

    {
      id: "std/builtins",
      title: "内建函数与格式化",
      body: `
<h1>内建函数与格式化</h1>
<p>以下名称由编译器直接提供，不来自源码标准库，也不能被用户定义覆盖。</p>

<h2>1. 输出</h2>
<table>
  <thead><tr><th>函数</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>print(...)</code></td><td>输出后不换行；无参数时不输出内容</td></tr>
    <tr><td><code>println(...)</code></td><td>输出后追加换行；无参数时只输出换行</td></tr>
  </tbody>
</table>
<p>二者只能作为独立语句调用。第一个参数当前必须是字符串字面量。</p>
<pre><code>print("Hello, ");
println("{}!", "Dolphin");
println();                 // 仅换行
println("{} + {} = {}", 1, 2, 3);</code></pre>

<h2>2. 占位符与花括号</h2>
<p>每个 <code>{}</code> 消耗一个格式化参数，按从左到右的顺序求值。参数数量与占位符数量必须一致，否则产生编译错误。</p>
<pre><code>println("{} {}", 1);      // 编译错误：缺少参数
println("{}", 1, 2);      // 编译错误：参数过多
println("{{}}");          // 输出 {}</code></pre>
<p>当前支持格式化全部整数、浮点数、<code>char</code>、<code>bool</code> 与 <code>string</code>。数组整体不能直接格式化，但其元素可以。</p>
<pre><code>val samples = [10, 20, 30];
println("first = {}", samples[0]);</code></pre>

<h2>3. 字符串内建</h2>
<table>
  <thead><tr><th>名称</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>length(s: string): usize</code></td><td>UTF-8 字节长度</td></tr>
    <tr><td><code>s.bytes(): []const u8</code></td><td>只读字节视图，零分配</td></tr>
    <tr><td><code>string.from_bytes(bytes: []const u8): string</code></td><td>校验 UTF-8 后返回 <code>string</code> 视图；非法 UTF-8 以 <code>104</code> 终止</td></tr>
  </tbody>
</table>
<pre><code>val greeting = "海豚";
println("bytes = {}", length(greeting));
val again = string.from_bytes(greeting.bytes());
println("{}", again == greeting);</code></pre>
<p>字符串使用 <code>==</code> / <code>!=</code> 按字节内容比较；内建 <code>string</code> 不支持 <code>+</code> 与下标，拼接新字符串请用 <code>std.text.concat</code> 或 <code>std.text.Builder</code>。</p>

<h2>4. 运算符与转换</h2>
<table>
  <thead><tr><th>类别</th><th>运算符</th></tr></thead>
  <tbody>
    <tr><td>算术</td><td><code>+</code> <code>-</code> <code>*</code> <code>/</code> <code>%</code>（<code>%</code> 仅整数）</td></tr>
    <tr><td>比较</td><td><code>&lt;</code> <code>&lt;=</code> <code>&gt;</code> <code>&gt;=</code>（全部整数、浮点与 <code>char</code>）</td></tr>
    <tr><td>相等</td><td><code>==</code> <code>!=</code>（<code>i32</code>、<code>bool</code>、<code>string</code> 等）</td></tr>
    <tr><td>逻辑</td><td><code>!</code> <code>&amp;&amp;</code> <code>||</code>（短路求值）</td></tr>
    <tr><td>转换</td><td><code>as</code>（窄化保留低位，扩展按符号/零扩展，浮点转整数饱和）</td></tr>
  </tbody>
</table>
<p>无后缀整数字面量默认 <code>i32</code>，无后缀浮点字面量默认 <code>f64</code>。字面量后缀必须使用下划线，例如 <code>10_i64</code>、<code>1.5_f32</code>。</p>

<h2>5. 运行时错误与诊断</h2>
<p>以下情况会安全终止程序，而不是产生未定义行为：</p>
<table>
  <thead><tr><th>退出码</th><th>情况</th></tr></thead>
  <tbody>
    <tr><td><code>101</code></td><td>整数溢出、除以零、取模零、数组/切片越界、<code>null</code> 视图或对齐检查失败</td></tr>
    <tr><td><code>102</code></td><td>分配失败或分配尺寸溢出</td></tr>
    <tr><td><code>103</code></td><td>无效释放（Debug 检测）</td></tr>
    <tr><td><code>104</code></td><td>UTF-8 校验失败</td></tr>
    <tr><td><code>106</code></td><td><code>std.test.expect(false)</code> 或 <code>fail()</code> 的测试断言失败</td></tr>
  </tbody>
</table>
<p>编译诊断包含稳定类别 <code>E0000</code> / <code>E0001</code>、文件路径、Unicode 字符列号与多行源码标记。CLI 支持 <code>--color auto|always|never</code>。当前编译器通常在第一个错误处停止。</p>

<h2>6. 平台相关别名</h2>
<p>与 C 互操作时可用的内建别名：<code>c_int</code>、<code>c_uint</code>、<code>c_long</code>、<code>c_ulong</code>、<code>c_char</code>。它们随目标平台变化，例如 Windows 上 <code>c_long</code> 为 32 位。</p>
`
    }
  ]
});
