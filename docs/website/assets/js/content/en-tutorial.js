window.DolphinDocsContent = window.DolphinDocsContent || {};
window.DolphinDocsContent["en-US"] = window.DolphinDocsContent["en-US"] || { groups: [] };
window.DolphinDocsContent["en-US"].groups.push({
  id: "tutorial",
  title: "Learn Dolphin",
  pages: [
    {
      id: "tutorial/install",
      title: "Installation and your first program",
      body: `
<h1>Installation and your first program</h1>
<p>This chapter gets the Dolphin toolchain installed and compiles your first program. Dolphin ships as a release archive: unpack it and go, with no Rust toolchain or compiler checkout. Linking uses the system linker (<code>cc</code> on Unix, <code>link</code> on Windows) and needs a native toolchain plus the platform's CRT/SDK, as described in the <a href="../install.html">installation guide</a>.</p>

<h2>1. Install the compiler</h2>
<p>Releases cover three tier-1 platforms: Linux x86_64, macOS ARM64 and Windows x86_64. Download the archive, unpack it anywhere, and add the directory to <code>PATH</code>:</p>
<pre><code>mkdir -p ~/.local/dolphin
tar xzf dolphin-0.4.0-x86_64-unknown-linux-gnu.tar.gz -C ~/.local/dolphin
export PATH="$HOME/.local/dolphin:$PATH"</code></pre>
<p>Inside the archive, <code>dc</code> is the compiler command. No linker is bundled: linking uses the native system linker, and with a Rust toolchain installed, <code>--bundled-linker</code> switches to <code>rust-lld</code>. See the <a href="../install.html">installation guide</a> for checksums and uninstall steps.</p>
<p>Verify the installation:</p>
<pre><code>dc --version
dc env</code></pre>
<p><code>dc env</code> prints the host/target platform, ABI, selected linker and cache root. If the command is not found, check that <code>PATH</code> points at the unpacked directory.</p>

<h2>2. Your first program</h2>
<p>Create <code>hello.do</code>:</p>
<pre><code>fn main() {
    println("Hello, Dolphin!");
}</code></pre>
<p>Compile a single file directly, without modules or imports:</p>
<pre><code>dc build hello.do -o hello
./hello</code></pre>
<p>Output:</p>
<pre><code>Hello, Dolphin!</code></pre>
<p>You can also use the standard project layout. Every project uses <code>src/</code> as its source root, and the <code>main</code> function must live in a file directly under <code>src</code>:</p>
<pre><code>hello-project/
└── src/
    └── main.do</code></pre>
<pre><code>cd hello-project
dc check .
dc build .
dc run .</code></pre>
<p>Build artifacts land in the project's <code>target/</code> directory:</p>
<pre><code>target/hello-project             native executable
target/hello-project.o           Dolphin program object
target/hello-project.runtime.o   minimal runtime object</code></pre>

<h2>3. Entry point and exit codes</h2>
<p>An executable must define exactly one <code>main</code>. It takes no parameters; application arguments are read through <code>std.process</code> (see <a href="#/std/io">process, streams and files</a>). It may return an <code>i32</code> process exit code:</p>
<pre><code>fn main(): i32 {
    return 42;
}</code></pre>
<pre><code>dc build . &amp;&amp; ./target/hello-project
echo $?   # 42</code></pre>
<p>Falling off the end or executing <code>return;</code> exits with <code>0</code>. A function with a return type must return a value on every reachable path.</p>

<h2>4. Common commands</h2>
<table>
  <thead><tr><th>Command</th><th>Purpose</th></tr></thead>
  <tbody>
    <tr><td><code>dc check &lt;project&gt;</code></td><td>Lexing, parsing, name and type checks only; no artifacts</td></tr>
    <tr><td><code>dc build &lt;project&gt;</code></td><td>Compile and link an executable (Debug by default)</td></tr>
    <tr><td><code>dc run &lt;project&gt;</code></td><td>Build then run, propagating the program's exit code</td></tr>
    <tr><td><code>dc test &lt;project&gt;</code></td><td>Run the project's <code>tests/*.do</code> suite</td></tr>
    <tr><td><code>dc info &lt;project&gt;</code></td><td>Show package coordinates, targets, dependencies and lock status</td></tr>
    <tr><td><code>dc fmt &lt;file-or-dir...&gt;</code></td><td>Reformat in place; <code>--check</code> only reports</td></tr>
    <tr><td><code>dc lsp [project]</code></td><td>Start the stdio language server (diagnostics, hover, go-to-definition)</td></tr>
    <tr><td><code>dc env</code></td><td>Show host/target platform and toolchain details</td></tr>
  </tbody>
</table>
<p><code>--debug</code> and <code>--release</code> are mutually exclusive, and Debug is the default. Release turns on more optimization for smaller, faster output.</p>

<h2>5. Next steps</h2>
<p>You can now build and run Dolphin programs. Continue with the <a href="#/tutorial/intro">language overview</a>, or jump to <a href="#/tutorial/types">variables, types and expressions</a> to start a systematic tour.</p>
`
    },

    {
      id: "tutorial/intro",
      title: "Language overview",
      body: `
<h1>Language overview</h1>
<p>Dolphin is a statically typed language whose syntax draws on Rust, Kotlin, Java and C. It compiles to native executables that the operating system can run directly, and it prioritizes simple rules, predictable behavior and a maintainable compiler.</p>

<h2>1. Design goals</h2>
<ul>
  <li>Concise syntax that stays readable in everyday code.</li>
  <li>Static types with local type inference.</li>
  <li>Explicit, safe defaults that surface mistakes at compile time.</li>
  <li>Functions, arrays, control flow, modules, generics and a small standard library.</li>
  <li>Compilation to native objects linked into a runnable binary.</li>
  <li>A clear compiler structure that is easy to extend with features and backends.</li>
</ul>
<p>Dolphin uses a Zig-style explicit memory model: no garbage collection, reference counting, implicit destruction or borrow checking. Values are copied by default; pointers and slices copy only their descriptor, and allocation is explicit.</p>

<h2>2. Source files and comments</h2>
<p>Source files are UTF-8 with a <code>.do</code> extension. Both line and block comments are supported:</p>
<pre><code>// line comment

/*
 * block comment (nesting is not supported)
 */</code></pre>
<p>Identifiers start with an ASCII letter or underscore and continue with letters, digits and underscores: <code>value</code>, <code>user_name</code>, <code>value2</code>, <code>_internal</code>. Keywords cannot be identifiers, and Unicode identifiers are not supported yet.</p>

<h2>3. A complete example</h2>
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
<p>This sample shows structs, functions, local type inference, field access and formatted output. Each is covered in later chapters.</p>

<h2>4. Compilation pipeline</h2>
<pre><code>UTF-8 source
  -&gt; tokens
  -&gt; AST
  -&gt; module and name resolution
  -&gt; type and control-flow checks
  -&gt; typed CFG IR
  -&gt; Cranelift IR
  -&gt; native object
  -&gt; embedded runtime + system linker (or rust-lld with --bundled-linker)
  -&gt; native executable</code></pre>
<p>Dolphin programs link glibc on Linux, libSystem on macOS and UCRT on Windows. These are operating-system components.</p>

<h2>5. A peek at modules</h2>
<p>Every project uses <code>src/</code> as its source root. Files directly under <code>src</code> belong to the root module and may omit <code>pkg</code>; files in subdirectories must declare their directory as the package (the file name is not part of <code>pkg</code>):</p>
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
<p>Top-level declarations are private to their module by default; adding <code>pub</code> makes them accessible across modules. See <a href="#/tutorial/packages">modules, visibility and packages</a>.</p>

<h2>6. Current boundaries</h2>
<p>Dolphin is still evolving. Not yet implemented: nested and empty array literals, resource <code>try</code> syntax, borrow checking, lifetimes, closures, dynamic dispatch, the <code>?</code> operator, network standard libraries, C header import and cross-compilation. DWARF line and function info is emitted by the LLVM backend in Debug builds on Unix; Cranelift and Windows/PDB have no debug info yet.</p>

<h2>7. Keep learning</h2>
<ul>
  <li><a href="#/tutorial/types">Variables, types and expressions</a></li>
  <li><a href="#/tutorial/control">Control flow and functions</a></li>
  <li><a href="#/tutorial/composites">Arrays, structs, enums and match</a></li>
  <li><a href="#/tutorial/generics">Generics, traits and the standard library</a></li>
  <li><a href="#/tutorial/memory">Memory model and C interop</a></li>
</ul>
`
    },

    {
      id: "tutorial/types",
      title: "Variables, types and expressions",
      body: `
<h1>Variables, types and expressions</h1>
<p>Start with the scalar types, then see how <code>val</code>/<code>var</code>, conversions and operators behave in practice.</p>

<h2>1. Bindings</h2>
<p>Use <code>var</code> for mutable variables and <code>val</code> for immutable ones. Every variable must be initialized at its declaration; the type can be inferred from the initializer:</p>
<pre><code>var count = 1;
count = 2;

val limit = 10;
// limit = 20;  // compile error: cannot assign to val</code></pre>
<pre><code>var count: i32 = 1;      // explicit type
val name = "Dolphin";    // inferred as string</code></pre>
<p>A scope cannot declare the same name twice, and inner scopes may shadow outer bindings. Function parameters are immutable locals.</p>
<div class="callout">
  <p><strong>Tip:</strong> prefer <code>val</code> and reach for <code>var</code> only when reassignment is truly needed. It lets the compiler catch accidental mutation.</p>
</div>

<h2>2. Integer types</h2>
<p>Signed <code>i8</code>, <code>i16</code>, <code>i32</code>, <code>i64</code>, unsigned <code>u8</code>, <code>u16</code>, <code>u32</code>, <code>u64</code>, and pointer-width <code>usize</code>/<code>isize</code> are supported. An unsuffixed integer literal defaults to <code>i32</code>:</p>
<pre><code>val count = 10;            // i32
val small: i8 = -8_i8;
val medium = 1600_i16;
val large: i64 = 64000_i64;
val capacity: usize = 100_usize;</code></pre>
<table>
  <thead><tr><th>Type</th><th>Range</th></tr></thead>
  <tbody>
    <tr><td><code>i8</code></td><td>-128 to 127</td></tr>
    <tr><td><code>i32</code></td><td>-2147483648 to 2147483647</td></tr>
    <tr><td><code>i64</code></td><td>-9223372036854775808 to 9223372036854775807</td></tr>
    <tr><td><code>u8</code></td><td>0 to 255</td></tr>
    <tr><td><code>u64</code></td><td>0 to 18446744073709551615</td></tr>
    <tr><td><code>usize</code> / <code>isize</code></td><td>Pointer width (64-bit today)</td></tr>
  </tbody>
</table>
<p>Numeric types never mix implicitly, and potentially lossy narrowing is rejected. Use <code>as</code> explicitly:</p>
<pre><code>val large: i64 = 64000_i64;
// val small: i32 = large;         // compile error
val small: i32 = large as i32;     // explicit
val back: i64 = small as i64;      // sign-extended widening</code></pre>
<p>Integer add, subtract, multiply and negate are overflow-checked at runtime; division and modulo by zero terminate with a uniform runtime error. Narrowing keeps the low bits; widening sign- or zero-extends according to the source type.</p>

<h2>3. Floating point</h2>
<p>IEEE <code>f32</code> and <code>f64</code> are supported. Unsuffixed float literals default to <code>f64</code>:</p>
<pre><code>val ratio = 0.5;          // f64
val precise = 2.25_f64;   // f64
val single: f32 = 1.5_f32;</code></pre>
<p>Floats support <code>+</code>, <code>-</code>, <code>*</code>, <code>/</code>, comparison and equality, but not <code>%</code>. Float division by zero follows IEEE rules, and <code>as</code> from float to integer saturates.</p>

<h2>4. Booleans and characters</h2>
<pre><code>val enabled: bool = true;
val disabled = false;

val symbol: char = '海';
val latin: char = 'D';</code></pre>
<p>Conditions must be <code>bool</code>; integers are never implicitly truthy. <code>char</code> is a Unicode scalar value and supports comparison, equality, arrays, function calls, formatting and explicit conversion to and from integers.</p>

<h2>5. Strings</h2>
<p><code>string</code> is an immutable UTF-8 view compared by byte content:</p>
<pre><code>val greeting = "Hello, 世界!";
println("{}", greeting == "Hello, 世界!");  // true
println("bytes = {}", length(greeting));     // UTF-8 byte count</code></pre>
<p>String literals support these escapes:</p>
<pre><code>\\n        newline
\\r        carriage return
\\t        tab
\\\\        backslash
\\"        double quote
\\u{1F600} Unicode code point</code></pre>
<p><code>length(s)</code> returns the UTF-8 byte count. <code>s.bytes()</code> returns a read-only byte view <code>[]const u8</code> with zero allocation, and <code>string.from_bytes(bytes)</code> validates UTF-8 and returns a <code>string</code> view; invalid UTF-8 terminates with <code>104</code>. Strings do not support <code>+</code>; use <code>std.text.concat</code> to join them.</p>

<h2>6. Unit</h2>
<p>A function that omits its return type returns <code>Unit</code>, which cannot be stored, used in expressions or formatted:</p>
<pre><code>fn do_nothing() {
    return;
}</code></pre>

<h2>7. Operators and precedence</h2>
<table>
  <thead><tr><th>Category</th><th>Operators</th></tr></thead>
  <tbody>
    <tr><td>Arithmetic</td><td><code>+</code> <code>-</code> <code>*</code> <code>/</code> <code>%</code></td></tr>
    <tr><td>Comparison</td><td><code>&lt;</code> <code>&lt;=</code> <code>&gt;</code> <code>&gt;=</code></td></tr>
    <tr><td>Equality</td><td><code>==</code> <code>!=</code></td></tr>
    <tr><td>Logic</td><td><code>!</code> <code>&amp;&amp;</code> <code>||</code></td></tr>
    <tr><td>Assignment</td><td><code>=</code> <code>+=</code> <code>-=</code> <code>*=</code> <code>/=</code> <code>%=</code></td></tr>
    <tr><td>Conversion</td><td><code>as</code></td></tr>
  </tbody>
</table>
<p>Precedence, highest to lowest: calls/index/grouping, unary <code>-</code> and <code>!</code>, <code>* / %</code>, <code>+ -</code>, comparison, equality, <code>&amp;&amp;</code>, <code>||</code>. Assignment is a statement, not a nestable expression.</p>
<p><code>&amp;&amp;</code> and <code>||</code> short-circuit:</p>
<pre><code>val safe = false &amp;&amp; 1 / 0 == 0;  // right side never runs</code></pre>

<h2>8. Output and formatting</h2>
<pre><code>print("Hello, ");
println("{}!", "Dolphin");
println("{} + {} = {}", 1, 2, 3);
println("{{}}");   // prints {}</code></pre>
<p>Each <code>{}</code> consumes one argument. All integers, floats, <code>char</code>, <code>bool</code> and <code>string</code> are supported. A mismatch between placeholders and arguments is a compile error. The first argument must currently be a string literal.</p>

<h2>Exercises</h2>
<ol>
  <li>Define integers of three different types, convert between them, and observe both compile errors and runtime behavior.</li>
  <li>Use <code>length</code> and <code>bytes</code> to measure a Chinese string and explain the difference from its character count.</li>
  <li>Write an expression with short-circuiting that never divides by zero.</li>
</ol>
`
    },

    {
      id: "tutorial/control",
      title: "Control flow and functions",
      body: `
<h1>Control flow and functions</h1>
<p>Conditions, loops and functions; by the end you can write a small calculator.</p>

<h2>1. if / else</h2>
<p>Parentheses are not required around conditions, and conditions must be <code>bool</code>:</p>
<pre><code>if score &gt;= 60 {
    println("passed");
} else {
    println("failed");
}</code></pre>
<p>Today <code>if</code> is a statement that produces no value, and the <code>else if</code> shorthand is not available. For multiple branches, either nest <code>if</code> or reach for <code>match</code>.</p>

<h2>2. while and loop</h2>
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
<p><code>loop</code> creates an infinite loop that exits via <code>break</code>.</p>

<h2>3. for and ranges</h2>
<p>The half-open range <code>start..end</code> excludes the end, while <code>start..=end</code> includes it:</p>
<pre><code>for i in 0..3 {    // 0, 1, 2
    println("{}", i);
}

for i in 0..=3 {   // 0, 1, 2, 3
    println("{}", i);
}</code></pre>
<p><code>for</code> also iterates arrays directly:</p>
<pre><code>val numbers = [1, 2, 3];
for number in numbers {
    println("{}", number);
}</code></pre>
<p><code>for x in expr</code> requires <code>expr</code> to implement the <code>Iterator</code> protocol; arrays and slices are adapted automatically, and ranges lower to a <code>Range</code> iterator. The loop variable is not assignable, and the iterated expression is evaluated once. <code>break</code> and <code>continue</code> may only appear inside loops.</p>

<h2>4. Functions</h2>
<p>Parameters must be annotated, and a value-returning function must declare its return type:</p>
<pre><code>fn add(a: i32, b: i32): i32 {
    return a + b;
}

fn greet(name: string) {
    println("Hello, {}!", name);
}</code></pre>
<p>Functions need not be defined before use, and recursion is supported:</p>
<pre><code>fn factorial(n: i32): i32 {
    if n &lt;= 1 {
        return 1;
    }
    return n * factorial(n - 1);
}</code></pre>
<p>A value-returning function must return the right type on every reachable path:</p>
<pre><code>fn max(a: i32, b: i32): i32 {
    if a &gt; b {
        return a;
    }
    return b;
}</code></pre>

<h2>5. main</h2>
<pre><code>fn main() {
    println("Hello");
}</code></pre>
<pre><code>fn main(): i32 {
    return 0;
}</code></pre>
<p>A program must define exactly one <code>main</code>. It takes no arguments today; when the return type is omitted it is treated as a process exit code. Falling off the end or <code>return;</code> exits with <code>0</code>, and <code>return expr;</code> must yield an <code>i32</code>.</p>

<h2>6. Control-flow checking</h2>
<p>The compiler rejects definitely-unreachable statements and any <code>break</code>/<code>continue</code> outside a loop. These checks surface mistakes at compile time rather than at runtime.</p>

<h2>Full example: prime sieve</h2>
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

<h2>Exercises</h2>
<ol>
  <li>Implement Fibonacci recursively and compare it with a loop version.</li>
  <li>Write a function that decides whether an integer is a perfect number.</li>
  <li>Use <code>for</code> with <code>break</code> to find the index of the first even number in an array.</li>
</ol>
`
    },

    {
      id: "tutorial/composites",
      title: "Arrays, structs, enums and match",
      body: `
<h1>Arrays, structs, enums and match</h1>
<p>Model data with arrays, structs and enums, then dispatch over enums with <code>match</code>.</p>

<h2>1. Fixed-size arrays</h2>
<p>An array type is written <code>[ElementType; length]</code>, and the length is part of the type:</p>
<pre><code>val inferred = [1, 2, 3];                 // [i32; 3]
val explicit: [i32; 3] = [1, 2, 3];
val repeated = [false; 4];                // [bool; 4]</code></pre>
<p>Indexing starts at 0 and accepts constants or runtime values:</p>
<pre><code>var values = [10, 20, 30];
val first = values[0];
values[1] = 25;
values[2] += 5;</code></pre>
<p>The compiler rejects statically out-of-bounds constant indices, and dynamic indices are checked at runtime. Only one-dimensional, non-empty arrays of non-<code>Unit</code> scalar elements are supported today; <code>[i32; 2]</code> and <code>[i32; 3]</code> are different types. Arrays are value types and are copied in full when passed or returned.</p>

<h2>2. Structs</h2>
<p>A struct aggregates fields into a value type, constructed positionally and accessed by field:</p>
<pre><code>struct Point {
    x: i32,
    y: i32,
}

fn main() {
    var p = Point(3, 4);
    println("point = ({}, {})", p.x, p.y);
    p.x = 10;             // fields of a var struct can be written
    println("x = {}", p.x);
}</code></pre>
<p>Fields must be unique within a struct, and accessing a missing field is a compile error. Fields are module-private by default; cross-module reads, writes or positional construction require <code>pub</code>:</p>
<pre><code>pub struct Pair {
    pub first: i32,
    pub second: i32,
}</code></pre>
<p>Fields may be scalar types, arrays, other structs or generic parameters. Self-reference must go through a pointer; a by-value layout cycle is a compile error.</p>

<h2>3. Enums</h2>
<p>An enum describes a finite set of values whose variants may carry data:</p>
<pre><code>enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
    Empty,
}</code></pre>
<p>Variants are constructed with <code>Shape.Circle(...)</code>; a payload-free variant is written <code>Shape.Empty</code> (without parentheses). Variant names must be unique within an enum, and payload types are fixed at the declaration.</p>

<h2>4. match</h2>
<p><code>match</code> is expression-style control flow that dispatches exhaustively over an enum:</p>
<pre><code>fn area(shape: Shape): f64 {
    return match shape {
        Shape.Circle(r) =&gt; 3.14159 * r * r,
        Shape.Rectangle(w, h) =&gt; w * h,
        Shape.Empty =&gt; 0.0,
    };
}</code></pre>
<ul>
  <li>All arms must produce the same type; <code>match</code> is used in expression position (for example <code>val area = match ...</code>).</li>
  <li>Missing or redundant arms produce source-level diagnostics.</li>
  <li>Destructuring and the <code>_</code> wildcard are supported for single fields and whole arms.</li>
</ul>
<pre><code>val label = match shape {
    Shape.Circle(_) =&gt; "circle",
    Shape.Rectangle(w, h) =&gt; "rect",
    _ =&gt; "other",
};</code></pre>
<div class="callout warn">
  <p><strong>Current limits:</strong> <code>match</code> cannot be used as a bare statement and an arm body cannot be a statement block (<code>=&gt; { ... }</code>) yet; <code>if</code> is a statement, not an expression. Pattern matching over non-enum values such as integers or strings is also not supported. Put multi-statement branching logic in a helper function.</p>
</div>

<h2>5. Using types across modules</h2>
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
<p>Top-level types are module-private by default; <code>pub struct</code> and <code>pub enum</code> can be referenced across modules. See <a href="#/tutorial/packages">modules and packages</a>.</p>

<h2>6. A complete domain model</h2>
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

<h2>Exercises</h2>
<ol>
  <li>Define <code>enum Color { Rgb(u8, u8, u8), Named(i32) }</code> and use <code>match</code> to return the sum of its components.</li>
  <li>Define a struct with <code>pub</code> fields, then construct and modify it from another module.</li>
  <li>Store several structs in an array and aggregate a field.</li>
</ol>
`
    },

    {
      id: "tutorial/packages",
      title: "Modules, visibility and packages",
      body: `
<h1>Modules, visibility and packages</h1>
<p>How <code>src/</code> maps to modules, how <code>use</code> and <code>pub</code> work, and how <code>dolphin.toml</code> describes dependencies and libraries.</p>

<h2>1. Source root and pkg</h2>
<p>Every project uses <code>src/</code> as its source root; <code>src</code> only organizes the project and is not part of any module name. Files directly under <code>src</code> form the root module and may omit <code>pkg</code>:</p>
<pre><code>project/
└── src/
    ├── main.do
    ├── helper.do
    └── mathutil/
        └── math.do</code></pre>
<p>A file in a subdirectory must declare <code>pkg</code> as its first effective statement, matching its directory relative to <code>src</code>; the file's own module name is the directory path plus the file name:</p>
<pre><code>// src/mathutil/math.do
pkg mathutil;

pub fn min(a: i32, b: i32): i32 {
    if a &lt; b {
        return a;
    }
    return b;
}</code></pre>
<p>Omitting <code>pkg</code> or mismatching the directory is a compile error. The <code>std</code> namespace is reserved for the standard library.</p>

<h2>2. use and pub</h2>
<p>Import a whole module to reach its public members through the module name, or import a single member:</p>
<pre><code>// src/main.do
use mathutil.math;         // reach it as math.min(...)
// use mathutil.math.min;  // or call min(...) directly
// use mathutil;           // or import the package prefix and use mathutil.math.min(...)

fn main() {
    println("min = {}", math.min(8, 3));
}</code></pre>
<p>Top-level declarations are private to their module by default; <code>pub</code> makes them accessible across modules. Importing unknown or private names, duplicate imports and unknown modules are compile errors. Wildcard imports (<code>use std.*</code>), aliases and re-exports are not supported, but fully-qualified public paths may be used directly.</p>

<h2>3. The dolphin.toml manifest</h2>
<p>When <code>dolphin.toml</code> exists at the project root, <code>dc check/build/run</code> discovers it upward from the given directory and drives the build from it:</p>
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
  <thead><tr><th>Key</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr><td><code>[package]</code></td><td><code>group:name:version</code> forms the coordinate; <code>source</code> is the source root (default <code>src</code>)</td></tr>
    <tr><td><code>[lib]</code></td><td>Declares the library target; <code>path</code> must be a direct child of <code>source</code>; at most one lib per package</td></tr>
    <tr><td><code>[[bin]]</code></td><td>Declares one or more executables, each with its own <code>main</code></td></tr>
    <tr><td><code>[repositories]</code></td><td>Maps repository IDs to base addresses (<code>http(s)://</code> or <code>file://</code>)</td></tr>
    <tr><td><code>[dependencies]</code></td><td>Maps private aliases to exact coordinates or <code>{ path = ... }</code></td></tr>
    <tr><td><code>[native.&lt;target&gt;]</code></td><td>Declares prebuilt objects / static-libs / shared-libs / runtime-files</td></tr>
    <tr><td><code>[build]</code></td><td><code>optimization</code> (debug/release) and the <code>output</code> directory</td></tr>
  </tbody>
</table>
<p>A package declares at least one of lib or <code>[[bin]]</code>, and may declare both. Multi-target projects need <code>--bin</code> to pick a target, and <code>run</code> on a pure library is explicitly rejected.</p>

<h2>4. Dependencies and library packages</h2>
<p>Dependencies are either local paths or Maven-style coordinates <code>group:name:version</code>. Libraries are distributed as deterministic <code>.dlib</code> (ZIP) archives containing a normalized manifest and the full library source:</p>
<pre><code>dc fetch my-project                 # resolve dependencies and write dolphin.lock
dc build my-project --locked        # require the lock to match, without rewriting
dc build my-project --offline       # never touch the network
dc package my-project               # produce target/package/&lt;name&gt;-&lt;version&gt;.dlib
dc publish my-project --repository default</code></pre>
<p>Resolution results are written to <code>dolphin.lock</code> at the root, which should be committed. <code>--locked</code> requires the lock to match the manifest, repositories and compiler version; <code>--offline</code> avoids HTTP(S). With remote dependencies, <code>dc</code> downloads over HTTPS into a content-addressed cache (<code>DOLPHIN_HOME</code>, <code>~/.dolphin</code> by default) and verifies SHA-256.</p>
<div class="callout">
  <p><strong>Cross-package generics:</strong> a library's generics are monomorphized once on the consumer side, keyed by <code>(package identity, qualified name, concrete type arguments)</code>, so same-named definitions never collide.</p>
</div>

<h2>5. Command-line reference</h2>
<pre><code>dc check &lt;dir-or-main.do&gt; [--locked] [--offline] [--color auto|always|never]
dc build &lt;dir-or-main.do&gt; [--bin &lt;name&gt;] [--lib] [-o &lt;file&gt;] [--debug|--release] [--system-linker|--bundled-linker] [--backend cranelift|llvm] [--locked] [--offline]
dc run   &lt;dir-or-main.do&gt; [--bin &lt;name&gt;] [-o &lt;file&gt;] [--debug|--release] [--system-linker|--bundled-linker] [--backend cranelift|llvm] [--locked] [--offline] [-- &lt;app args&gt;...]
dc test  &lt;dir&gt; [--filter &lt;text&gt;] [--debug|--release] [--system-linker|--bundled-linker] [--backend cranelift|llvm] [--locked] [--offline]
dc package &lt;dir&gt; [--locked] [--offline]
dc fetch   &lt;dir&gt; [--locked] [--offline]
dc publish &lt;dir&gt; [--repository &lt;id&gt;] [--locked] [--offline]
dc info &lt;dir&gt;
dc env
dc fmt &lt;file-or-dir...&gt; [--check]
dc lsp [project]</code></pre>
<p>Use <code>dc &lt;subcommand&gt; --help</code> for subcommand options. <code>--color</code> is global and may appear before or after the subcommand. Arguments after <code>--</code> in <code>dc run</code> are forwarded to the program unchanged, without shell processing. <code>dc test</code> runs each test in its own process; see <a href="#/tutorial/testing">testing your code</a>. <code>dc info</code> accepts only a project directory; <code>dc env</code> shows the host/target platform, ABI, selected linker and cache root.</p>
<p><code>dc check/build/run/test</code> print all lexical, syntax and declaration-level diagnostics at once (at most 100 per file) and refuse to emit objects or executables when any error is present. Without path arguments, <code>dc fmt</code> discovers <code>dolphin.toml</code> upward from the current directory, formats under <code>[package].source</code> (default <code>src</code>) while excluding <code>build.output</code> (default <code>target</code>) and <code>.git</code>, formats every selected file in memory first and writes nothing if any file fails; explicit file arguments still apply exactly. <code>dc lsp [project]</code> uses a side-effect-free shared analysis snapshot in project mode: documents with <code>pkg</code>/<code>use</code> get project semantic diagnostics, and hover/go-to-definition resolve by symbol identity across unsaved overlays and file/package boundaries; without a <code>dolphin.toml</code> it falls back to single-file analysis.</p>

<h2>6. A multi-target project</h2>
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
<pre><code>dc build tools            # build the lib and every bin
dc build tools --lib      # build the library and package it as a .dlib
dc run tools --bin cli    # run a specific executable target</code></pre>
<p><code>--lib</code> also produces <code>target/package/&lt;name&gt;-&lt;version&gt;.dlib</code>, and <code>.dlib</code> packaging rejects path dependencies. A lib+bin package that declares one therefore builds with <code>--bin</code>, or develops through <code>dc test</code>.</p>

<h2>Exercises</h2>
<ol>
  <li>Split the calculator from the previous chapter into <code>src/main.do</code> and <code>src/calc/eval.do</code> with correct <code>pkg</code> and <code>pub</code>.</li>
  <li>Write a <code>dolphin.toml</code> declaring one lib and two bins.</li>
  <li>Create a local library and depend on it with <code>{ path = "../..." }</code>, then call one of its generic functions.</li>
</ol>
`
    },

    {
      id: "tutorial/generics",
      title: "Generics, traits and the standard library",
      body: `
<h1>Generics, traits and the standard library</h1>
<p>Generic functions and types, methods with static trait dispatch, and the source standard library that ships with the compiler.</p>

<h2>1. Generic functions</h2>
<p>Generic parameters appear in angle brackets after the function name. Call sites may supply type arguments explicitly, or rely on inference from the arguments:</p>
<pre><code>fn identity&lt;T&gt;(value: T): T {
    return value;
}

fn main() {
    val a = identity&lt;i32&gt;(20);   // explicit
    val b = identity(22);        // inferred as i32
    println("{} {}", a, b);
}</code></pre>
<p>The compiler monomorphizes generics with a work queue: each <code>(package identity, qualified name, concrete type arguments)</code> is generated once, and uninstantiated templates emit no symbols. Instance chains are limited to depth 128 and 10000 total instances, with diagnostics instead of a panic. Templates may call templates and nest generic instances.</p>

<h2>2. Generic structs and enums</h2>
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
<p>A generic enum's payload may carry composite values such as <code>string</code> and structs. Struct self-reference must go through a pointer, for example <code>struct Node&lt;T&gt; { next: *Node&lt;T&gt; }</code>; a by-value cycle is reported as soon as it is instantiated.</p>

<h2>3. Methods and impl</h2>
<p>Use <code>impl</code> to define methods for a type. The receiver may be <code>self</code>, <code>self: *Self</code> or <code>self: *const Self</code>, and addressable objects are auto-addressed:</p>
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
<p>Generic types can have methods too:</p>
<pre><code>impl&lt;T&gt; Pair&lt;T&gt; {
    fn swapped(self): Pair&lt;T&gt; {
        return Pair&lt;T&gt;(self.second, self.first);
    }
}</code></pre>

<h2>4. Traits and associated types</h2>
<p>A trait declares a behavioral contract, and <code>impl Trait for Type</code> provides a statically dispatched implementation:</p>
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
<p>A type parameter may carry a single constraint <code>T: Trait</code>, and associated types are written <code>Self::Item</code> or <code>C::Item</code>:</p>
<pre><code>fn first_of&lt;T: Head&gt;(container: *const T): T::Item {
    return container.head();
}</code></pre>
<p>A missing constraint implementation, missing trait member, unknown or duplicate method, or a trait impl signature mismatch all produce source-level diagnostics. Multiple constraints, default methods and dynamic dispatch are not supported yet; same-named trait methods are rejected as duplicates.</p>

<h2>5. Standard library overview</h2>
<p>The standard library has two parts: compiler built-ins and source modules. <code>Option</code>, <code>Result</code> and <code>Iterator</code> are available automatically as a minimal prelude.</p>
<table>
  <thead><tr><th>Module</th><th>Contents</th></tr></thead>
  <tbody>
    <tr><td><code>std</code></td><td><code>Option&lt;T&gt;</code>, <code>Result&lt;T, E&gt;</code>, <code>Iterator</code></td></tr>
    <tr><td><code>std.mem</code></td><td>Built-in allocation, free, copy, layout queries and view construction</td></tr>
    <tr><td><code>std.collections</code></td><td><code>Vec&lt;T&gt;</code>, <code>SliceIter&lt;T&gt;</code>, <code>Range</code></td></tr>
    <tr><td><code>std.text</code></td><td><code>String</code>, <code>concat</code>, <code>trim</code>, <code>substring</code>, <code>from_utf8</code> and more</td></tr>
    <tr><td><code>std.ffi</code></td><td><code>CString</code></td></tr>
  </tbody>
</table>

<h2>6. Iteration and Vec</h2>
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
<p><code>for x in expr</code> requires <code>expr</code> to implement <code>Iterator</code>; arrays and slices adapt to <code>SliceIter&lt;T&gt;</code> over a read-only slice, ranges lower to <code>Range</code>, and <code>s.iter()</code> allocates nothing. Owned <code>Vec&lt;T&gt;</code> resources must be explicitly <code>deinit</code>ed; see the <a href="#/std/collections">standard library reference</a>.</p>

<h2>7. Text utilities</h2>
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

<h2>Exercises</h2>
<ol>
  <li>Write a generic <code>max_of&lt;T&gt;</code> function constrained by a comparison trait if feasible.</li>
  <li>Implement a trait for a custom struct and use that constraint in a generic function.</li>
  <li>Store data in a <code>Vec&lt;i32&gt;</code>, iterate it, and compute the average.</li>
</ol>
`
    },

    {
      id: "tutorial/memory",
      title: "Memory model and C interop",
      body: `
<h1>Memory model and C interop</h1>
<p>Dolphin uses a Zig-style explicit memory model: no GC, RC, automatic destruction, implicit move or borrow checking. Values are copied by default; pointers and slices copy only their descriptor. Allocation and free are explicit, and <code>defer</code> is the only scope cleanup syntax.</p>

<h2>1. Pointers, slices and integer widths</h2>
<table>
  <thead><tr><th>Form</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr><td><code>*T</code> / <code>*const T</code></td><td>Nullable writable / read-only raw pointer</td></tr>
    <tr><td><code>[]T</code> / <code>[]const T</code></td><td>Writable / read-only view <code>{ ptr, len }</code></td></tr>
    <tr><td><code>string</code></td><td>Read-only view of valid UTF-8 bytes; owns nothing</td></tr>
    <tr><td><code>usize</code> / <code>isize</code></td><td>Pointer-width integer (64-bit today)</td></tr>
  </tbody>
</table>
<ul>
  <li><code>&amp;var_local</code> yields <code>*T</code>; <code>&amp;val_local</code> and address-of on immutable parameters yield <code>*const T</code>. Address-of only applies to stable storage, temporaries are rejected, and indexed address-of is bounds-checked.</li>
  <li><code>[]T</code> coerces implicitly to <code>[]const T</code>, and <code>*T</code> to <code>*const T</code>; the reverse is a compile error.</li>
  <li><code>null</code> is only allowed where a pointer type is known. <code>.ptr</code> and <code>.len</code> are read-only slice fields, and a <code>string</code>'s <code>.len</code> is its byte count.</li>
</ul>

<h2>2. std.mem</h2>
<p><code>use std.mem;</code> exposes compiler built-ins without any on-disk library:</p>
<table>
  <thead><tr><th>API</th><th>Behavior and responsibility</th></tr></thead>
  <tbody>
    <tr><td><code>mem.alloc&lt;T&gt;(count): []T</code></td><td>Allocates uninitialized contiguous storage; write, then read, then hand it to <code>mem.free</code></td></tr>
    <tr><td><code>mem.free&lt;T&gt;(buffer: []T)</code></td><td>Frees only the full original slice; does not recursively free elements</td></tr>
    <tr><td><code>mem.create&lt;T&gt;(value): *T</code> / <code>mem.destroy&lt;T&gt;(ptr)</code></td><td>Allocate / free a single initialized object</td></tr>
    <tr><td><code>mem.size_of&lt;T&gt;()</code> / <code>mem.align_of&lt;T&gt;()</code></td><td>Compile-time layout constants</td></tr>
    <tr><td><code>mem.copy&lt;T&gt;(dst, src)</code></td><td>Equal-length by-value copy (<code>memmove</code> semantics); no allocation, no deep copy</td></tr>
    <tr><td><code>mem.is_valid_utf8(bytes): bool</code></td><td>Validates encoding only; no allocation, no trap</td></tr>
    <tr><td><code>mem.view&lt;T&gt;(ptr, len)</code> / <code>mem.view_const&lt;T&gt;(ptr, len)</code></td><td>Builds a view from a C pointer without taking ownership</td></tr>
    <tr><td><code>mem.cast_ptr&lt;T&gt;(ptr)</code> / <code>mem.cast_const_ptr&lt;T&gt;(ptr)</code></td><td>Explicitly converts between object pointers and <code>*Unit</code>, checking alignment</td></tr>
  </tbody>
</table>
<p>A sub-slice is written <code>buffer.slice(start, end)</code> and always checks <code>0 &lt;= start &lt;= end &lt;= len</code>.</p>

<h2>3. Allocate, use, free</h2>
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
<p><code>defer call_expression;</code> binds to the nearest lexical block and runs in reverse registration order when the block exits. Call arguments are evaluated at exit time, and names bind to the local's identity at registration, so the latest value is read:</p>
<pre><code>val buffer = mem.alloc&lt;u8&gt;(4_usize);
defer mem.free(buffer);   // freed when the block exits</code></pre>
<p>Falling out of a block, <code>return</code>, <code>break</code> and <code>continue</code> all clean up the scopes actually exited; <code>return expr</code> evaluates and saves the result before cleanup. Traps and OOM do not unwind the Dolphin stack, so <code>defer</code> does not run. Defer blocks, nested defer and resource <code>try</code> are not supported.</p>

<h2>5. String views</h2>
<pre><code>val text = "dolphin";
val bytes = text.bytes();          // []const u8, zero allocation
val again = string.from_bytes(bytes);
println("{}", again == text);      // true</code></pre>
<p>String literals are static views and must never be <code>free</code>d. Use <code>std.text.String</code> when you need an owned, growable string.</p>

<h2>6. C interop</h2>
<pre><code>extern struct CPoint { x: f64, y: f64 }

extern "C" {
    pub fn demo_add(a: i32, b: i32): i32;
    pub fn demo_create(): *Unit;
    pub fn demo_destroy(handle: *Unit);
    pub fn demo_translate(point: *CPoint, dx: f64): f64;
}</code></pre>
<ul>
  <li><code>c_int</code>, <code>c_uint</code>, <code>c_long</code>, <code>c_ulong</code> and <code>c_char</code> are platform-dependent built-in aliases.</li>
  <li>Extern functions import the original C symbol with no Dolphin mangling; <code>*Unit</code> represents C's <code>void*</code> and cannot be dereferenced.</li>
  <li>Passing <code>bool</code>, Dolphin <code>char</code>, <code>string</code>, slices, plain structs or enums by value in an extern signature is rejected; <code>extern struct</code> may only be passed by pointer to C.</li>
  <li><code>[native.&lt;triple&gt;]</code> in <code>dolphin.toml</code> declares <code>objects</code>, <code>static-libs</code>, <code>shared-libs</code> and <code>runtime-files</code>. <code>dc</code> consumes prebuilt C files; it does not compile C sources.</li>
</ul>

<h2>7. Runtime failures</h2>
<table>
  <thead><tr><th>Situation</th><th>Debug</th><th>Release</th></tr></thead>
  <tbody>
    <tr><td>Arithmetic trap, slice out of bounds, invalid slice range, null view / alignment check</td><td><code>101</code></td><td><code>101</code></td></tr>
    <tr><td>Allocation failure / allocation size overflow</td><td><code>102</code></td><td><code>102</code></td></tr>
    <tr><td>Invalid free: unknown or already-freed address, length mismatch</td><td><code>103</code></td><td>Not guaranteed</td></tr>
    <tr><td>UTF-8 validation failure</td><td><code>104</code></td><td><code>104</code></td></tr>
    <tr><td>Leak at normal exit</td><td>Reported on stderr, exit code preserved</td><td>Not tracked</td></tr>
  </tbody>
</table>
<p>Debug builds link an instrumented runtime with a live-allocation registry; Release links the plain one. Every failure terminates safely, with no undefined behavior.</p>

<h2>Exercises</h2>
<ol>
  <li>Allocate an array of structs with <code>mem.alloc</code>, fill it, sum a field, and free it with <code>defer</code>.</li>
  <li>Write a function that converts a <code>string</code> into a <code>CString</code> and handles interior NUL errors.</li>
  <li>Explain why <code>defer</code> does not run when the program terminates from a trap.</li>
</ol>
`
    },


    {
      id: "tutorial/testing",
      title: "Testing your code",
      body: `
<h1>Testing your code</h1>
<p>M19 adds <code>dc test</code>: it discovers functions in the project's <code>tests/</code> directory and runs each one in its own child process. It serves user projects; the compiler's own regression suite still runs through <code>cargo test</code>.</p>

<h2>1. Writing a test</h2>
<p>Test files are the direct children of <code>tests/</code> (subdirectories are ignored) and must omit <code>pkg</code>, because they compile as part of the package root module. A test is any function whose name starts with <code>test_</code>, takes no parameters and returns nothing:</p>
<pre><code>// tests/math.do
use std.test.expect;

fn test_addition() {
    expect(2 + 2 == 4);
}

fn test_division() {
    expect(10 / 2 == 5);
}</code></pre>
<p>Other functions in the same file are helpers and are not run as tests. Test files must not define <code>main</code>; <code>dc test</code> generates the entry point. Tests share the package root module, so they can call private functions from <code>src/*.do</code> and <code>pub</code> items from submodules.</p>

<h2>2. Assertions</h2>
<pre><code>use std.test.expect;
use std.test.fail;

fn test_assertions() {
    expect(1 + 1 == 2);
    if 1 + 1 != 2 {
        fail();
    }
}</code></pre>
<p><code>expect(false)</code> and <code>fail()</code> write <code>Dolphin test assertion failed</code> to stderr and exit with code <code>106</code>. Like runtime traps, an assertion failure does not run <code>defer</code> cleanup.</p>

<h2>3. Running tests</h2>
<pre><code>dc test .                  # discover and run every test (Debug by default)
dc test . --release        # build an optimized test binary
dc test . --filter math    # only tests whose name contains "math"</code></pre>
<p>Every test gets a fresh process and a fixed 30-second timeout; on timeout it is killed and the remaining tests still run. Output is one line per test plus a summary:</p>
<pre><code>test test_addition ... ok
test test_division ... ok
2 passed; 0 failed; 0 filtered out</code></pre>
<p>Failures are classified as <code>FAILED (assertion)</code>, <code>FAILED (trap exit 101)</code> or <code>FAILED (timeout after 30s)</code>. The command exits <code>0</code> when all selected tests pass and <code>1</code> otherwise. With no tests it prints <code>no tests found</code>, and when <code>--filter</code> matches nothing it prints <code>no tests matched filter</code>; both exit <code>1</code>. The test binary is written to <code>target/test/&lt;package&gt;-tests</code>.</p>

<h2>4. Library projects and path dependencies</h2>
<p>A package needs a <code>[lib]</code> target for <code>dc test</code>. The test build does not produce a <code>.dlib</code> and does not require publishability, so path dependencies resolve normally; this is the recommended development loop for a library. The <code>examples/m19</code> project in the repository uses it for both packages.</p>
`
    },

    {
      id: "tutorial/tour",
      title: "Putting it together: from beginner to mastery",
      body: `
<h1>Putting it together: from beginner to mastery</h1>
<p>This chapter combines everything so far into a complete program that uses generics, standard-library containers, pattern matching and explicit memory management: a small grade statistics tool.</p>

<h2>1. Requirements</h2>
<ul>
  <li>Define a struct representing a student record.</li>
  <li>Collect records dynamically with <code>Vec&lt;Record&gt;</code>.</li>
  <li>Represent query results with an enum and <code>match</code>.</li>
  <li>Compute the total, average and highest score.</li>
  <li>Free every owned resource with <code>defer</code>.</li>
</ul>

<h2>2. Project layout</h2>
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

<h2>3. The statistics module</h2>
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

<h2>4. The main program</h2>
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
<p>Build and run:</p>
<pre><code>dc check report
dc build report --release
./report/target/report
echo $?</code></pre>

<h2>5. Review</h2>
<table>
  <thead><tr><th>Feature used</th><th>Chapter</th></tr></thead>
  <tbody>
    <tr><td>Structs, fields, positional construction</td><td><a href="#/tutorial/composites">Arrays, structs, enums and match</a></td></tr>
    <tr><td>Enums and exhaustive <code>match</code></td><td><a href="#/tutorial/composites">Arrays, structs, enums and match</a></td></tr>
    <tr><td><code>pkg</code> / <code>use</code> / <code>pub</code></td><td><a href="#/tutorial/packages">Modules and packages</a></td></tr>
    <tr><td>Generic container <code>Vec&lt;T&gt;</code> and the iterator protocol</td><td><a href="#/tutorial/generics">Generics, traits and the standard library</a></td></tr>
    <tr><td>Pointer parameters and explicit free</td><td><a href="#/tutorial/memory">Memory model and C interop</a></td></tr>
  </tbody>
</table>

<h2>6. Going deeper</h2>
<ul>
  <li>Read the <a href="#/std/overview">standard library reference</a> for the complete <code>Vec</code>, <code>String</code>, <code>Option</code> and <code>Result</code> APIs.</li>
  <li>Read <a href="#/std/mem">std.mem</a> to understand layout and view construction, and <a href="#/std/io">process, streams and files</a> for the M19 I/O modules.</li>
  <li>Add tests for the stats module with <a href="#/tutorial/testing">dc test</a> and <code>std.test</code>.</li>
  <li>Wire the project into an editor: <code>dc lsp report</code> serves cross-file/package diagnostics, hover and go-to-definition (including unsaved text), and <code>dc fmt --check</code> fits CI.</li>
  <li>Extract the statistics logic into a reusable library and depend on it through <code>[lib]</code> and a path dependency.</li>
</ul>

<h2>Challenges</h2>
<ol>
  <li>Implement a generic <code>reduce&lt;T&gt;</code> that accumulates over an <code>Iterator</code> and handles the empty case.</li>
  <li>Generate a formatted text report with <code>std.text.String</code> and free its buffer.</li>
  <li>Call one of your own C functions through <code>extern "C"</code> and pass the statistics to it.</li>
</ol>
`
    }
  ]
});
