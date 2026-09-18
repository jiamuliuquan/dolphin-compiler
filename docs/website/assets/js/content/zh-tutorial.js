window.DolphinDocsContent = window.DolphinDocsContent || {};
window.DolphinDocsContent["zh-CN"] = window.DolphinDocsContent["zh-CN"] || { groups: [] };
window.DolphinDocsContent["zh-CN"].groups.push({
  id: "tutorial",
  title: "从入门到精通",
  pages: [
    {
      id: "tutorial/install",
      title: "安装与第一个程序",
      body: `
<h1>安装与第一个程序</h1>
<p>本章带你完成 Dolphin 工具链的安装，并编译运行第一个程序。Dolphin 以自包含发行包发布，解压即用，构建程序时不需要 C/C++ 工具链，也不需要 Rust。</p>

<h2>1. 安装编译器</h2>
<p>发行包提供三个一级平台的归档：Linux x86_64、macOS ARM64 与 Windows x86_64。下载后解压到任意目录，并把该目录加入 <code>PATH</code>：</p>
<pre><code>mkdir -p ~/.local/dolphin
tar xzf dolphin-0.1.0-x86_64-unknown-linux-gnu.tar.gz -C ~/.local/dolphin
export PATH="$HOME/.local/dolphin:$PATH"</code></pre>
<p>解压目录中的 <code>dc</code> 是编译器主命令，<code>rust-lld</code> 是随包分发的链接器，二者必须位于同一目录。完整的平台说明、校验和与卸载方式见 <a href="../install.html">安装指南</a>。</p>
<p>验证安装：</p>
<pre><code>dc --version
dc env</code></pre>
<p><code>dc env</code> 会显示宿主/目标平台、ABI、所选链接器与缓存根。若命令找不到，请检查 <code>PATH</code> 是否指向解压目录。</p>

<h2>2. 第一个程序</h2>
<p>创建文件 <code>hello.do</code>：</p>
<pre><code>fn main() {
    println("Hello, Dolphin!");
}</code></pre>
<p>直接编译单个文件而不使用模块和导入：</p>
<pre><code>dc build hello.do -o hello
./hello</code></pre>
<p>输出：</p>
<pre><code>Hello, Dolphin!</code></pre>
<p>也可以使用统一的项目结构。每个项目以 <code>src/</code> 作为源码根目录，入口函数 <code>main</code> 必须定义在 <code>src</code> 根目录的某个文件中：</p>
<pre><code>hello-project/
└── src/
    └── main.do</code></pre>
<pre><code>cd hello-project
dc check .
dc build .
dc run .</code></pre>
<p>构建产物位于项目的 <code>target/</code> 目录：</p>
<pre><code>target/hello-project             本机可执行文件
target/hello-project.o           Dolphin 程序目标文件
target/hello-project.runtime.o   最小运行时目标文件</code></pre>

<h2>3. 程序入口与退出码</h2>
<p>可执行程序必须且只能定义一个 <code>main</code>。当前 <code>main</code> 不接受命令行参数，也可以返回 <code>i32</code> 作为进程退出码：</p>
<pre><code>fn main(): i32 {
    return 42;
}</code></pre>
<pre><code>dc build . && ./target/hello-project
echo $?   # 42</code></pre>
<p>自然执行结束或执行 <code>return;</code> 时，退出码为 <code>0</code>。有返回类型的函数必须保证所有可达路径都返回值。</p>

<h2>4. 常用命令</h2>
<table>
  <thead><tr><th>命令</th><th>作用</th></tr></thead>
  <tbody>
    <tr><td><code>dc check &lt;项目&gt;</code></td><td>只做词法、语法、类型与名称检查，不生成产物</td></tr>
    <tr><td><code>dc build &lt;项目&gt;</code></td><td>编译并链接为可执行文件（默认 Debug）</td></tr>
    <tr><td><code>dc run &lt;项目&gt;</code></td><td>构建后立即运行，并透传程序退出码</td></tr>
    <tr><td><code>dc info &lt;项目&gt;</code></td><td>显示包坐标、目标、依赖与锁文件状态</td></tr>
    <tr><td><code>dc env</code></td><td>显示宿主/目标平台与工具链信息</td></tr>
  </tbody>
</table>
<p><code>--debug</code> 与 <code>--release</code> 互斥，默认使用 Debug。Release 会开启更多优化，产物更小、运行更快。</p>

<h2>5. 下一步</h2>
<p>你已经能够编译并运行 Dolphin 程序。继续阅读<a href="#/tutorial/intro">语言概览</a>了解语法全貌，或直接跳到<a href="#/tutorial/types">变量、类型与表达式</a>开始系统学习。</p>
`
    },

    {
      id: "tutorial/intro",
      title: "语言概览",
      body: `
<h1>语言概览</h1>
<p>Dolphin 是一门静态类型语言，语法参考 Rust、Kotlin、Java 和 C，可编译为操作系统可直接运行的本机可执行文件。它优先保证规则简单、行为明确、编译器易于维护。</p>

<h2>1. 设计目标</h2>
<ul>
  <li>语法简洁，常用代码容易阅读。</li>
  <li>静态类型，并支持局部变量类型推断。</li>
  <li>默认提供明确、安全的行为，尽量在编译期发现错误。</li>
  <li>支持函数、数组、控制流、模块、泛型与基础标准库。</li>
  <li>编译为本机目标文件，并链接为可直接运行的二进制。</li>
  <li>编译器结构清晰，方便逐步增加语言特性和新后端。</li>
</ul>
<p>Dolphin 采用 Zig 式显式内存模型：没有垃圾回收、引用计数、隐式析构和借用检查；值默认复制，指针与切片只复制描述符，分配与释放显式可见。</p>

<h2>2. 源文件与注释</h2>
<p>源文件使用 UTF-8 编码，扩展名为 <code>.do</code>。支持单行注释与多行注释：</p>
<pre><code>// 单行注释

/*
 * 多行注释（暂不支持嵌套）
 */</code></pre>
<p>标识符由 ASCII 字母或下划线开头，后续可以包含字母、数字和下划线：<code>value</code>、<code>user_name</code>、<code>value2</code>、<code>_internal</code>。关键字不能作为标识符，当前不支持 Unicode 标识符。</p>

<h2>3. 一段完整代码</h2>
<pre><code>struct Point {
    x: i32,
    y: i32,
}

fn distance_squared(a: Point, b: Point): i32 {
    val dx = a.x - b.x;
    val dy = a.y - b.y;
    return dx * dx + dy * dy;
}

fn main() {
    val origin = Point(0, 0);
    val target = Point(3, 4);
    println("distance^2 = {}", distance_squared(origin, target));
}</code></pre>
<p>这段代码展示了结构体、函数、局部类型推断、字段访问与格式化输出——它们会在后续章节逐一展开。</p>

<h2>4. 编译流程</h2>
<pre><code>UTF-8 源文件
  -&gt; Token
  -&gt; AST
  -&gt; 模块与名称解析
  -&gt; 类型和控制流检查
  -&gt; 类型化 CFG IR
  -&gt; Cranelift IR
  -&gt; 本机目标文件
  -&gt; 内嵌运行时与 rust-lld 链接器
  -&gt; 本机可执行文件</code></pre>
<p>Dolphin 程序在 Linux 上动态链接 glibc、在 macOS 上动态链接 libSystem、在 Windows 上动态链接 UCRT，这些都是操作系统自带组件。</p>

<h2>5. 模块系统一瞥</h2>
<p>每个项目以 <code>src/</code> 作为源码根目录。直接位于 <code>src/</code> 下的文件属于根模块，可以省略 <code>pkg</code>；子目录中的文件必须声明所在目录作为包名（文件名不参与）：</p>
<pre><code>// src/mathutil/math.do
pkg mathutil;

pub fn min(a: i32, b: i32): i32 {
    if a &lt; b {
        return a;
    }
    return b;
}</code></pre>
<pre><code>// src/main.do
use mathutil.math;

fn main() {
    println("min = {}", math.min(8, 3));
}</code></pre>
<p>顶层声明默认仅在本模块内可见，添加 <code>pub</code> 后可以跨模块访问。详见<a href="#/tutorial/packages">模块、可见性与包管理</a>。</p>

<h2>6. 当前边界</h2>
<p>Dolphin 仍在演进中。以下能力尚未实现：嵌套数组与空数组字面量、资源 <code>try</code> 语法、借用检查、生命周期、闭包、动态分派、<code>?</code> 错误传播、文件/进程/网络标准库、C 头文件导入、交叉编译与 DWARF 调试信息。请在编写代码前确认对应能力是否可用。</p>

<h2>7. 继续学习</h2>
<ul>
  <li><a href="#/tutorial/types">变量、类型与表达式</a></li>
  <li><a href="#/tutorial/control">控制流与函数</a></li>
  <li><a href="#/tutorial/composites">数组、结构体、枚举与 match</a></li>
  <li><a href="#/tutorial/generics">泛型、trait 与标准库</a></li>
  <li><a href="#/tutorial/memory">内存模型与 C 互操作</a></li>
</ul>
`
    },

    {
      id: "tutorial/types",
      title: "变量、类型与表达式",
      body: `
<h1>变量、类型与表达式</h1>
<p>本章介绍 Dolphin 的基础标量类型、变量绑定、类型转换与运算符。</p>

<h2>1. 变量绑定</h2>
<p>使用 <code>var</code> 定义可变变量，使用 <code>val</code> 定义不可变变量。所有变量必须在声明时初始化，类型可以由初始化表达式推断：</p>
<pre><code>var count = 1;
count = 2;

val limit = 10;
// limit = 20;  // 编译错误：不能给 val 赋值</code></pre>
<pre><code>var count: i32 = 1;      // 显式类型
val name = "Dolphin";    // 推断为 string</code></pre>
<p>同一作用域内不能重复定义同名变量；内层作用域可以遮蔽外层变量。函数参数视为不可变局部变量。</p>
<div class="callout">
  <p><strong>提示：</strong>应优先使用 <code>val</code>，只有在确实需要重新赋值时才使用 <code>var</code>。这能让编译器帮你捕捉意外修改。</p>
</div>

<h2>2. 整数类型</h2>
<p>支持有符号 <code>i8</code>、<code>i16</code>、<code>i32</code>、<code>i64</code> 与无符号 <code>u8</code>、<code>u16</code>、<code>u32</code>、<code>u64</code>，以及指针宽度的 <code>usize</code> / <code>isize</code>。没有后缀的整数字面量默认是 <code>i32</code>：</p>
<pre><code>val count = 10;            // i32
val small: i8 = -8_i8;
val medium = 1600_i16;
val large: i64 = 64000_i64;
val capacity: usize = 100_usize;</code></pre>
<table>
  <thead><tr><th>类型</th><th>范围</th></tr></thead>
  <tbody>
    <tr><td><code>i8</code></td><td>-128 到 127</td></tr>
    <tr><td><code>i32</code></td><td>-2147483648 到 2147483647</td></tr>
    <tr><td><code>i64</code></td><td>-9223372036854775808 到 9223372036854775807</td></tr>
    <tr><td><code>u8</code></td><td>0 到 255</td></tr>
    <tr><td><code>u64</code></td><td>0 到 18446744073709551615</td></tr>
    <tr><td><code>usize</code> / <code>isize</code></td><td>目标指针宽度（当前为 64 位）</td></tr>
  </tbody>
</table>
<p>不同数值类型不会隐式混合，也不会进行可能丢失数据的窄化。必须使用 <code>as</code> 显式转换：</p>
<pre><code>val large: i64 = 64000_i64;
// val small: i32 = large;         // 编译错误
val small: i32 = large as i32;     // 显式转换
val back: i64 = small as i64;      // 扩展转换按符号扩展</code></pre>
<p>运行时整数加、减、乘、取负执行溢出检查，除以零与取模零会以统一运行时错误终止程序；窄化转换保留低位，扩展转换按来源类型做符号或零扩展。</p>

<h2>3. 浮点数</h2>
<p>支持 IEEE <code>f32</code> 与 <code>f64</code>，无后缀浮点字面量默认是 <code>f64</code>：</p>
<pre><code>val ratio = 0.5;          // f64
val precise = 2.25_f64;   // f64
val single: f32 = 1.5_f32;</code></pre>
<p>浮点支持 <code>+</code>、<code>-</code>、<code>*</code>、<code>/</code>、比较与相等，不支持 <code>%</code>。浮点除零遵循 IEEE 规则。浮点到整数的 <code>as</code> 转换使用饱和语义。</p>

<h2>4. 布尔与字符</h2>
<pre><code>val enabled: bool = true;
val disabled = false;

val symbol: char = '海';
val latin: char = 'D';</code></pre>
<p>条件表达式必须是 <code>bool</code>，整数不能隐式作为条件。<code>char</code> 表示一个 Unicode 标量值，支持比较、相等、数组、函数调用、格式化，以及与整数之间的显式转换。</p>

<h2>5. 字符串</h2>
<p><code>string</code> 是不可变的 UTF-8 字符串视图，按字节内容比较：</p>
<pre><code>val greeting = "Hello, 世界!";
println("{}", greeting == "Hello, 世界!");  // true
println("bytes = {}", length(greeting));     // UTF-8 字节数</code></pre>
<p>字符串字面量支持以下转义：</p>
<pre><code>\\n        换行
\\r        回车
\\t        制表符
\\\\        反斜杠
\\"        双引号
\\u{1F600} Unicode 码点</code></pre>
<p><code>length(s)</code> 返回 UTF-8 字节数。<code>s.bytes()</code> 零分配返回只读字节视图 <code>[]const u8</code>，<code>string.from_bytes(bytes)</code> 校验 UTF-8 后返回 <code>string</code> 视图；非法 UTF-8 会以 <code>104</code> 终止程序。字符串不支持 <code>+</code> 拼接，需要拼接时请使用 <code>std.text.concat</code>。</p>

<h2>6. Unit</h2>
<p>省略返回类型的普通函数内部视为返回 <code>Unit</code>。<code>Unit</code> 不能保存到变量、参与运算或格式化输出：</p>
<pre><code>fn do_nothing() {
    return;
}</code></pre>

<h2>7. 运算符与优先级</h2>
<table>
  <thead><tr><th>类别</th><th>运算符</th></tr></thead>
  <tbody>
    <tr><td>算术</td><td><code>+</code> <code>-</code> <code>*</code> <code>/</code> <code>%</code></td></tr>
    <tr><td>比较</td><td><code>&lt;</code> <code>&lt;=</code> <code>&gt;</code> <code>&gt;=</code></td></tr>
    <tr><td>相等</td><td><code>==</code> <code>!=</code></td></tr>
    <tr><td>逻辑</td><td><code>!</code> <code>&amp;&amp;</code> <code>||</code></td></tr>
    <tr><td>赋值</td><td><code>=</code> <code>+=</code> <code>-=</code> <code>*=</code> <code>/=</code> <code>%=</code></td></tr>
    <tr><td>转换</td><td><code>as</code></td></tr>
  </tbody>
</table>
<p>优先级从高到低：调用/下标/括号、一元 <code>-</code> <code>!</code>、<code>* / %</code>、<code>+ -</code>、比较、相等、<code>&amp;&amp;</code>、<code>||</code>。赋值只作为语句存在，当前不是可嵌套的表达式。</p>
<p><code>&amp;&amp;</code> 和 <code>||</code> 使用短路求值：</p>
<pre><code>val safe = false &amp;&amp; 1 / 0 == 0;  // 不执行右侧除零表达式</code></pre>

<h2>8. 输出与格式化</h2>
<pre><code>print("Hello, ");
println("{}!", "Dolphin");
println("{} + {} = {}", 1, 2, 3);
println("{{}}");   // 输出 {}</code></pre>
<p>每个 <code>{}</code> 消耗一个格式化参数，支持全部整数、浮点数、<code>char</code>、<code>bool</code> 与 <code>string</code>。占位符数量与参数数量不一致会产生编译错误。第一个参数当前必须是字符串字面量。</p>

<h2>练习</h2>
<ol>
  <li>定义三个不同类型的整数并互相转换，观察编译错误与运行时行为。</li>
  <li>使用 <code>length</code> 与 <code>bytes</code> 计算一个中文字符串的字节数，并解释它与字符数的区别。</li>
  <li>用短路逻辑写一个不会除零的表达式。</li>
</ol>
`
    },

    {
      id: "tutorial/control",
      title: "控制流与函数",
      body: `
<h1>控制流与函数</h1>
<p>本章介绍条件、循环、函数定义与格式化输出，它们是组织程序逻辑的基础。</p>

<h2>1. if / else</h2>
<p>条件不强制添加括号，条件表达式必须是 <code>bool</code>：</p>
<pre><code>if score &gt;= 60 {
    println("passed");
} else {
    println("failed");
}</code></pre>
<p>当前 <code>if</code> 是语句，不产生值，也尚不支持 <code>else if</code> 简写；需要多分支时可以嵌套 <code>if</code>，或改用 <code>match</code>。</p>

<h2>2. while 与 loop</h2>
<pre><code>var i = 0;
while i &lt; 5 {
    println("i = {}", i);
    i += 1;
}</code></pre>
<pre><code>var i = 0;
loop {
    if i &gt;= 5 {
        break;
    }
    println("i = {}", i);
    i += 1;
}</code></pre>
<p><code>loop</code> 创建无限循环，通过 <code>break</code> 退出。</p>

<h2>3. for 与范围</h2>
<p>半开范围 <code>start..end</code> 不包含结束值，闭合范围 <code>start..=end</code> 包含结束值：</p>
<pre><code>for i in 0..3 {    // 0、1、2
    println("{}", i);
}

for i in 0..=3 {   // 0、1、2、3
    println("{}", i);
}</code></pre>
<p><code>for</code> 也可以直接遍历数组：</p>
<pre><code>val numbers = [1, 2, 3];
for number in numbers {
    println("{}", number);
}</code></pre>
<p><code>for x in expr</code> 要求 <code>expr</code> 实现 <code>Iterator</code> 协议；数组与切片会被自动适配，范围会降低为一个 <code>Range</code> 迭代器。循环变量不可赋值，被遍历的表达式只求值一次。<code>break</code> 与 <code>continue</code> 只能出现在循环中。</p>

<h2>4. 函数</h2>
<p>参数必须显式标注类型，返回值函数必须显式标注返回类型：</p>
<pre><code>fn add(a: i32, b: i32): i32 {
    return a + b;
}

fn greet(name: string) {
    println("Hello, {}!", name);
}</code></pre>
<p>函数不需要先于调用位置定义，支持前向调用与递归：</p>
<pre><code>fn factorial(n: i32): i32 {
    if n &lt;= 1 {
        return 1;
    }
    return n * factorial(n - 1);
}</code></pre>
<p>有返回类型的函数必须保证所有可达路径都返回正确类型的值：</p>
<pre><code>fn max(a: i32, b: i32): i32 {
    if a &gt; b {
        return a;
    }
    return b;
}</code></pre>

<h2>5. main 入口</h2>
<pre><code>fn main() {
    println("Hello");
}</code></pre>
<pre><code>fn main(): i32 {
    return 0;
}</code></pre>
<p>程序必须且只能定义一个 <code>main</code>。当前 <code>main</code> 不接受参数；省略返回类型时按进程退出码处理，自然结束或 <code>return;</code> 的退出码为 <code>0</code>，<code>return expr;</code> 的表达式必须是 <code>i32</code>。</p>

<h2>6. 控制流检查</h2>
<p>编译器会拒绝确定不可达的语句，也会拒绝循环外的 <code>break</code> / <code>continue</code>。这些检查让错误在编译期暴露，而不是留到运行时。</p>

<h2>完整示例：素数筛</h2>
<pre><code>fn is_prime(n: i32): bool {
    if n &lt; 2 {
        return false;
    }
    var divisor = 2;
    while divisor * divisor &lt;= n {
        if n % divisor == 0 {
            return false;
        }
        divisor += 1;
    }
    return true;
}

fn main() {
    var count = 0;
    for candidate in 2..50 {
        if is_prime(candidate) {
            print("{} ", candidate);
            count += 1;
        }
    }
    println("");
    println("found {} primes", count);
}</code></pre>

<h2>练习</h2>
<ol>
  <li>用递归实现斐波那契数列，并与循环版本比较写法。</li>
  <li>编写函数判断一个整数是否为完全数。</li>
  <li>用 <code>for</code> 与 <code>break</code> 实现查找数组中第一个偶数并返回其下标。</li>
</ol>
`
    },

    {
      id: "tutorial/composites",
      title: "数组、结构体、枚举与 match",
      body: `
<h1>数组、结构体、枚举与 match</h1>
<p>本章介绍如何用数组、结构体和枚举表达复合数据，并用 <code>match</code> 对枚举做穷尽式分派。</p>

<h2>1. 定长数组</h2>
<p>数组类型写作 <code>[元素类型; 长度]</code>，长度是类型的一部分：</p>
<pre><code>val inferred = [1, 2, 3];                 // [i32; 3]
val explicit: [i32; 3] = [1, 2, 3];
val repeated = [false; 4];                // [bool; 4]</code></pre>
<p>下标从 0 开始，可以是常量或运行时变量：</p>
<pre><code>var values = [10, 20, 30];
val first = values[0];
values[1] = 25;
values[2] += 5;</code></pre>
<p>编译器会拒绝确定越界的常量下标；动态下标在运行时检查，越界会触发陷阱并终止程序。当前只支持一维、非空数组，元素可以是任意已实现的非 <code>Unit</code> 标量类型；<code>[i32; 2]</code> 与 <code>[i32; 3]</code> 是不同类型。数组采用按值语义，作为参数或返回值时会完整复制。</p>

<h2>2. 结构体</h2>
<p>结构体把字段聚合为一个值类型，采用位置构造与字段访问：</p>
<pre><code>struct Point {
    x: i32,
    y: i32,
}

fn main() {
    var p = Point(3, 4);
    println("point = ({}, {})", p.x, p.y);
    p.x = 10;             // var 结构体的字段可以单独写入
    println("x = {}", p.x);
}</code></pre>
<p>字段在结构体内不得重名，访问不存在的字段是编译期错误。字段默认模块私有，跨模块读/写或位置构造需要 <code>pub</code>：</p>
<pre><code>pub struct Pair {
    pub first: i32,
    pub second: i32,
}</code></pre>
<p>结构体字段支持基础类型、数组、其它结构体以及泛型参数。结构体自引用必须经过指针，按值布局形成环会在编译期报错。</p>

<h2>3. 枚举</h2>
<p>枚举描述一组有限的取值，枚举项可以携带数据：</p>
<pre><code>enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
    Empty,
}</code></pre>
<p>枚举项通过 <code>Shape.Circle(...)</code> 构造，无参枚举项写作 <code>Shape.Empty</code>（不带括号）。同一枚举内的枚举项名不得重复，携带参数的类型在声明处固定。</p>

<h2>4. match 模式匹配</h2>
<p><code>match</code> 是表达式式控制流，对枚举做穷尽匹配：</p>
<pre><code>fn area(shape: Shape): f64 {
    return match shape {
        Shape.Circle(r) =&gt; 3.14159 * r * r,
        Shape.Rectangle(w, h) =&gt; w * h,
        Shape.Empty =&gt; 0.0,
    };
}</code></pre>
<ul>
  <li>各分支必须求值为同一类型；<code>match</code> 在表达式位置使用（例如 <code>val area = match ...</code>）。</li>
  <li>缺失分支或冗余分支会产生源码级诊断。</li>
  <li>支持解构绑定，以及用 <code>_</code> 忽略单个字段或整体通配。</li>
</ul>
<pre><code>val label = match shape {
    Shape.Circle(_) =&gt; "circle",
    Shape.Rectangle(w, h) =&gt; "rect",
    _ =&gt; "other",
};</code></pre>
<div class="callout warn">
  <p><strong>当前限制：</strong><code>match</code> 不能作为独立语句，分支体也尚不支持语句块（<code>=&gt; { ... }</code>）；<code>if</code> 是语句而不是表达式；也不支持对整数、字符串等非枚举值做模式匹配。多语句分支逻辑请放进辅助函数。</p>
</div>

<h2>5. 跨模块使用类型</h2>
<pre><code>// src/geom/shapes.do
pkg geom;

pub struct Point {
    pub x: i32,
    pub y: i32,
}

pub enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
    Empty,
}</code></pre>
<pre><code>// src/main.do
use geom.shapes;

fn main() {
    val p = shapes.Point(3, 4);
    val circle = shapes.Shape.Circle(2.0);
    println("p = ({}, {})", p.x, p.y);
}</code></pre>
<p>顶层类型默认仅在本模块内可见，<code>pub struct</code> / <code>pub enum</code> 可以跨模块引用。详见<a href="#/tutorial/packages">模块与包管理</a>。</p>

<h2>6. 一个完整的领域模型</h2>
<pre><code>enum Token {
    Number(i32),
    Plus,
    Minus,
    Invalid,
}

fn step(token: Token, pending: i32): i32 {
    val value = match token {
        Token.Number(number) =&gt; number,
        Token.Plus =&gt; pending,
        Token.Minus =&gt; -pending,
        Token.Invalid =&gt; 0,
    };
    return value;
}

fn main() {
    var result = 0;
    var pending = 0;
    pending = step(Token.Number(5), pending);
    result += pending;
    pending = step(Token.Plus, pending);
    result += pending;
    pending = step(Token.Number(3), pending);
    result += pending;
    println("result = {}", result);
}</code></pre>

<h2>练习</h2>
<ol>
  <li>定义 <code>enum Color { Rgb(u8, u8, u8), Named(i32) }</code> 并用 <code>match</code> 返回其分量之和。</li>
  <li>定义带 <code>pub</code> 字段的结构体，跨模块构造并修改。</li>
  <li>用数组保存若干结构体，计算字段的聚合值。</li>
</ol>
`
    },

    {
      id: "tutorial/packages",
      title: "模块、可见性与包管理",
      body: `
<h1>模块、可见性与包管理</h1>
<p>本章介绍项目的源码组织、模块导入与可见性规则，以及 <code>dolphin.toml</code> 清单、依赖与库包发布。</p>

<h2>1. 源码根目录与 pkg</h2>
<p>每个项目以 <code>src/</code> 作为源码根目录。<code>src</code> 只用于组织项目，不属于模块名。直接位于 <code>src/</code> 下的 <code>.do</code> 文件组成根模块，可以省略 <code>pkg</code>：</p>
<pre><code>project/
└── src/
    ├── main.do
    ├── helper.do
    └── mathutil/
        └── math.do</code></pre>
<p>子目录文件必须在第一条有效语句声明 <code>pkg</code>，且与相对于 <code>src</code> 的目录一致；文件自身的模块名由目录加文件名共同决定：</p>
<pre><code>// src/mathutil/math.do
pkg mathutil;

pub fn min(a: i32, b: i32): i32 {
    if a &lt; b {
        return a;
    }
    return b;
}</code></pre>
<p>如果子目录文件省略 <code>pkg</code> 或目录不一致，编译器会报错。<code>std</code> 命名空间保留给标准库，用户模块不得占用。</p>

<h2>2. use 与 pub</h2>
<p>导入整个模块后，通过模块名访问公开成员；也可以精确导入成员：</p>
<pre><code>// src/main.do
use mathutil.math;         // 通过 math.min(...) 访问
// use mathutil.math.min;  // 或直接使用 min(...)
// use mathutil;           // 也可以导入包前缀，通过 mathutil.math.min(...) 访问

fn main() {
    println("min = {}", math.min(8, 3));
}</code></pre>
<p>顶层声明默认仅在本模块内可见，添加 <code>pub</code> 后可以跨模块访问。导入不存在或非公开的名称、重复导入以及未知模块都会产生编译错误。当前不支持通配符导入 <code>use std.*</code>、导入别名与重导出，但允许直接使用完整公开路径调用。</p>

<h2>3. dolphin.toml 清单</h2>
<p>当项目根目录存在 <code>dolphin.toml</code> 时，<code>dc check/build/run</code> 会从给定目录向上查找清单并以其驱动构建：</p>
<pre><code>[package]
group = "me.foxlab"
name = "greeter"
version = "0.1.0"
source = "src"

[lib]
path = "src/lib.do"

[[bin]]
name = "cli"
path = "src/main.do"

[repositories]
default = "https://packages.example.org/dolphin"

[dependencies]
math = "org.example:mathlib:1.0.0"
codec = { coordinate = "org.example:codec:2.0.0", repository = "default" }
local = { path = "../local-lib" }

[build]
optimization = "debug"
output = "target"</code></pre>
<table>
  <thead><tr><th>键</th><th>说明</th></tr></thead>
  <tbody>
    <tr><td><code>[package]</code></td><td><code>group:name:version</code> 组成包坐标，<code>source</code> 是源码根目录（默认 <code>src</code>）</td></tr>
    <tr><td><code>[lib]</code></td><td>声明库目标，<code>path</code> 必须是 <code>source</code> 的直接子文件；一个包最多一个 lib</td></tr>
    <tr><td><code>[[bin]]</code></td><td>声明一个或多个可执行目标，各自拥有独立的 <code>main</code></td></tr>
    <tr><td><code>[repositories]</code></td><td>仓库 ID 到基地址的映射（<code>http(s)://</code> 或 <code>file://</code>）</td></tr>
    <tr><td><code>[dependencies]</code></td><td>私有别名到精确坐标或 <code>{ path = ... }</code> 的映射</td></tr>
    <tr><td><code>[native.&lt;target&gt;]</code></td><td>声明预编译的 objects / static-libs / shared-libs / runtime-files</td></tr>
    <tr><td><code>[build]</code></td><td><code>optimization</code>（debug/release）与产物目录 <code>output</code></td></tr>
  </tbody>
</table>
<p>lib 与 <code>[[bin]]</code> 至少声明一种，可同时存在。多目标项目运行需用 <code>--bin</code> 选择目标，纯库项目的 <code>run</code> 会被明确拒绝。</p>

<h2>4. 依赖与库包</h2>
<p>依赖可以是本地 path，也可以是 Maven 风格坐标 <code>group:name:version</code>。库以确定性 <code>.dlib</code>（ZIP）分发，包含规范化清单与完整库源码：</p>
<pre><code>dc fetch my-project                 # 解析依赖并写 dolphin.lock
dc build my-project --locked        # 要求锁与清单一致且不重写
dc build my-project --offline       # 不访问网络
dc package my-project               # 产出 target/package/&lt;name&gt;-&lt;version&gt;.dlib
dc publish my-project --repository default</code></pre>
<p>解析结果写入根目录 <code>dolphin.lock</code>，应提交到版本管理。<code>--locked</code> 要求锁与清单、仓库、编译器版本一致；<code>--offline</code> 不访问 HTTP(S)。使用远程依赖时，<code>dc</code> 通过 HTTPS 下载到内容寻址缓存（<code>DOLPHIN_HOME</code>，默认 <code>~/.dolphin</code>），并校验 SHA-256。</p>
<div class="callout">
  <p><strong>跨包泛型：</strong>同一库的泛型只在消费端单态化一次，实例 key 由 <code>(包身份, 限定名, 具体类型实参)</code> 组成，因此不同包的同名定义不会冲突。</p>
</div>

<h2>5. 命令行参考</h2>
<pre><code>dc check &lt;项目目录或main.do&gt; [--locked] [--offline] [--color auto|always|never]
dc build &lt;项目目录或main.do&gt; [--bin &lt;名称&gt;] [--lib] [-o &lt;输出文件&gt;] [--debug|--release] [--system-linker] [--locked] [--offline]
dc run   &lt;项目目录或main.do&gt; [--bin &lt;名称&gt;] [--debug|--release]
dc package &lt;项目目录&gt; [--locked] [--offline]
dc fetch   &lt;项目目录&gt; [--locked] [--offline]
dc publish &lt;项目目录&gt; [--repository &lt;id&gt;]
dc info &lt;项目目录&gt;
dc env</code></pre>
<p>使用 <code>dc &lt;子命令&gt; --help</code> 查看子命令参数。<code>--color</code> 是全局选项，可放在子命令前后。<code>dc info</code> 只接受项目目录，<code>dc env</code> 显示宿主/目标平台、ABI、所选链接器与缓存根。</p>

<h2>6. 一个多目标项目</h2>
<pre><code>[package]
group = "me.foxlab"
name = "tools"
version = "0.1.0"

[lib]
path = "src/lib.do"

[[bin]]
name = "cli"
path = "src/cli.do"

[[bin]]
name = "server"
path = "src/server.do"</code></pre>
<pre><code>dc build tools            # 构建 lib 与全部 bin
dc build tools --lib      # 只构建库
dc run tools --bin cli    # 运行指定可执行目标</code></pre>

<h2>练习</h2>
<ol>
  <li>把上一章的计算器拆分为 <code>src/main.do</code> 与 <code>src/calc/eval.do</code>，正确声明 <code>pkg</code> 与 <code>pub</code>。</li>
  <li>为项目编写 <code>dolphin.toml</code>，声明一个 lib 与两个 bin。</li>
  <li>创建一个本地库，通过 <code>{ path = "../..." }</code> 依赖并调用其中的泛型函数。</li>
</ol>
`
    },

    {
      id: "tutorial/generics",
      title: "泛型、trait 与标准库",
      body: `
<h1>泛型、trait 与标准库</h1>
<p>本章介绍泛型函数与泛型类型、方法与 trait 的静态分派，以及随编译器分发的源码标准库。</p>

<h2>1. 泛型函数</h2>
<p>泛型参数写在函数名后的尖括号中，调用时可以显式给出类型实参，也可以由参数推断：</p>
<pre><code>fn identity&lt;T&gt;(value: T): T {
    return value;
}

fn main() {
    val a = identity&lt;i32&gt;(20);   // 显式
    val b = identity(22);        // 由实参推断为 i32
    println("{} {}", a, b);
}</code></pre>
<p>编译器以工作队列对泛型做单态化：每个 <code>(包身份, 限定名, 具体类型实参)</code> 只生成一次代码，未实例化的模板不产出符号。实例链深度上限 128，实例总数上限 10000，超限会给出诊断。支持模板调用模板与嵌套泛型实例。</p>

<h2>2. 泛型结构体与枚举</h2>
<pre><code>struct Pair&lt;T&gt; {
    pub first: T,
    pub second: T,
}

enum Maybe&lt;T&gt; {
    Just(T),
    Nothing,
}

fn main() {
    val p = Pair&lt;i32&gt;(3, 4);
    val none: Maybe&lt;i32&gt; = Maybe.Nothing;
    println("{}", p.first);
}</code></pre>
<p>泛型枚举的 payload 可以携带 <code>string</code>、结构体等复合值。结构体自引用必须经过指针，例如 <code>struct Node&lt;T&gt; { next: *Node&lt;T&gt; }</code>；按值包含环会在实例化后立即报错。</p>

<h2>3. 方法与 impl</h2>
<p>使用 <code>impl</code> 为类型定义方法。接收者可以是 <code>self</code>、<code>self: *Self</code> 或 <code>self: *const Self</code>，可寻址对象会自动取址：</p>
<pre><code>struct Counter {
    value: i32,
}

impl Counter {
    fn new(): Counter {
        return Counter(0);
    }

    fn increment(self: *Self) {
        self-&gt;value += 1;
    }

    fn get(self: *const Self): i32 {
        return self-&gt;value;
    }
}

fn main() {
    var counter = Counter::new();
    counter.increment();
    counter.increment();
    println("count = {}", counter.get());
}</code></pre>
<p>也可以为泛型类型定义方法：</p>
<pre><code>impl&lt;T&gt; Pair&lt;T&gt; {
    fn swapped(self): Pair&lt;T&gt; {
        return Pair&lt;T&gt;(self.second, self.first);
    }
}</code></pre>

<h2>4. trait 与关联类型</h2>
<p>trait 声明行为契约，通过 <code>impl Trait for Type</code> 提供静态分派实现：</p>
<pre><code>trait Head {
    type Item;
    fn head(self: *const Self): Self::Item;
}

impl&lt;T&gt; Head for Pair&lt;T&gt; {
    type Item = T;
    fn head(self: *const Self): T {
        return self-&gt;first;
    }
}</code></pre>
<p>类型参数可以带单约束 <code>T: Trait</code>，关联类型写作 <code>Self::Item</code> 或 <code>C::Item</code>：</p>
<pre><code>fn first_of&lt;T: Head&gt;(container: *const T): T::Item {
    return container.head();
}</code></pre>
<p>缺少约束实现、缺失 trait 成员、未知或重复方法、trait 实现签名不一致都会产生源码级诊断。当前不支持多约束、trait 默认方法与动态分派；同名 trait 方法按重复方法拒绝。</p>

<h2>5. 标准库总览</h2>
<p>标准库由两部分组成：编译器内建入口与源码标准库。<code>Option</code>、<code>Result</code>、<code>Iterator</code> 作为最小 prelude 自动可用。</p>
<table>
  <thead><tr><th>模块</th><th>内容</th></tr></thead>
  <tbody>
    <tr><td><code>std</code></td><td><code>Option&lt;T&gt;</code>、<code>Result&lt;T, E&gt;</code>、<code>Iterator</code></td></tr>
    <tr><td><code>std.mem</code></td><td>内建分配、释放、复制、布局查询与视图构造</td></tr>
    <tr><td><code>std.collections</code></td><td><code>Vec&lt;T&gt;</code>、<code>SliceIter&lt;T&gt;</code>、<code>Range</code></td></tr>
    <tr><td><code>std.text</code></td><td><code>String</code>、<code>concat</code>、<code>trim</code>、<code>substring</code>、<code>from_utf8</code> 等</td></tr>
    <tr><td><code>std.ffi</code></td><td><code>CString</code></td></tr>
  </tbody>
</table>

<h2>6. 迭代协议与 Vec</h2>
<pre><code>use std.collections.Vec;

fn main() {
    var numbers = Vec&lt;i32&gt;::init();
    defer numbers.deinit();

    numbers.push(1);
    numbers.push(2);
    numbers.push(3);

    var total = 0;
    for value in numbers.iter() {
        total += value;
    }

    val found: Option&lt;i32&gt; = numbers.get(1_usize);
    if found.is_some() {
        println("total = {}", total);
    }
}</code></pre>
<p><code>for x in expr</code> 要求 <code>expr</code> 实现 <code>Iterator</code>；数组与切片经只读切片适配到 <code>SliceIter&lt;T&gt;</code>，范围降低为 <code>Range</code>，<code>s.iter()</code> 零分配。<code>Vec&lt;T&gt;</code> 拥有型资源必须显式 <code>deinit</code>，详见<a href="#/std/collections">标准库参考</a>。</p>

<h2>7. 字符串工具</h2>
<pre><code>use std.text;

fn main() {
    val raw = "  Dolphin  ";
    val trimmed = text.trim(raw);
    var combined = text.concat(trimmed, "!");
    defer combined.deinit();
    println("{}", combined.view());

    val part = text.substring("hello", 0_usize, 2_usize);
    val label = match part {
        Result.Ok(value) =&gt; value,
        Result.Err(error) =&gt; "error",
    };
    println("{}", label);
}</code></pre>

<h2>练习</h2>
<ol>
  <li>编写泛型函数 <code>max_of&lt;T&gt;</code>，要求类型实现比较 trait（如可行）。</li>
  <li>为自定义结构体实现一个 trait，并在泛型函数中使用该约束。</li>
  <li>用 <code>Vec&lt;i32&gt;</code> 保存数据，遍历并计算平均值。</li>
</ol>
`
    },

    {
      id: "tutorial/memory",
      title: "内存模型与 C 互操作",
      body: `
<h1>内存模型与 C 互操作</h1>
<p>Dolphin 采用 Zig 式显式内存模型：没有 GC、RC、自动析构、隐式 move 或借用检查；值默认复制，指针和切片只复制描述符。分配与释放显式可见，<code>defer</code> 是唯一的作用域清理语法。</p>

<h2>1. 指针、切片与整数宽度</h2>
<table>
  <thead><tr><th>形式</th><th>含义</th></tr></thead>
  <tbody>
    <tr><td><code>*T</code> / <code>*const T</code></td><td>可为 null 的可写 / 只读原始指针</td></tr>
    <tr><td><code>[]T</code> / <code>[]const T</code></td><td>可写 / 只读视图 <code>{ ptr, len }</code></td></tr>
    <tr><td><code>string</code></td><td>合法 UTF-8 的只读字节视图，不拥有内存</td></tr>
    <tr><td><code>usize</code> / <code>isize</code></td><td>目标指针宽度整数（当前 64 位）</td></tr>
  </tbody>
</table>
<ul>
  <li><code>&amp;var_local</code> 得到 <code>*T</code>，<code>&amp;val_local</code> 与不可变参数取址得到 <code>*const T</code>；取址只作用于稳定存储，禁止对临时值取址，索引取址执行边界检查。</li>
  <li><code>[]T</code> 可隐式转为 <code>[]const T</code>，<code>*T</code> 可隐式转为 <code>*const T</code>，反向转换是编译错误。</li>
  <li><code>null</code> 只能用于已知指针类型的上下文；<code>.ptr</code>、<code>.len</code> 是切片只读字段，<code>string</code> 的 <code>.len</code> 是字节数。</li>
</ul>

<h2>2. std.mem</h2>
<p><code>use std.mem;</code> 提供编译器内建入口，无需磁盘标准库：</p>
<table>
  <thead><tr><th>API</th><th>结果及责任</th></tr></thead>
  <tbody>
    <tr><td><code>mem.alloc&lt;T&gt;(count): []T</code></td><td>分配未初始化连续存储；写入后读取，交 <code>mem.free</code></td></tr>
    <tr><td><code>mem.free&lt;T&gt;(buffer: []T)</code></td><td>只释放完整原始切片，不递归释放元素</td></tr>
    <tr><td><code>mem.create&lt;T&gt;(value): *T</code> / <code>mem.destroy&lt;T&gt;(ptr)</code></td><td>分配 / 释放单个已初始化对象</td></tr>
    <tr><td><code>mem.size_of&lt;T&gt;()</code> / <code>mem.align_of&lt;T&gt;()</code></td><td>编译期布局常量</td></tr>
    <tr><td><code>mem.copy&lt;T&gt;(dst, src)</code></td><td>等长按值复制（<code>memmove</code> 语义），不分配、不深拷贝</td></tr>
    <tr><td><code>mem.is_valid_utf8(bytes): bool</code></td><td>只校验编码，不分配、不 trap</td></tr>
    <tr><td><code>mem.view&lt;T&gt;(ptr, len)</code> / <code>mem.view_const&lt;T&gt;(ptr, len)</code></td><td>从 C 指针创建视图，不取得释放权</td></tr>
    <tr><td><code>mem.cast_ptr&lt;T&gt;(ptr)</code> / <code>mem.cast_const_ptr&lt;T&gt;(ptr)</code></td><td>在对象指针与 <code>*Unit</code> 间显式转换并校验对齐</td></tr>
  </tbody>
</table>
<p>切片子视图写作 <code>buffer.slice(start, end)</code>，始终检查 <code>0 &lt;= start &lt;= end &lt;= len</code>。</p>

<h2>3. 分配、使用与释放</h2>
<pre><code>use std.mem;

struct Record {
    id: u32,
    score: f64,
}

fn main() {
    val records = mem.alloc&lt;Record&gt;(3);
    defer mem.free(records);

    var index: usize = 0_usize;
    while index &lt; records.len {
        records[index] = Record(index as u32, (index as f64) * 1.5);
        index += 1_usize;
    }

    println("count = {}", records.len);
}</code></pre>

<h2>4. defer</h2>
<p><code>defer call_expression;</code> 绑定最近的词法块，块退出时按注册逆序执行。调用参数在退出时求值，名称绑定注册处的局部变量身份，因此读取的是最新值：</p>
<pre><code>val buffer = mem.alloc&lt;u8&gt;(4_usize);
defer mem.free(buffer);   // 块退出时释放</code></pre>
<p>自然出块、<code>return</code>、<code>break</code>、<code>continue</code> 都会清理实际退出到的作用域；<code>return expr</code> 先求值并保存返回值再清理。trap 或 OOM 不会展开 Dolphin 栈，因此不会执行 <code>defer</code>。不支持 defer 块、嵌套 defer 与资源 <code>try</code>。</p>

<h2>5. 字符串视图</h2>
<pre><code>val text = "dolphin";
val bytes = text.bytes();          // []const u8，零分配
val again = string.from_bytes(bytes);
println("{}", again == text);      // true</code></pre>
<p>字符串字面量是静态视图，永远不能 <code>free</code>。需要拥有型、可拼接的字符串时使用 <code>std.text.String</code>。</p>

<h2>6. C 互操作</h2>
<pre><code>extern struct CPoint { x: f64, y: f64 }

extern "C" {
    pub fn demo_add(a: i32, b: i32): i32;
    pub fn demo_create(): *Unit;
    pub fn demo_destroy(handle: *Unit);
    pub fn demo_translate(point: *CPoint, dx: f64): f64;
}</code></pre>
<ul>
  <li><code>c_int</code> / <code>c_uint</code> / <code>c_long</code> / <code>c_ulong</code> / <code>c_char</code> 是平台相关内建别名。</li>
  <li>extern 函数按原 C 符号导入，不加 Dolphin mangling；<code>*Unit</code> 表示 C 的 <code>void*</code>，禁止解引用。</li>
  <li>首版拒绝把 <code>bool</code>、Dolphin <code>char</code>、<code>string</code>、切片、普通结构体与枚举按值写入 extern 签名；<code>extern struct</code> 只能通过指针传给 C。</li>
  <li><code>dolphin.toml</code> 的 <code>[native.&lt;triple&gt;]</code> 声明 <code>objects</code>、<code>static-libs</code>、<code>shared-libs</code>、<code>runtime-files</code>。<code>dc</code> 消费预编译 C 文件，不编译 C 源码。</li>
</ul>

<h2>7. 运行时失败</h2>
<table>
  <thead><tr><th>情况</th><th>Debug</th><th>Release</th></tr></thead>
  <tbody>
    <tr><td>算术 trap、切片越界、无效切片区间、null 视图 / 对齐检查</td><td><code>101</code></td><td><code>101</code></td></tr>
    <tr><td>分配失败 / 分配尺寸溢出</td><td><code>102</code></td><td><code>102</code></td></tr>
    <tr><td>无效释放：未知地址、已释放地址、长度不匹配</td><td><code>103</code></td><td>不保证检测</td></tr>
    <tr><td>UTF-8 校验失败</td><td><code>104</code></td><td><code>104</code></td></tr>
    <tr><td>正常退出时泄漏</td><td>stderr 报告，保留退出码</td><td>不追踪</td></tr>
  </tbody>
</table>
<p>Debug 构建链接带存活分配登记表的检测版运行时，Release 构建链接普通版。这些失败都是安全终止，不会产生未定义行为。</p>

<h2>练习</h2>
<ol>
  <li>用 <code>mem.alloc</code> 分配一个结构体数组，填充后求和，并用 <code>defer</code> 释放。</li>
  <li>编写一个把 <code>string</code> 转为 <code>CString</code> 的函数，并处理内部 NUL 的错误。</li>
  <li>说明为什么 <code>defer</code> 在程序因 trap 终止时不会执行。</li>
</ol>
`
    },

    {
      id: "tutorial/tour",
      title: "综合实战：从入门到精通",
      body: `
<h1>综合实战：从入门到精通</h1>
<p>本章把前面学到的知识整合起来，构建一个使用泛型、标准库容器、模式匹配与显式内存管理的完整程序：一个简单的成绩统计工具。</p>

<h2>1. 需求</h2>
<ul>
  <li>定义表示学生成绩的结构体。</li>
  <li>使用 <code>Vec&lt;Record&gt;</code> 动态收集记录。</li>
  <li>用枚举与 <code>match</code> 表示查询结果。</li>
  <li>计算总分、平均分与最高分。</li>
  <li>所有拥有型资源都用 <code>defer</code> 释放。</li>
</ul>

<h2>2. 项目结构</h2>
<pre><code>report/
├── dolphin.toml
└── src/
    ├── main.do
    └── report/
        └── stats.do</code></pre>
<pre><code># dolphin.toml
[package]
group = "me.foxlab"
name = "report"
version = "0.1.0"
source = "src"

[[bin]]
name = "report"
path = "src/main.do"</code></pre>

<h2>3. 统计模块</h2>
<pre><code>// src/report/stats.do
pkg report;

use std.Option;
use std.collections.Vec;

pub struct Record {
    pub id: u32,
    pub score: i32,
}

pub enum Query {
    Found(i32),
    NotFound,
}

pub fn total(records: *const Vec&lt;Record&gt;): i32 {
    var sum = 0;
    for record in records.iter() {
        sum += record.score;
    }
    return sum;
}

pub fn highest(records: *const Vec&lt;Record&gt;): Query {
    var best = 0;
    var seen = false;
    for record in records.iter() {
        if !seen || record.score &gt; best {
            best = record.score;
            seen = true;
        }
    }
    if seen {
        return Query.Found(best);
    }
    return Query.NotFound;
}</code></pre>

<h2>4. 主程序</h2>
<pre><code>// src/main.do
use report.stats;
use report.stats.Record;
use report.stats.Query;
use std.collections.Vec;

fn main(): i32 {
    var records = Vec&lt;Record&gt;::with_capacity(4_usize);
    defer records.deinit();

    records.push(Record(1_u32, 88));
    records.push(Record(2_u32, 95));
    records.push(Record(3_u32, 72));

    val sum = stats.total(&amp;records);
    val average = sum / (records.len() as i32);
    println("total = {}, average = {}", sum, average);

    val best = stats.highest(&amp;records);
    val best_score = match best {
        Query.Found(score) =&gt; score,
        Query.NotFound =&gt; -1,
    };
    if best_score &gt;= 0 {
        println("best = {}", best_score);
    } else {
        println("no records");
    }

    return average;
}</code></pre>
<p>构建并运行：</p>
<pre><code>dc check report
dc build report --release
./report/target/report
echo $?</code></pre>

<h2>5. 复盘</h2>
<table>
  <thead><tr><th>用到的能力</th><th>所在章节</th></tr></thead>
  <tbody>
    <tr><td>结构体、字段、位置构造</td><td><a href="#/tutorial/composites">数组、结构体、枚举与 match</a></td></tr>
    <tr><td>枚举、<code>match</code> 穷尽分派</td><td><a href="#/tutorial/composites">数组、结构体、枚举与 match</a></td></tr>
    <tr><td><code>pkg</code> / <code>use</code> / <code>pub</code></td><td><a href="#/tutorial/packages">模块与包管理</a></td></tr>
    <tr><td>泛型容器 <code>Vec&lt;T&gt;</code> 与迭代协议</td><td><a href="#/tutorial/generics">泛型、trait 与标准库</a></td></tr>
    <tr><td>指针参数与显式释放</td><td><a href="#/tutorial/memory">内存模型与 C 互操作</a></td></tr>
  </tbody>
</table>

<h2>6. 继续深入</h2>
<ul>
  <li>阅读<a href="#/std/overview">标准库参考</a>，掌握 <code>Vec</code>、<code>String</code>、<code>Option</code>、<code>Result</code> 的完整 API。</li>
  <li>阅读<a href="#/std/mem">std.mem</a>，理解内存布局与视图构造。</li>
  <li>为统计模块添加单元测试风格的校验函数，用返回值表示成功或失败。</li>
  <li>尝试把统计逻辑抽成通用库，通过 <code>dolphin.toml</code> 的 <code>[lib]</code> 与 path 依赖复用。</li>
</ul>

<h2>挑战题</h2>
<ol>
  <li>实现一个泛型函数 <code>reduce&lt;T&gt;</code>，在 <code>Iterator</code> 上累积求和，并处理空集合。</li>
  <li>用 <code>std.text.String</code> 生成一份格式化的文本报告并释放其缓冲。</li>
  <li>通过 <code>extern "C"</code> 调用一个你自己的 C 函数，把统计结果传给它。</li>
</ol>
`
    }
  ]
});
