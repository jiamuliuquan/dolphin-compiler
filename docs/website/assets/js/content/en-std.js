window.DolphinDocsContent = window.DolphinDocsContent || {};
window.DolphinDocsContent["en-US"] = window.DolphinDocsContent["en-US"] || { groups: [] };
window.DolphinDocsContent["en-US"].groups.push({
  id: "std",
  title: "Standard Library",
  pages: [
    {
      id: "std/overview",
      title: "Standard library overview",
      body: `
<h1>Standard library overview</h1>
<p>Dolphin's standard library has two parts: compiler built-ins and the source modules shipped with the compiler. The built-ins provide the memory, layout, view, UTF-8 and C ABI foundation; containers and text protocols are implemented entirely in Dolphin source, so you can read and extend them.</p>

<h2>1. Modules</h2>
<table>
  <thead><tr><th>Module</th><th>Import</th><th>Contents</th></tr></thead>
  <tbody>
    <tr><td><code>std</code></td><td>Automatic prelude</td><td><code>Option&lt;T&gt;</code>, <code>Result&lt;T, E&gt;</code>, <code>Iterator</code></td></tr>
    <tr><td><code>std.mem</code></td><td><code>use std.mem;</code></td><td>Allocation, free, copy, layout queries, views and pointer casts (built-in)</td></tr>
    <tr><td><code>std.collections</code></td><td><code>use std.collections.Vec;</code></td><td><code>Vec&lt;T&gt;</code>, <code>SliceIter&lt;T&gt;</code>, <code>Range</code></td></tr>
    <tr><td><code>std.text</code></td><td><code>use std.text;</code></td><td><code>String</code>, <code>concat</code>, <code>trim</code>, <code>substring</code>, <code>from_utf8</code>, <code>starts_with</code> and more</td></tr>
    <tr><td><code>std.ffi</code></td><td><code>use std.ffi.CString;</code></td><td><code>CString</code>, <code>CStringError</code></td></tr>
  </tbody>
</table>
<div class="callout">
  <p><strong>Reserved namespace:</strong> <code>std</code> belongs to the standard library and cannot be occupied by user modules. Standard-library generics are monomorphized on the consumer side and only generated on first use.</p>
</div>

<h2>2. Minimal prelude</h2>
<p>These three protocols are available without imports:</p>
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
<p>A module-defined or explicitly imported type of the same name shadows the prelude entry. See <a href="#/std/prelude">Option, Result and Iterator</a>.</p>

<h2>3. Built-in functions and types</h2>
<table>
  <thead><tr><th>Name</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>print</code> / <code>println</code></td><td>Formatted output (built-ins that user functions cannot override)</td></tr>
    <tr><td><code>length(s)</code></td><td>UTF-8 byte count, typed <code>usize</code></td></tr>
    <tr><td><code>string.from_bytes(bytes)</code></td><td>Validates UTF-8 and returns a <code>string</code> view</td></tr>
    <tr><td><code>s.bytes()</code></td><td>Returns a read-only byte view <code>[]const u8</code></td></tr>
    <tr><td><code>mem.*</code></td><td>See <a href="#/std/mem">std.mem</a></td></tr>
  </tbody>
</table>
<p>For the complete list of built-ins, formatting rules and diagnostic codes, see <a href="#/std/builtins">Built-in functions and formatting</a>.</p>

<h2>4. Reading the API docs</h2>
<p>Signatures use Dolphin syntax. A receiver <code>self: *Self</code> means the method may mutate the object, while <code>self: *const Self</code> is read-only; addressable locals and method results are auto-addressed. Owned resources such as <code>Vec</code>, <code>String</code> and <code>CString</code> must be released explicitly with <code>deinit</code>.</p>
<pre><code>use std.collections.Vec;

fn main() {
    var values = Vec&lt;i32&gt;::init();
    defer values.deinit();     // explicit release

    values.push(1);
    println("len = {}", values.len());
}</code></pre>

<h2>5. Where the source lives</h2>
<p>The source standard library lives in <code>crates/dolphin-std/src/</code> in the compiler repository:</p>
<ul>
  <li><code>std.do</code>: <code>Option</code>, <code>Result</code>, <code>Iterator</code>.</li>
  <li><code>collections.do</code>: <code>Vec</code>, <code>SliceIter</code>, <code>Range</code>.</li>
  <li><code>text.do</code>: <code>String</code> and text utilities.</li>
  <li><code>ffi.do</code>: <code>CString</code>.</li>
</ul>
<p>These files are parsed together with user sources on every build under the reserved identity <code>PackageId::STD</code>, so the implementation is yours to read.</p>

<h2>6. Related chapters</h2>
<ul>
  <li><a href="#/std/prelude">Option, Result and Iterator</a></li>
  <li><a href="#/std/mem">std.mem memory API</a></li>
  <li><a href="#/std/collections">std.collections containers</a></li>
  <li><a href="#/std/text">std.text text processing</a></li>
  <li><a href="#/std/ffi">std.ffi C strings</a></li>
</ul>
`
    },

    {
      id: "std/prelude",
      title: "Option, Result and Iterator",
      body: `
<h1>Option, Result and Iterator</h1>
<p>These types are defined in the <code>std</code> module and are available automatically as a minimal prelude.</p>

<h2>1. Option&lt;T&gt;</h2>
<p><code>Option&lt;T&gt;</code> means "maybe a value". The language has no implicit <code>null</code>; use <code>Option</code> to express absence:</p>
<pre><code>enum Option&lt;T&gt; {
    Some(T),
    None,
}</code></pre>
<pre><code>val maybe: Option&lt;i32&gt; = Option.Some(42);

val text = match maybe {
    Option.Some(value) =&gt; "got a value",
    Option.None =&gt; "nothing",
};</code></pre>
<h3>Methods</h3>
<table>
  <thead><tr><th>Signature</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>is_some(self: *const Self): bool</code></td><td>Whether it is <code>Some</code></td></tr>
    <tr><td><code>is_none(self: *const Self): bool</code></td><td>Whether it is <code>None</code></td></tr>
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
<p><code>Result&lt;T, E&gt;</code> represents recoverable success or failure: <code>Ok</code> carries the value, <code>Err</code> carries the error.</p>
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
<h3>Methods</h3>
<table>
  <thead><tr><th>Signature</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>is_ok(self: *const Self): bool</code></td><td>Whether it is <code>Ok</code></td></tr>
    <tr><td><code>is_err(self: *const Self): bool</code></td><td>Whether it is <code>Err</code></td></tr>
  </tbody>
</table>
<div class="callout warn">
  <p><strong>Current limit:</strong> the language has no <code>?</code> propagation operator yet, so both branches must be handled explicitly with <code>match</code>.</p>
</div>

<h2>3. Iterator</h2>
<p><code>for x in expr</code> requires <code>expr</code> to implement the <code>Iterator</code> protocol:</p>
<pre><code>pub trait Iterator {
    type Item;
    fn next(self: *Self): Option&lt;Self::Item&gt;;
}</code></pre>
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
        println("{}", value);   // 3, 2, 1
    }
}</code></pre>
<h3>Built-in adaptations</h3>
<ul>
  <li>Arrays and slices adapt to <code>SliceIter&lt;T&gt;</code> over a read-only slice.</li>
  <li><code>start..end</code> / <code>start..=end</code> lower to <code>Range</code>.</li>
  <li><code>s.iter()</code> returns a slice iterator with zero allocation.</li>
  <li><code>break</code> / <code>continue</code> and per-iteration <code>defer</code> share one cleanup path.</li>
</ul>

<h2>4. Combining them</h2>
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
      title: "std.mem memory API",
      body: `
<h1>std.mem memory API</h1>
<p><code>std.mem</code> is a compiler built-in with no on-disk library. It provides explicit allocation, free, copy, layout queries and view/pointer casts. Every API requires an explicit type argument, for example <code>mem.alloc&lt;i32&gt;(n)</code>.</p>

<h2>1. Allocation and free</h2>
<table>
  <thead><tr><th>API</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>mem.alloc&lt;T&gt;(count: usize): []T</code></td><td>Allocates <code>count</code> uninitialized <code>T</code> values contiguously. Returns a writable slice; the caller initializes it and later passes it to <code>mem.free</code>.</td></tr>
    <tr><td><code>mem.free&lt;T&gt;(buffer: []T)</code></td><td>Frees the full original slice returned by <code>alloc</code>. Does not free elements or deep-copy.</td></tr>
    <tr><td><code>mem.create&lt;T&gt;(value: T): *T</code></td><td>Allocates and initializes a single object, returning a pointer.</td></tr>
    <tr><td><code>mem.destroy&lt;T&gt;(ptr: *T)</code></td><td>Frees an object created with <code>create</code>.</td></tr>
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
  <p><strong>Freeing rules:</strong> only the full slice returned by <code>alloc</code> may be freed. Never free a sub-slice or a string literal. Length mismatches, unknown addresses and double frees produce runtime diagnostics (see below).</p>
</div>

<h2>2. Layout constants</h2>
<table>
  <thead><tr><th>API</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>mem.size_of&lt;T&gt;(): usize</code></td><td>Size of <code>T</code> in bytes, a compile-time constant</td></tr>
    <tr><td><code>mem.align_of&lt;T&gt;(): usize</code></td><td>Alignment of <code>T</code>, a compile-time constant</td></tr>
  </tbody>
</table>
<pre><code>struct Point { x: f64, y: f64 }

println("size = {}, align = {}", mem.size_of&lt;Point&gt;(), mem.align_of&lt;Point&gt;());</code></pre>

<h2>3. Copy and views</h2>
<table>
  <thead><tr><th>API</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>mem.copy&lt;T&gt;(dst: []T, src: []const T)</code></td><td>Equal-length by-value copy with <code>memmove</code> semantics, allowing overlap; no allocation, no deep copy</td></tr>
    <tr><td><code>mem.is_valid_utf8(bytes: []const u8): bool</code></td><td>Validates UTF-8 only; no allocation, no trap</td></tr>
    <tr><td><code>mem.view&lt;T&gt;(ptr: *T, len: usize): []T</code></td><td>Builds a writable view from a C pointer without taking ownership</td></tr>
    <tr><td><code>mem.view_const&lt;T&gt;(ptr: *const T, len: usize): []const T</code></td><td>Builds a read-only view from a C pointer</td></tr>
    <tr><td><code>mem.cast_ptr&lt;T&gt;(ptr: *Unit): *T</code></td><td>Casts <code>void*</code> to an object pointer with an alignment check</td></tr>
    <tr><td><code>mem.cast_const_ptr&lt;T&gt;(ptr: *const Unit): *const T</code></td><td>Casts <code>const void*</code> to a read-only object pointer with an alignment check</td></tr>
  </tbody>
</table>
<pre><code>val source = [1, 2, 3];
val destination = mem.alloc&lt;i32&gt;(3_usize);
defer mem.free(destination);
mem.copy&lt;i32&gt;(destination, source);</code></pre>

<h2>4. Sub-slices</h2>
<p>Calling <code>slice(start, end)</code> on a slice yields a sub-view and always checks <code>0 &lt;= start &lt;= end &lt;= len</code>:</p>
<pre><code>val buffer = mem.alloc&lt;u8&gt;(8_usize);
defer mem.free(buffer);
val middle = buffer.slice(2_usize, 6_usize);
println("middle len = {}", middle.len);</code></pre>
<p>A sub-view does not own memory and cannot be freed on its own. Slice fields <code>.ptr</code> and <code>.len</code> are read-only; a <code>string</code>'s <code>.len</code> is its UTF-8 byte count.</p>

<h2>5. defer and ownership</h2>
<p><code>defer</code> is the only scope cleanup syntax and runs in reverse registration order at block exit. Call arguments are evaluated at exit time, and names bind to the local's identity at registration:</p>
<pre><code>fn process() {
    val buffer = mem.alloc&lt;u32&gt;(16_usize);
    defer mem.free(buffer);
    // ... use buffer; freed automatically on return
}</code></pre>
<p><code>return</code>, <code>break</code> and <code>continue</code> all clean up the scopes actually exited; <code>return expr</code> evaluates and saves the result before cleanup. Traps and OOM do not unwind the Dolphin stack, so <code>defer</code> does not run.</p>

<h2>6. Failure modes</h2>
<table>
  <thead><tr><th>Exit code</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr><td><code>101</code></td><td>Arithmetic trap, slice out of bounds, invalid slice range, <code>null</code> view / alignment check failure</td></tr>
    <tr><td><code>102</code></td><td>Allocation failure / allocation size overflow</td></tr>
    <tr><td><code>103</code></td><td>Invalid free (checked in Debug; not guaranteed in Release)</td></tr>
    <tr><td><code>104</code></td><td>UTF-8 validation failure</td></tr>
  </tbody>
</table>
<p>Debug builds link an instrumented runtime with a live-allocation registry and report leaks on stderr at a normal exit while preserving the exit code.</p>

<h2>7. Relationship to source modules</h2>
<p><code>std.mem</code> provides only the foundation. Owned containers such as <code>Vec&lt;T&gt;</code>, <code>String</code> and <code>CString</code> are implemented in Dolphin source on top of it and call <code>mem.alloc</code> / <code>mem.free</code> explicitly. Reading their sources is a good way to learn how to build safe abstractions.</p>
`
    },

    {
      id: "std/collections",
      title: "std.collections containers",
      body: `
<h1>std.collections containers</h1>
<p><code>std.collections</code> provides the growable array <code>Vec&lt;T&gt;</code> plus the slice and range iterators <code>SliceIter&lt;T&gt;</code> and <code>Range</code>.</p>
<pre><code>use std.collections.Vec;</code></pre>

<h2>1. Vec&lt;T&gt;</h2>
<p><code>Vec&lt;T&gt;</code> is a growable, owning contiguous array. It owns its backing buffer and must be explicitly <code>deinit</code>ed.</p>
<pre><code>pub struct Vec&lt;T&gt; {
    storage: []T,
    used: usize,
}</code></pre>

<h3>Construction</h3>
<table>
  <thead><tr><th>Signature</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>Vec&lt;T&gt;::init(): Vec&lt;T&gt;</code></td><td>Creates an empty vector with zero capacity</td></tr>
    <tr><td><code>Vec&lt;T&gt;::with_capacity(capacity: usize): Vec&lt;T&gt;</code></td><td>Pre-allocates room for at least <code>capacity</code> elements</td></tr>
  </tbody>
</table>

<h3>Capacity and length</h3>
<table>
  <thead><tr><th>Signature</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>len(self: *const Self): usize</code></td><td>Number of used elements</td></tr>
    <tr><td><code>capacity(self: *const Self): usize</code></td><td>Elements the current buffer can hold</td></tr>
    <tr><td><code>is_empty(self: *const Self): bool</code></td><td>Whether the vector is empty</td></tr>
    <tr><td><code>reserve(self: *Self, additional: usize)</code></td><td>Ensures room for at least <code>additional</code> more elements, growing as needed</td></tr>
  </tbody>
</table>

<h3>Element access and mutation</h3>
<table>
  <thead><tr><th>Signature</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>push(self: *Self, value: T)</code></td><td>Appends an element, growing when needed (starts at 4, then doubles)</td></tr>
    <tr><td><code>pop(self: *Self): Option&lt;T&gt;</code></td><td>Removes and returns the last element, or <code>None</code> when empty</td></tr>
    <tr><td><code>get(self: *const Self, index: usize): Option&lt;T&gt;</code></td><td>Reads a copy by index; out of bounds returns <code>None</code></td></tr>
    <tr><td><code>set(self: *Self, index: usize, value: T)</code></td><td>Writes by index; out of bounds is checked at runtime</td></tr>
    <tr><td><code>as_slice(self: *const Self): []const T</code></td><td>Read-only view over the used elements</td></tr>
    <tr><td><code>as_mut_slice(self: *Self): []T</code></td><td>Writable view over the used elements</td></tr>
  </tbody>
</table>

<h3>Iteration, copy and cleanup</h3>
<table>
  <thead><tr><th>Signature</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>iter(self: *const Self): SliceIter&lt;T&gt;</code></td><td>Zero-allocation iterator, usable directly in <code>for</code></td></tr>
    <tr><td><code>clone(self: *const Self): Vec&lt;T&gt;</code></td><td>Shallow-copies all used elements into a new owning vector</td></tr>
    <tr><td><code>clear(self: *Self)</code></td><td>Removes all elements, keeping capacity</td></tr>
    <tr><td><code>deinit(self: *Self)</code></td><td>Frees the backing buffer and resets to empty</td></tr>
  </tbody>
</table>
<div class="callout warn">
  <p><strong>Growth and overflow:</strong> when capacity or element count approaches the <code>usize</code> limit, the growth path terminates safely with <code>102</code> instead of wrapping.</p>
</div>

<h3>Complete example</h3>
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
<p>A read-only slice iterator produced by arrays, slices and <code>Vec::iter()</code>.</p>
<pre><code>pub struct SliceIter&lt;T&gt; {
    storage: []const T,
    index: usize,
}</code></pre>
<table>
  <thead><tr><th>Signature</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>SliceIter&lt;T&gt;::init(storage: []const T)</code></td><td>Creates an iterator from a read-only slice</td></tr>
    <tr><td><code>len(self: *const Self): usize</code></td><td>Remaining element count</td></tr>
    <tr><td><code>is_empty(self: *const Self): bool</code></td><td>Whether it is exhausted</td></tr>
    <tr><td><code>next(self: *Self): Option&lt;T&gt;</code></td><td><code>Iterator</code> implementation yielding element copies</td></tr>
  </tbody>
</table>

<h2>3. Range</h2>
<p>An integer range iterator produced by <code>start..end</code> and <code>start..=end</code>.</p>
<table>
  <thead><tr><th>Signature</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>Range::exclusive(start: i32, end: i32): Range</code></td><td>Half-open range <code>start..end</code></td></tr>
    <tr><td><code>Range::inclusive(start: i32, end: i32): Range</code></td><td>Inclusive range <code>start..=end</code></td></tr>
    <tr><td><code>next(self: *Self): Option&lt;i32&gt;</code></td><td><code>Iterator</code> implementation that handles endpoints safely and never wraps past <code>i32::MAX</code></td></tr>
  </tbody>
</table>
<pre><code>for i in 0..=2 {
    println("{}", i);   // 0, 1, 2
}</code></pre>
`
    },

    {
      id: "std/text",
      title: "std.text text processing",
      body: `
<h1>std.text text processing</h1>
<p><code>std.text</code> provides the owned UTF-8 string <code>String</code> and a set of zero-allocation text utilities. The built-in <code>string</code> is a read-only view; <code>String</code> is a releasable owning buffer.</p>
<pre><code>use std.text;</code></pre>

<h2>1. Error type</h2>
<pre><code>pub enum TextError {
    InvalidUtf8,
    InvalidBoundary,
    OutOfBounds,
}</code></pre>

<h2>2. String</h2>
<pre><code>pub struct String {
    bytes: []u8,
}</code></pre>
<table>
  <thead><tr><th>Signature</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>String::from(s: string): String</code></td><td>Copies the contents of a <code>string</code> view into an owning buffer</td></tr>
    <tr><td><code>view(self: *const Self): string</code></td><td>Returns a zero-allocation read-only view</td></tr>
    <tr><td><code>clone(self: *const Self): String</code></td><td>Deep-copies into a new owning string</td></tr>
    <tr><td><code>deinit(self: *Self)</code></td><td>Frees the backing buffer</td></tr>
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

<h2>3. Text functions</h2>
<table>
  <thead><tr><th>Signature</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>concat(a: string, b: string): String</code></td><td>Joins two strings into a new owning string (overflow takes the <code>102</code> path)</td></tr>
    <tr><td><code>trim(s: string): string</code></td><td>Trims leading and trailing ASCII whitespace, returning a view over the same buffer</td></tr>
    <tr><td><code>substring(s: string, start: usize, end: usize): Result&lt;string, TextError&gt;</code></td><td>Byte-range sub-string; out of range yields <code>OutOfBounds</code>, cutting a code point yields <code>InvalidBoundary</code></td></tr>
    <tr><td><code>from_utf8(bytes: []const u8): Result&lt;string, TextError&gt;</code></td><td>Validates UTF-8 and returns a view; failure yields <code>InvalidUtf8</code></td></tr>
    <tr><td><code>starts_with(s: string, part: string): bool</code></td><td>Whether <code>s</code> starts with <code>part</code></td></tr>
    <tr><td><code>ends_with(s: string, part: string): bool</code></td><td>Whether <code>s</code> ends with <code>part</code></td></tr>
    <tr><td><code>contains(s: string, part: string): bool</code></td><td>Whether <code>s</code> contains <code>part</code> (empty needle returns <code>false</code>)</td></tr>
  </tbody>
</table>

<h3>Example</h3>
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
  <p><strong>Bytes vs. characters:</strong> both <code>length</code> and range arguments are in UTF-8 bytes. <code>substring</code> checks that boundaries fall on code-point starts so it can never produce invalid UTF-8.</p>
</div>

<h2>4. Interop with byte views</h2>
<pre><code>val text_bytes = "ok".bytes();              // []const u8
val parsed = text.from_utf8(text_bytes);
val is_ok = match parsed {
    Result.Ok(value) =&gt; true,
    Result.Err(error) =&gt; false,
};</code></pre>
`
    },

    {
      id: "std/ffi",
      title: "std.ffi C strings",
      body: `
<h1>std.ffi C strings</h1>
<p><code>std.ffi</code> provides the owning, NUL-terminated C string <code>CString</code> for passing Dolphin strings to C functions.</p>
<pre><code>use std.ffi.CString;</code></pre>

<h2>1. Types</h2>
<pre><code>pub enum CStringError {
    InteriorNul,
}

pub struct CString {
    bytes: []u8,
}</code></pre>
<p><code>CString</code> owns its buffer and must be released with <code>deinit</code>. The pointer returned by <code>ptr()</code> is valid only while the <code>CString</code> is alive.</p>

<h2>2. API</h2>
<table>
  <thead><tr><th>Signature</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>CString::empty(): CString</code></td><td>Creates an empty C string containing only the terminating NUL</td></tr>
    <tr><td><code>CString::from(s: string): Result&lt;CString, CStringError&gt;</code></td><td>Copies the contents and appends NUL; if <code>s</code> contains an interior NUL, returns <code>InteriorNul</code></td></tr>
    <tr><td><code>ptr(self: *const Self): *const c_char</code></td><td>Returns a pointer to the underlying NUL-terminated buffer for C</td></tr>
    <tr><td><code>deinit(self: *Self)</code></td><td>Frees the backing buffer</td></tr>
  </tbody>
</table>

<h2>3. Example</h2>
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
  <p><strong>Lifetime:</strong> after <code>deinit</code> the pointer from <code>ptr()</code> dangles. Make sure C never holds it beyond the <code>CString</code>'s lifetime.</p>
</div>

<h2>4. Handling interior NUL</h2>
<pre><code>val bad = CString::from("a\\u{0}b");
var handled = match bad {
    Result.Ok(value) =&gt; value,
    Result.Err(error) =&gt; CString::empty(),
};
defer handled.deinit();</code></pre>

<h2>5. Related chapters</h2>
<ul>
  <li><a href="#/tutorial/memory">Memory model and C interop</a>: <code>extern "C"</code>, platform aliases such as <code>c_char</code>, and native linking.</li>
  <li><a href="#/std/mem">std.mem</a>: pointer casts such as <code>mem.view</code> and <code>mem.cast_const_ptr</code>.</li>
</ul>
`
    },

    {
      id: "std/builtins",
      title: "Built-in functions and formatting",
      body: `
<h1>Built-in functions and formatting</h1>
<p>The names below are provided directly by the compiler. They do not come from the source standard library and cannot be overridden by user definitions.</p>

<h2>1. Output</h2>
<table>
  <thead><tr><th>Function</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>print(...)</code></td><td>Writes without a trailing newline; with no arguments it writes nothing</td></tr>
    <tr><td><code>println(...)</code></td><td>Writes with a trailing newline; with no arguments it writes only a newline</td></tr>
  </tbody>
</table>
<p>Both may only be called as standalone statements. The first argument must currently be a string literal.</p>
<pre><code>print("Hello, ");
println("{}!", "Dolphin");
println();                 // newline only
println("{} + {} = {}", 1, 2, 3);</code></pre>

<h2>2. Placeholders and braces</h2>
<p>Each <code>{}</code> consumes one argument, evaluated left to right. The number of arguments must match the number of placeholders or you get a compile error.</p>
<pre><code>println("{} {}", 1);      // compile error: missing argument
println("{}", 1, 2);      // compile error: too many arguments
println("{{}}");          // prints {}</code></pre>
<p>All integers, floats, <code>char</code>, <code>bool</code> and <code>string</code> can be formatted. Whole arrays cannot be formatted directly, but their elements can.</p>
<pre><code>val samples = [10, 20, 30];
println("first = {}", samples[0]);</code></pre>

<h2>3. String built-ins</h2>
<table>
  <thead><tr><th>Name</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>length(s: string): usize</code></td><td>UTF-8 byte length</td></tr>
    <tr><td><code>s.bytes(): []const u8</code></td><td>Read-only byte view with zero allocation</td></tr>
    <tr><td><code>string.from_bytes(bytes: []const u8): string</code></td><td>Validates UTF-8 and returns a <code>string</code> view; invalid UTF-8 terminates with <code>104</code></td></tr>
  </tbody>
</table>
<pre><code>val greeting = "海豚";
println("bytes = {}", length(greeting));
val again = string.from_bytes(greeting.bytes());
println("{}", again == greeting);</code></pre>
<p>Strings compare by byte content with <code>==</code> / <code>!=</code>. There is currently no string concatenation or character indexing.</p>

<h2>4. Operators and conversions</h2>
<table>
  <thead><tr><th>Category</th><th>Operators</th></tr></thead>
  <tbody>
    <tr><td>Arithmetic</td><td><code>+</code> <code>-</code> <code>*</code> <code>/</code> <code>%</code> (<code>%</code> integers only)</td></tr>
    <tr><td>Comparison</td><td><code>&lt;</code> <code>&lt;=</code> <code>&gt;</code> <code>&gt;=</code> (currently <code>i32</code> only)</td></tr>
    <tr><td>Equality</td><td><code>==</code> <code>!=</code> (<code>i32</code>, <code>bool</code>, <code>string</code> and more)</td></tr>
    <tr><td>Logic</td><td><code>!</code> <code>&amp;&amp;</code> <code>||</code> (short-circuiting)</td></tr>
    <tr><td>Conversion</td><td><code>as</code> (narrowing keeps low bits, widening sign/zero-extends, float-to-int saturates)</td></tr>
  </tbody>
</table>
<p>Unsuffixed integer literals default to <code>i32</code> and unsuffixed float literals to <code>f64</code>. Literal suffixes use an underscore, for example <code>10_i64</code> and <code>1.5_f32</code>.</p>

<h2>5. Runtime errors and diagnostics</h2>
<p>The following terminate safely rather than producing undefined behavior:</p>
<table>
  <thead><tr><th>Exit code</th><th>Situation</th></tr></thead>
  <tbody>
    <tr><td><code>101</code></td><td>Integer overflow, division by zero, modulo by zero, array/slice out of bounds, null view or alignment check failure</td></tr>
    <tr><td><code>102</code></td><td>Allocation failure or allocation size overflow</td></tr>
    <tr><td><code>103</code></td><td>Invalid free (checked in Debug)</td></tr>
    <tr><td><code>104</code></td><td>UTF-8 validation failure</td></tr>
  </tbody>
</table>
<p>Compile diagnostics carry stable categories <code>E0000</code> / <code>E0001</code>, file paths, Unicode character columns and multi-line source markers. The CLI supports <code>--color auto|always|never</code>. The compiler currently stops at the first error.</p>

<h2>6. Platform-dependent aliases</h2>
<p>These built-in aliases are available for C interop: <code>c_int</code>, <code>c_uint</code>, <code>c_long</code>, <code>c_ulong</code> and <code>c_char</code>. They vary by target; for example, <code>c_long</code> is 32-bit on Windows.</p>
`
    }
  ]
});
