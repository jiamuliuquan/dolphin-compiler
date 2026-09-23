(function () {
  var SUPPORTED = ["zh-CN", "en-US"];
  var DEFAULT = "zh-CN";
  var STORAGE_KEY = "dolphin-lang";

  var UI = {
    "zh-CN": {
      "brand.name": "Dolphin",
      "brand.tagline": "编程语言",
      "nav.home": "首页",
      "nav.install": "安装",
      "nav.docs": "文档",
      "nav.tutorial": "教程",
      "nav.stdlib": "标准库",
      "nav.menu": "菜单",
      "nav.theme": "切换主题",
      "nav.language": "切换语言",

      "hero.eyebrow": "静态类型 · 编译为本机代码",
      "hero.title": "一门可以亲手写编译器的语言",
      "hero.lead":
        "Dolphin 是一门面向学习与实践的静态类型语言，语法参考 Rust、Kotlin、Java 和 C。编译器用 Rust 写成，默认通过 Cranelift 生成本机可执行文件，也可选用 LLVM 后端。",
      "hero.cta.install": "开始安装",
      "hero.cta.docs": "阅读文档",
      "hero.termtitle": "terminal",


      "why.eyebrow": "为什么选择 Dolphin",
      "why.title": "为理解编译器而设计",
      "why.lead":
        "从词法分析到本机代码生成的每个阶段都拆成独立 crate，源码标准库也用 Dolphin 写成，读语言和读实现可以同步进行。",
      "why.card1.title": "静态类型，错误前置",
      "why.card1.body":
        "完整的类型检查、控制流检查与返回路径检查，尽量在编译期发现错误，运行时只保留明确的陷阱。",
      "why.card2.title": "显式且安全的内存",
      "why.card2.body":
        "Zig 式手动内存模型：没有 GC、RC 与隐式析构，分配与释放显式可见，defer 是唯一的作用域清理语法。",
      "why.card3.title": "自包含工具链",
      "why.card3.body":
        "发行包内置预编译运行时与 rust-lld 链接器，解压即可构建；链接仍使用系统自带的 CRT/SDK。",
      "why.card4.title": "泛型与 trait",
      "why.card4.body":
        "用户泛型经工作队列单态化，trait 提供静态分派，标准库的 Vec、String、Option、Result 全部由 Dolphin 源码写成。",
      "why.card5.title": "模块与包管理",
      "why.card5.body":
        "多文件项目、pkg/use/pub 可见性，以及 dolphin.toml 清单、path/坐标依赖、确定性 .dlib 包与锁文件。",
      "why.card6.title": "面向学习",
      "why.card6.body":
        "仓库按里程碑提供可运行示例，并如实记录尚未实现的能力，方便按顺序阅读和扩展。",

      "preview.eyebrow": "语言预览",
      "preview.title": "简洁、明确的语法",
      "preview.lead":
        "函数、变量、控制流与模式匹配的写法接近 Rust 与 Kotlin，下面是一个完整的面积计算程序。",
      "preview.list.1": "val 与 var 区分不可变与可变绑定，支持局部类型推断。",
      "preview.list.2": "定长数组、切片、原始指针与显式内存分配。",
      "preview.list.3": "结构体、携带数据的枚举与穷尽式 match。",
      "preview.list.4": "泛型函数、泛型类型、方法与 trait 静态分派。",
      "preview.list.5": "内置格式化输出与 UTF-8 字符串处理。",
      "preview.link": "查看完整语言参考",

      "quickstart.eyebrow": "快速开始",
      "quickstart.title": "三步运行第一个程序",
      "quickstart.lead": "下载发行包并加入 PATH，然后编译运行任意 Dolphin 项目。",
      "quickstart.step1.title": "1. 获取编译器",
      "quickstart.step1.body": "从发行页下载对应平台的归档，解压到任意目录。",
      "quickstart.step2.title": "2. 加入 PATH",
      "quickstart.step2.body": "把解压目录加入 PATH，确保 dc 与 rust-lld 位于同一目录。",
      "quickstart.step3.title": "3. 测试并运行",
      "quickstart.step3.body": "dc test 运行库测试，dc build 构建应用；产物写入项目的 target/ 目录。",
      "quickstart.link": "阅读安装指南",

      "cta.title": "现在就开始使用 Dolphin",
      "cta.lead":
        "语言、标准库与工具链的源码都在仓库里，可以边用边读。",
      "cta.install": "安装 Dolphin",
      "cta.docs": "进入文档",

      "footer.product": "产品",
      "footer.docs": "文档",
      "footer.resources": "资源",
      "footer.download": "下载与安装",
      "footer.quickstart": "快速开始",
      "footer.tutorial": "入门到精通",
      "footer.stdlib": "标准库参考",
      "footer.language": "语言设计",
      "footer.cli": "命令行参考",
      "footer.examples": "可运行示例",
      "footer.issues": "问题反馈",
      "footer.license": "GPL-3.0 许可证",
      "footer.disclaimer": "Dolphin 是用于学习与实践的编程语言项目。",
      "footer.rights": "保留所有权利。",

      "install.eyebrow": "安装",
      "install.title": "安装 Dolphin",
      "install.lead":
        "Dolphin 以自包含发行包发布：解压后即可使用，不需要 Rust 或编译器源码；链接仍使用系统自带的 CRT/SDK。",
      "install.tabs.linux": "Linux",
      "install.tabs.macos": "macOS",
      "install.tabs.windows": "Windows",
      "install.requirements": "系统要求",
      "install.requirements.body":
        "Linux x86_64、macOS ARM64（Apple Silicon）与 Windows x86_64 为一级支持平台，均在 CI 中完成构建与端到端测试。",
      "install.step.download": "1. 下载归档",
      "install.step.verify": "2. 校验完整性",
      "install.step.extract": "3. 解压并加入 PATH",
      "install.step.verifyenv": "4. 验证安装",
      "install.package.title": "发行包内容",
      "install.package.body":
        "dc 与 rust-lld 必须位于同一目录；macOS 与 Linux 包还携带 rust-lld 依赖的 LLVM 动态库，请勿拆分。",
      "install.offline.title": "离线使用",
      "install.offline.body":
        "在不声明远程依赖时，dc check/build/run/test 不会访问网络。运行时目标文件内嵌于 dc，链接器为同目录的 rust-lld。使用 Maven 风格依赖时，dc 会通过 HTTPS 下载 .dlib 到内容寻址缓存（DOLPHIN_HOME，默认 ~/.dolphin），并写入 dolphin.lock。",
      "install.upgrade.title": "升级与卸载",
      "install.upgrade.body":
        "升级：用新版本覆盖旧目录，或解压到新目录后替换 PATH。卸载：删除发行目录并从 PATH 移除对应条目即可，Dolphin 不写注册表、不创建系统服务。",
      "install.notes.title": "说明",
      "install.notes.1": "构建程序时仍会动态链接操作系统自带组件（Linux glibc、macOS libSystem、Windows UCRT）。",
      "install.notes.2": "从源码构建编译器自身才需要 Rust 与本机 C 编译器。",
      "install.notes.3": "当前不支持交叉编译，编译器只生成宿主平台的本机程序。",
      "install.next": "安装完成后，继续阅读入门教程。",
      "install.next.btn": "进入入门教程",

      "docs.search": "搜索文档…",
      "docs.toc": "本页目录",
      "docs.prev": "上一页",
      "docs.next": "下一页",
      "docs.notfound": "没有找到对应的文档页面。",
      "docs.empty": "没有匹配的文档。",
      "docs.loading": "正在加载…"
    },

    "en-US": {
      "brand.name": "Dolphin",
      "brand.tagline": "Programming Language",
      "nav.home": "Home",
      "nav.install": "Install",
      "nav.docs": "Docs",
      "nav.tutorial": "Learn",
      "nav.stdlib": "Standard Library",
      "nav.menu": "Menu",
      "nav.theme": "Toggle theme",
      "nav.language": "Switch language",

      "hero.eyebrow": "Statically typed · Compiles to native code",
      "hero.title": "A language you can build a compiler for",
      "hero.lead":
        "Dolphin is a statically typed language for learning and practice, with syntax that draws on Rust, Kotlin, Java and C. Its Rust-based compiler emits native executables through Cranelift by default, with an optional LLVM backend.",
      "hero.cta.install": "Get started",
      "hero.cta.docs": "Read the docs",
      "hero.termtitle": "terminal",


      "why.eyebrow": "Why Dolphin",
      "why.title": "Designed for understanding compilers",
      "why.lead":
        "Every stage from lexing to native code generation lives in its own crate, and the standard library is written in Dolphin itself, so you can read the language and the implementation together.",
      "why.card1.title": "Static types, early errors",
      "why.card1.body":
        "Type checking, control-flow checking and return-path analysis catch mistakes at compile time, leaving only explicit traps at runtime.",
      "why.card2.title": "Explicit, safe memory",
      "why.card2.body":
        "A Zig-style manual memory model: no GC, RC or implicit destruction. Allocation is visible, and defer is the only scope cleanup syntax.",
      "why.card3.title": "Self-contained toolchain",
      "why.card3.body":
        "Release archives bundle a precompiled runtime and the rust-lld linker, so you can build right after unpacking. Linking still uses the system CRT/SDK.",
      "why.card4.title": "Generics and traits",
      "why.card4.body":
        "User generics are monomorphized with a work queue, traits give static dispatch, and Vec, String, Option and Result are all written in Dolphin.",
      "why.card5.title": "Modules and packages",
      "why.card5.body":
        "Multi-file projects, pkg/use/pub visibility, plus a dolphin.toml manifest, path/coordinate dependencies, deterministic .dlib packages and lock files.",
      "why.card6.title": "Built for learning",
      "why.card6.body":
        "The repository ships a runnable example per milestone and documents what is not implemented yet, so you can read and extend it in order.",

      "preview.eyebrow": "Language preview",
      "preview.title": "Concise, predictable syntax",
      "preview.lead":
        "Functions, variables, control flow and pattern matching follow Rust and Kotlin closely; the sample below is a complete program.",
      "preview.list.1": "val and var distinguish immutable from mutable bindings, with local type inference.",
      "preview.list.2": "Fixed-size arrays, slices, raw pointers and explicit allocation.",
      "preview.list.3": "Structs, data-carrying enums and exhaustive match.",
      "preview.list.4": "Generic functions, generic types, methods and static trait dispatch.",
      "preview.list.5": "Built-in formatted output and UTF-8 string handling.",
      "preview.link": "Read the full language reference",

      "quickstart.eyebrow": "Quickstart",
      "quickstart.title": "Run your first program in three steps",
      "quickstart.lead": "Download a release, add it to PATH, then build and run any Dolphin project.",
      "quickstart.step1.title": "1. Get the compiler",
      "quickstart.step1.body": "Download the archive for your platform from the releases page and unpack it anywhere.",
      "quickstart.step2.title": "2. Add it to PATH",
      "quickstart.step2.body": "Add the unpacked directory to PATH, keeping dc and rust-lld in the same folder.",
      "quickstart.step3.title": "3. Test and run",
      "quickstart.step3.body": "dc test runs the library tests and dc build compiles the app; artifacts land in that project's target/ folder.",
      "quickstart.link": "Read the installation guide",

      "cta.title": "Start using Dolphin today",
      "cta.lead":
        "The language, standard library and toolchain all live in one repository, ready to use and to read.",
      "cta.install": "Install Dolphin",
      "cta.docs": "Go to the docs",

      "footer.product": "Product",
      "footer.docs": "Documentation",
      "footer.resources": "Resources",
      "footer.download": "Download & install",
      "footer.quickstart": "Quickstart",
      "footer.tutorial": "Learn Dolphin",
      "footer.stdlib": "Standard library",
      "footer.language": "Language design",
      "footer.cli": "Command line",
      "footer.examples": "Examples",
      "footer.issues": "Report an issue",
      "footer.license": "GPL-3.0 License",
      "footer.disclaimer": "Dolphin is a programming language project for learning and practice.",
      "footer.rights": "All rights reserved.",

      "install.eyebrow": "Install",
      "install.title": "Install Dolphin",
      "install.lead":
        "Dolphin ships as a self-contained archive: unpack it and go, with no Rust toolchain or compiler checkout. Linking still uses the platform CRT/SDK.",
      "install.tabs.linux": "Linux",
      "install.tabs.macos": "macOS",
      "install.tabs.windows": "Windows",
      "install.requirements": "System requirements",
      "install.requirements.body":
        "Linux x86_64, macOS ARM64 (Apple Silicon) and Windows x86_64 are tier-1 platforms, all built and tested end to end in CI.",
      "install.step.download": "1. Download the archive",
      "install.step.verify": "2. Verify integrity",
      "install.step.extract": "3. Unpack and add to PATH",
      "install.step.verifyenv": "4. Verify the installation",
      "install.package.title": "What's in the archive",
      "install.package.body":
        "dc and rust-lld must stay in the same directory; macOS and Linux archives also ship the LLVM shared library that rust-lld needs, so keep them together.",
      "install.offline.title": "Offline use",
      "install.offline.body":
        "dc check/build/run/test never touch the network unless you declare remote dependencies. The runtime object is embedded in dc and the linker is the rust-lld beside it. With Maven-style dependencies, dc downloads .dlib files over HTTPS into a content-addressed cache (DOLPHIN_HOME, ~/.dolphin by default) and writes dolphin.lock.",
      "install.upgrade.title": "Upgrade and uninstall",
      "install.upgrade.body":
        "To upgrade, overwrite the old directory with a new release, or unpack elsewhere and repoint PATH. To uninstall, delete the archive directory and remove its PATH entry — Dolphin writes no registry keys and creates no services.",
      "install.notes.title": "Notes",
      "install.notes.1": "Built programs still link OS-provided components dynamically (glibc on Linux, libSystem on macOS, UCRT on Windows).",
      "install.notes.2": "Only building the compiler from source requires Rust and a native C compiler.",
      "install.notes.3": "Cross-compilation is not supported; the compiler emits host-native programs only.",
      "install.next": "Once installed, continue with the getting-started tutorial.",
      "install.next.btn": "Open the tutorial",

      "docs.search": "Search docs…",
      "docs.toc": "On this page",
      "docs.prev": "Previous",
      "docs.next": "Next",
      "docs.notfound": "No documentation page was found for that link.",
      "docs.empty": "No matching documents.",
      "docs.loading": "Loading…"
    }
  };

  function translate(lang, key) {
    var pack = UI[lang] || UI[DEFAULT];
    if (pack && Object.prototype.hasOwnProperty.call(pack, key)) return pack[key];
    if (Object.prototype.hasOwnProperty.call(UI[DEFAULT], key)) return UI[DEFAULT][key];
    return key;
  }

  function detect() {
    try {
      var saved = localStorage.getItem(STORAGE_KEY);
      if (saved && SUPPORTED.indexOf(saved) !== -1) return saved;
    } catch (e) {}
    var nav = (navigator.language || navigator.userLanguage || "").toLowerCase();
    if (nav.indexOf("zh") === 0) return "zh-CN";
    if (nav.indexOf("en") === 0) return "en-US";
    return DEFAULT;
  }

  var current = detect();

  function apply(lang) {
    document.documentElement.lang = lang;
    document.documentElement.setAttribute("data-lang", lang);
    document.querySelectorAll("[data-i18n]").forEach(function (el) {
      el.textContent = translate(lang, el.getAttribute("data-i18n"));
    });
    document.querySelectorAll("[data-i18n-html]").forEach(function (el) {
      el.innerHTML = translate(lang, el.getAttribute("data-i18n-html"));
    });
    document.querySelectorAll("[data-i18n-placeholder]").forEach(function (el) {
      el.setAttribute("placeholder", translate(lang, el.getAttribute("data-i18n-placeholder")));
    });
    document.querySelectorAll("[data-i18n-title]").forEach(function (el) {
      el.setAttribute("title", translate(lang, el.getAttribute("data-i18n-title")));
    });
    document.querySelectorAll("[data-i18n-aria]").forEach(function (el) {
      el.setAttribute("aria-label", translate(lang, el.getAttribute("data-i18n-aria")));
    });
    document.querySelectorAll("[data-lang-btn]").forEach(function (btn) {
      var active = btn.getAttribute("data-lang-btn") === lang;
      btn.classList.toggle("is-active", active);
      btn.setAttribute("aria-pressed", active ? "true" : "false");
    });
    document.dispatchEvent(new CustomEvent("dolphin:langchange", { detail: { lang: lang } }));
  }

  function setLang(lang) {
    if (SUPPORTED.indexOf(lang) === -1) lang = DEFAULT;
    current = lang;
    try {
      localStorage.setItem(STORAGE_KEY, lang);
    } catch (e) {}
    apply(lang);
  }

  window.DolphinI18n = {
    SUPPORTED: SUPPORTED,
    DEFAULT: DEFAULT,
    get lang() {
      return current;
    },
    t: function (key) {
      return translate(current, key);
    },
    setLang: setLang,
    apply: apply
  };
})();
