//! Dolphin 语言服务器（M17；M20/H20-03 升级为项目级分析）。
//!
//! 在标准输入/输出上提供最小 LSP：结构化诊断、文档符号、悬停与跳转定义。
//! 项目模式下使用 `dolphin-analysis` 的共享分析快照与符号索引（overlay、跨文件与
//! 跨包定义、局部遮蔽），不做文本同名猜测；无清单时按单文件规则分析。

use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use dolphin_analysis::{
    AnalysisHost, AnalysisMode, AnalysisSnapshot, DefKind, Resolution, SingleFileAnalysis,
    SymbolId, SymbolIndex, analyze_single_file, path_to_uri, uri_to_path,
};
use dolphin_source::diagnostic::{Diagnostic, Label, Severity};
use dolphin_source::lexer::lex;
use dolphin_source::source::{SourceFile, SourceId, SourceMap, Span};
use dolphin_source::token::{Token, TokenKind};
use dolphin_syntax::ast;
use dolphin_syntax::parser::parse;
use serde_json::{Value, json};

// LSP `SymbolKind` 取值。
const SYMBOL_ENUM: u32 = 10;
const SYMBOL_INTERFACE: u32 = 11;
const SYMBOL_FUNCTION: u32 = 12;
const SYMBOL_OBJECT: u32 = 19;
const SYMBOL_STRUCT: u32 = 23;

/// 单条消息的字节上限，避免被伪造的 `Content-Length` 触发超大分配。
const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;

/// 在 stdin/stdout 上运行语言服务器，直到客户端发送 `exit`。
///
/// 退出码按 §7.2：已 `shutdown` 后 `exit` 或 EOF 为 0；未 `shutdown` 的 `exit` 为 1。
pub fn serve(project: Option<PathBuf>) -> io::Result<ExitCode> {
    let root = project.unwrap_or_else(|| PathBuf::from("."));
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = io::BufReader::new(stdin.lock());
    let mut writer = io::BufWriter::new(stdout.lock());
    let mut server = Server::with_root(root);

    while let Some(body) = read_message(&mut reader)? {
        let Ok(message) = serde_json::from_slice::<Value>(&body) else {
            continue;
        };
        for output in server.handle(message) {
            write_message(&mut writer, &output)?;
        }
        writer.flush()?;
        if server.exited {
            break;
        }
    }
    Ok(server.exit_code())
}

fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut content_length = None;
    let mut saw_header = false;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            // 消息之间可能残留空行；只有在已读到 header 后空行才结束头部。
            if saw_header {
                break;
            }
            continue;
        }
        saw_header = true;
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().ok();
        }
    }

    let length = content_length.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length header")
    })?;
    if length > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Content-Length {length} exceeds the {MAX_MESSAGE_BYTES} byte limit"),
        ));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}

fn write_message(writer: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)
}

/// 协议处理器：不依赖标准输入/输出，便于单元测试。
pub struct Server {
    host: AnalysisHost,
    /// 打开的文档，按 URI 字典序排列（发布顺序确定性，§7.3 规则 2）。
    documents: BTreeMap<String, OpenDocument>,
    initialized: bool,
    shutdown: bool,
    /// 收到 `exit` 后置位，由传输循环负责终止。
    pub exited: bool,
    last_project_message: Option<String>,
}

/// 一个打开文档的未保存文本、版本与上次发布的诊断。
struct OpenDocument {
    uri: String,
    path: Option<PathBuf>,
    version: u64,
    text: String,
    last_published: Option<Vec<Value>>,
    single_file: Option<(u64, Arc<SingleFileAnalysis>)>,
}

impl OpenDocument {
    /// 该文档当前版本的单文件分析；同版本重复查询复用缓存。
    fn single_file(&mut self) -> Arc<SingleFileAnalysis> {
        if let Some((version, analysis)) = &self.single_file
            && *version == self.version
        {
            return analysis.clone();
        }
        let path = self
            .path
            .clone()
            .unwrap_or_else(|| PathBuf::from(&self.uri));
        let source = SourceFile::with_id(SourceId(0), path, self.text.clone());
        let analysis = Arc::new(analyze_single_file(&source));
        self.single_file = Some((self.version, analysis.clone()));
        analysis
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

impl Server {
    /// 默认项目根为当前目录。
    pub fn new() -> Self {
        Self::with_root(PathBuf::from("."))
    }

    pub fn with_root(root: PathBuf) -> Self {
        Server {
            host: AnalysisHost::new(root),
            documents: BTreeMap::new(),
            initialized: false,
            shutdown: false,
            exited: false,
            last_project_message: None,
        }
    }

    /// 传输循环终止后使用的退出码（§7.2）。
    pub fn exit_code(&self) -> ExitCode {
        if self.exited && !self.shutdown {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        }
    }

    /// 处理一条 JSON-RPC 消息，返回需要写回的响应与通知（按顺序）。
    pub fn handle(&mut self, message: Value) -> Vec<Value> {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let id = message.get("id").cloned();
        let params = message.get("params").cloned().unwrap_or(Value::Null);

        if method == "exit" {
            self.exited = true;
            return Vec::new();
        }
        if !self.initialized {
            return match method.as_str() {
                "initialize" => {
                    self.initialized = true;
                    vec![response(id, capabilities())]
                }
                _ if id.is_some() => vec![error_response(id, -32002, "Server not initialized")],
                _ => Vec::new(),
            };
        }
        if self.shutdown {
            return if id.is_some() {
                vec![error_response(id, -32600, "Invalid Request")]
            } else {
                Vec::new()
            };
        }

        match method.as_str() {
            "initialize" => vec![response(id, capabilities())],
            "initialized" => Vec::new(),
            "shutdown" => {
                self.shutdown = true;
                vec![response(id, Value::Null)]
            }
            "textDocument/didOpen" => self.did_open(&params),
            "textDocument/didChange" => self.did_change(&params),
            "textDocument/didClose" => self.did_close(&params),
            "textDocument/documentSymbol" => vec![response(id, self.document_symbol(&params))],
            "textDocument/hover" => vec![response(id, self.hover(&params))],
            "textDocument/definition" => vec![response(id, self.definition(&params))],
            _ => {
                if id.is_some() {
                    vec![error_response(id, -32601, "Method not found")]
                } else {
                    Vec::new()
                }
            }
        }
    }

    fn did_open(&mut self, params: &Value) -> Vec<Value> {
        let Some(document) = params.get("textDocument") else {
            return Vec::new();
        };
        let uri = document
            .get("uri")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let text = document
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let version = document.get("version").and_then(Value::as_u64).unwrap_or(0);
        let path = uri_to_path(&uri);
        if let Some(path) = &path {
            let accepted = self.host.set_overlay(path.clone(), version, text.clone());
            if !accepted && self.documents.contains_key(&uri) {
                // 重复打开且 overlay 版本不更新：忽略，保持最新文本。
                return Vec::new();
            }
        }
        self.documents.insert(
            uri.clone(),
            OpenDocument {
                uri,
                path,
                version,
                text,
                last_published: None,
                single_file: None,
            },
        );
        self.publish_all()
    }

    fn did_change(&mut self, params: &Value) -> Vec<Value> {
        let Some(document) = params.get("textDocument") else {
            return Vec::new();
        };
        let uri = document
            .get("uri")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let version = document.get("version").and_then(Value::as_u64).unwrap_or(0);
        let Some(text) = params
            .get("contentChanges")
            .and_then(Value::as_array)
            .and_then(|changes| changes.last())
            .and_then(|change| change.get("text"))
            .and_then(Value::as_str)
        else {
            return Vec::new();
        };
        let Some(current) = self.documents.get(&uri) else {
            return Vec::new();
        };
        if version <= current.version {
            // 旧版本 overlay 不生效（§6.5 规则 2），保持最新文本。
            return Vec::new();
        }
        let path = current.path.clone();
        if let Some(path) = &path
            && !self
                .host
                .set_overlay(path.clone(), version, text.to_string())
        {
            return Vec::new();
        }
        let Some(document) = self.documents.get_mut(&uri) else {
            return Vec::new();
        };
        document.version = version;
        document.text = text.to_string();
        document.single_file = None;
        self.publish_all()
    }

    fn did_close(&mut self, params: &Value) -> Vec<Value> {
        let Some(document) = params.get("textDocument") else {
            return Vec::new();
        };
        let uri = document
            .get("uri")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if let Some(document) = self.documents.remove(&uri)
            && let Some(path) = &document.path
        {
            self.host.remove_overlay(path);
        }
        let mut outputs = vec![notification(
            "textDocument/publishDiagnostics",
            json!({ "uri": uri, "diagnostics": [] }),
        )];
        outputs.extend(self.publish_all());
        outputs
    }

    /// 重算全部打开文档的诊断；只在集合变化时发布（§7.3）。
    fn publish_all(&mut self) -> Vec<Value> {
        let snapshot = self.host.snapshot();
        let mut outputs = Vec::new();
        let project_message = (!snapshot.project_diagnostics.is_empty()).then(|| {
            snapshot
                .project_diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message().to_string())
                .collect::<Vec<_>>()
                .join("\n")
        });
        match project_message {
            Some(message) => {
                if self.last_project_message.as_deref() != Some(message.as_str()) {
                    outputs.push(notification(
                        "window/showMessage",
                        json!({ "type": 1, "message": message.clone() }),
                    ));
                    self.last_project_message = Some(message);
                }
            }
            None => self.last_project_message = None,
        }
        let uris: Vec<String> = self.documents.keys().cloned().collect();
        for uri in uris {
            let Some(document) = self.documents.get_mut(&uri) else {
                continue;
            };
            let diagnostics = document_diagnostics(&snapshot, document);
            if document.last_published.as_ref() == Some(&diagnostics) {
                continue;
            }
            document.last_published = Some(diagnostics.clone());
            let mut params = json!({ "uri": uri, "diagnostics": diagnostics });
            params["version"] = json!(document.version);
            outputs.push(notification("textDocument/publishDiagnostics", params));
        }
        outputs
    }

    fn document_symbol(&self, params: &Value) -> Value {
        let Some((source, program)) = self.parsed(params) else {
            return Value::Null;
        };
        let symbols = collect_declarations(&program)
            .iter()
            .map(|declaration| {
                json!({
                    "name": declaration.name,
                    "kind": declaration.kind,
                    "range": range_json(&source, declaration.span),
                    "selectionRange": range_json(&source, declaration.name_span),
                })
            })
            .collect::<Vec<_>>();
        Value::Array(symbols)
    }

    fn hover(&mut self, params: &Value) -> Value {
        let Some(uri) = document_uri(params) else {
            return Value::Null;
        };
        let Some((line, character)) = cursor(params) else {
            return Value::Null;
        };
        self.with_index(uri, |sources, source_id, index| {
            let Some(source) = sources.get(source_id.0 as usize) else {
                return Value::Null;
            };
            let offset = position_to_offset(source, line, character);
            hover_at(sources, source_id, index, offset)
        })
        .unwrap_or(Value::Null)
    }

    fn definition(&mut self, params: &Value) -> Value {
        let Some(uri) = document_uri(params) else {
            return Value::Null;
        };
        let Some((line, character)) = cursor(params) else {
            return Value::Null;
        };
        self.with_index(uri, |sources, source_id, index| {
            let Some(source) = sources.get(source_id.0 as usize) else {
                return Value::Null;
            };
            let offset = position_to_offset(source, line, character);
            let (target_source, target_span) = match index.resolve(source_id, offset) {
                Resolution::Local { source, name_span } => (source, name_span),
                Resolution::Def(def) => match index.definition_of(&SymbolId::Def(def)) {
                    Some(definition) => (definition.source, definition.name_span),
                    None => return Value::Null,
                },
                Resolution::Instance { def, .. } => {
                    match index.definition_of(&SymbolId::Def(def)) {
                        Some(definition) => (definition.source, definition.name_span),
                        None => return Value::Null,
                    }
                }
                Resolution::Module { .. } | Resolution::Unresolved => return Value::Null,
            };
            location(sources, target_source, target_span)
        })
        .unwrap_or(Value::Null)
    }

    /// 对打开文档执行一次索引查询：项目单元优先，否则单文件分析。
    fn with_index<R>(
        &mut self,
        uri: &str,
        query: impl FnOnce(&[SourceFile], SourceId, &SymbolIndex) -> R,
    ) -> Option<R> {
        let snapshot = self.host.snapshot();
        let document = self.documents.get_mut(uri)?;
        let path = document.path.clone()?;
        if snapshot.mode == AnalysisMode::Project
            && let Some(unit) = snapshot.unit_for_path(&path)
            && let Some(source_id) = unit.source_id_for_path(&path)
            && let Some(index) = unit.index.as_ref()
        {
            return Some(query(&unit.sources, source_id, index));
        }
        let analysis = document.single_file();
        let index = analysis.index.as_ref()?;
        let source_id = analysis.sources.first()?.id;
        Some(query(&analysis.sources, source_id, index))
    }

    fn parsed(&self, params: &Value) -> Option<(SourceFile, ast::Program)> {
        let uri = document_uri(params)?;
        let text = self.documents.get(uri)?.text.as_str();
        let path = uri_to_path(uri).unwrap_or_else(|| PathBuf::from(uri));
        let source = SourceFile::new(path, text.to_string());
        let tokens = lex(&source).ok()?;
        let program = parse(&source, tokens).ok()?;
        Some((source, program))
    }
}

/// 该文档在当前快照下的 LSP 诊断数组（§7.3/§7.4）。
fn document_diagnostics(snapshot: &AnalysisSnapshot, document: &mut OpenDocument) -> Vec<Value> {
    if snapshot.mode == AnalysisMode::Project
        && let Some(path) = &document.path
        && let Some(unit) = snapshot.unit_for_path(path)
        && let Some(source_id) = unit.source_id_for_path(path)
    {
        let map = SourceMap::new(&unit.sources);
        return unit
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic
                    .labels()
                    .first()
                    .is_some_and(|label| label.source == source_id)
            })
            .map(|diagnostic| diagnostic_to_lsp(&map, diagnostic))
            .collect();
    }
    // 项目解析失败或文档不在任何单元内：仍做单文件语法/语义分析，
    // 保证“无法分析”不被显示为“没有错误”（§7.4）。
    let analysis = document.single_file();
    let map = SourceMap::new(&analysis.sources);
    analysis
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic_to_lsp(&map, diagnostic))
        .collect()
}

/// 悬停结果：身份来自 `SymbolIndex`，未解析返回 `null`（§2.5、§7.5）。
fn hover_at(
    sources: &[SourceFile],
    source_id: SourceId,
    index: &SymbolIndex,
    offset: usize,
) -> Value {
    let Some(source) = sources.get(source_id.0 as usize) else {
        return Value::Null;
    };
    let Ok(tokens) = lex(source) else {
        return Value::Null;
    };
    let Some((_, token_span)) = identifier_at(&tokens, offset) else {
        return Value::Null;
    };
    let value = match index.resolve(source_id, offset) {
        Resolution::Local {
            source: local,
            name_span,
        } => {
            let Some(local_source) = sources.get(local.0 as usize) else {
                return Value::Null;
            };
            let Some(name) = slice_text(local_source, name_span) else {
                return Value::Null;
            };
            match index.local_type(local, name_span) {
                Some(ty) => format!("local `{name}`: {ty}"),
                None => format!("local `{name}`"),
            }
        }
        Resolution::Def(def) => {
            let Some(text) = hover_definition(index, &SymbolId::Def(def)) else {
                return Value::Null;
            };
            text
        }
        Resolution::Instance { def, type_args } => {
            let Some(text) = hover_definition(index, &SymbolId::Instance { def, type_args }) else {
                return Value::Null;
            };
            text
        }
        Resolution::Module { qualified } => {
            let Some(display) = index.display_name(&SymbolId::Module { qualified }) else {
                return Value::Null;
            };
            format!("module `{display}`")
        }
        Resolution::Unresolved => return Value::Null,
    };
    json!({
        "contents": { "kind": "markdown", "value": value },
        "range": range_json(source, token_span),
    })
}

fn hover_definition(index: &SymbolIndex, symbol: &SymbolId) -> Option<String> {
    let definition = index.definition_of(symbol)?;
    let display = index.display_name(symbol)?;
    Some(format!("{} `{display}`", kind_label(definition.kind)))
}

fn kind_label(kind: DefKind) -> &'static str {
    match kind {
        DefKind::Function | DefKind::Method => "fn",
        DefKind::Struct => "struct",
        DefKind::Enum => "enum",
        DefKind::Trait => "trait",
        DefKind::Field => "field",
        DefKind::Variant => "variant",
        DefKind::TypeParam => "type",
    }
}

/// 定义位置：使用该分析单元的源码表把 `SourceId` 换算为 `file://` URI。
fn location(sources: &[SourceFile], source_id: SourceId, span: Span) -> Value {
    let Some(source) = sources.get(source_id.0 as usize) else {
        return Value::Null;
    };
    json!({
        "uri": path_to_uri(&source.path),
        "range": range_json(source, span),
    })
}

/// 收集单文件顶层声明的名称、种类与位置。
struct Declaration {
    name: String,
    kind: u32,
    name_span: Span,
    span: Span,
}

fn collect_declarations(program: &ast::Program) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    for function in &program.functions {
        declarations.push(Declaration {
            name: function.name.clone(),
            kind: SYMBOL_FUNCTION,
            name_span: function.name_span,
            span: function.span,
        });
    }
    for structure in &program.structs {
        declarations.push(Declaration {
            name: structure.name.clone(),
            kind: SYMBOL_STRUCT,
            name_span: structure.name_span,
            span: structure.name_span,
        });
    }
    for enumeration in &program.enums {
        declarations.push(Declaration {
            name: enumeration.name.clone(),
            kind: SYMBOL_ENUM,
            name_span: enumeration.name_span,
            span: enumeration.name_span,
        });
    }
    for trait_decl in &program.traits {
        declarations.push(Declaration {
            name: trait_decl.name.clone(),
            kind: SYMBOL_INTERFACE,
            name_span: trait_decl.name_span,
            span: trait_decl.name_span,
        });
    }
    for impl_block in &program.impls {
        declarations.push(Declaration {
            name: impl_block.type_name.clone(),
            kind: SYMBOL_OBJECT,
            name_span: impl_block.type_span,
            span: impl_block.type_span,
        });
    }
    declarations
}

fn identifier_at(tokens: &[Token], offset: usize) -> Option<(String, Span)> {
    for token in tokens {
        if let TokenKind::Identifier(name) = &token.kind
            && token.span.start <= offset
            && offset < token.span.end
        {
            return Some((name.clone(), token.span));
        }
    }
    for token in tokens {
        if let TokenKind::Identifier(name) = &token.kind
            && token.span.start < offset
            && offset == token.span.end
        {
            return Some((name.clone(), token.span));
        }
    }
    None
}

/// 结构化诊断 -> LSP `PublishDiagnosticsParams.diagnostics` 元素（M20 §4.1 规则 6）。
fn diagnostic_to_lsp(map: &SourceMap<'_>, diagnostic: &Diagnostic) -> Value {
    let range = match diagnostic.labels().first() {
        Some(label) => label_range(map, label),
        None => zero_range(),
    };
    let related = diagnostic
        .labels()
        .iter()
        .skip(1)
        .map(|label| {
            json!({
                "location": {
                    "uri": label_uri(map, label),
                    "range": label_range(map, label),
                },
                "message": label.message,
            })
        })
        .collect::<Vec<_>>();
    let mut value = json!({
        "range": range,
        "severity": severity_to_lsp(diagnostic.severity()),
        "code": diagnostic.code(),
        "source": "dolphin",
        "message": diagnostic.message(),
    });
    if !related.is_empty() {
        value["relatedInformation"] = Value::Array(related);
    }
    value
}

fn severity_to_lsp(severity: Severity) -> u32 {
    match severity {
        Severity::Error => 1,
        Severity::Warning => 2,
        Severity::Note => 3,
    }
}

fn zero_range() -> Value {
    json!({
        "start": { "line": 0, "character": 0 },
        "end": { "line": 0, "character": 0 },
    })
}

fn label_range(map: &SourceMap<'_>, label: &Label) -> Value {
    match map.file(label.source) {
        Some(source) => range_json(source, label.span),
        None => zero_range(),
    }
}

fn label_uri(map: &SourceMap<'_>, label: &Label) -> String {
    match map.path(label.source) {
        Some(path) => path_to_uri(path),
        None => String::new(),
    }
}

fn range_json(source: &SourceFile, span: Span) -> Value {
    json!({
        "start": offset_to_position(source, span.start),
        "end": offset_to_position(source, span.end),
    })
}

/// 字节偏移 → LSP 位置（0-based 行、UTF-16 列）。
fn offset_to_position(source: &SourceFile, offset: usize) -> Value {
    let offset = snap_boundary(source, offset);
    let (line, _) = source.line_column(offset);
    let start = source.line_start(line);
    let character = source.text[start..offset]
        .chars()
        .map(char::len_utf16)
        .sum::<usize>();
    json!({ "line": line - 1, "character": character })
}

/// LSP 位置（0-based 行、UTF-16 列）→ 字节偏移。
fn position_to_offset(source: &SourceFile, line: u64, character: u64) -> usize {
    let line = line as usize + 1;
    if line > source.line_count() {
        return source.text.len();
    }
    let mut byte = source.line_start(line);
    let mut utf16 = 0;
    for character_unit in source.line_text(line).chars() {
        if utf16 >= character as usize {
            break;
        }
        utf16 += character_unit.len_utf16();
        byte += character_unit.len_utf8();
    }
    byte
}

fn snap_boundary(source: &SourceFile, offset: usize) -> usize {
    let mut offset = offset.min(source.text.len());
    while !source.text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn slice_text(source: &SourceFile, span: Span) -> Option<String> {
    let start = snap_boundary(source, span.start);
    let end = snap_boundary(source, span.end);
    source.text.get(start..end).map(str::to_string)
}

fn document_uri(params: &Value) -> Option<&str> {
    params.get("textDocument")?.get("uri")?.as_str()
}

fn cursor(params: &Value) -> Option<(u64, u64)> {
    let position = params.get("position")?;
    Some((
        position.get("line").and_then(Value::as_u64).unwrap_or(0),
        position
            .get("character")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    ))
}

fn capabilities() -> Value {
    json!({
        "capabilities": {
            "textDocumentSync": 1,
            "hoverProvider": true,
            "definitionProvider": true,
            "documentSymbolProvider": true,
            "positionEncoding": "utf-16",
        }
    })
}

fn response(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": result })
}

fn error_response(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": { "code": code, "message": message },
    })
}

fn notification(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn initialized() -> Server {
        let mut server = Server::new();
        server.handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }));
        server.handle(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
        server
    }

    fn open(server: &mut Server, uri: &str, text: &str) -> Vec<Value> {
        server.handle(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "dolphin",
                    "version": 1,
                    "text": text,
                }
            }
        }))
    }

    fn diagnostics(outputs: &[Value]) -> &Value {
        outputs
            .iter()
            .find(|message| message["method"] == "textDocument/publishDiagnostics")
            .expect("diagnostics notification")
            .get("params")
            .and_then(|params| params.get("diagnostics"))
            .expect("diagnostics array")
    }

    #[test]
    fn utf16_positions_round_trip() {
        let source = SourceFile::new(PathBuf::from("test.do"), "a海𝄞b".to_string());
        assert_eq!(
            offset_to_position(&source, 0),
            json!({ "line": 0, "character": 0 })
        );
        assert_eq!(
            offset_to_position(&source, 1),
            json!({ "line": 0, "character": 1 })
        );
        assert_eq!(
            offset_to_position(&source, 4),
            json!({ "line": 0, "character": 2 })
        );
        assert_eq!(
            offset_to_position(&source, 8),
            json!({ "line": 0, "character": 4 })
        );
        assert_eq!(
            offset_to_position(&source, 9),
            json!({ "line": 0, "character": 5 })
        );
        assert_eq!(position_to_offset(&source, 0, 0), 0);
        assert_eq!(position_to_offset(&source, 0, 1), 1);
        assert_eq!(position_to_offset(&source, 0, 2), 4);
        assert_eq!(position_to_offset(&source, 0, 4), 8);
        assert_eq!(position_to_offset(&source, 0, 5), 9);
    }

    #[test]
    fn initialize_reports_capabilities() {
        let mut server = Server::new();
        let outputs = server
            .handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }));
        assert_eq!(outputs.len(), 1);
        let capabilities = &outputs[0]["result"]["capabilities"];
        assert_eq!(capabilities["textDocumentSync"], 1);
        assert_eq!(capabilities["hoverProvider"], true);
        assert_eq!(capabilities["definitionProvider"], true);
        assert_eq!(capabilities["documentSymbolProvider"], true);
        assert_eq!(capabilities["positionEncoding"], "utf-16");
    }

    #[test]
    fn requests_before_initialize_report_not_initialized() {
        let mut server = Server::new();
        let outputs = server.handle(json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "textDocument/hover",
            "params": {}
        }));
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0]["id"], 7);
        assert_eq!(outputs[0]["error"]["code"], -32002);
        assert!(outputs[0].get("result").is_none());
    }

    #[test]
    fn requests_after_shutdown_report_invalid_request() {
        let mut server = initialized();
        server.handle(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }));
        let outputs = server.handle(json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/documentSymbol",
            "params": {}
        }));
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0]["id"], 3);
        assert_eq!(outputs[0]["error"]["code"], -32600);
    }

    #[test]
    fn exit_without_shutdown_is_failure() {
        let mut server = initialized();
        assert!(
            server
                .handle(json!({ "jsonrpc": "2.0", "method": "exit" }))
                .is_empty()
        );
        assert!(server.exited);
        assert_eq!(server.exit_code(), ExitCode::FAILURE);

        let mut clean = initialized();
        clean.handle(json!({ "jsonrpc": "2.0", "id": 9, "method": "shutdown" }));
        clean.handle(json!({ "jsonrpc": "2.0", "method": "exit" }));
        assert_eq!(clean.exit_code(), ExitCode::SUCCESS);
    }

    #[test]
    fn publishes_semantic_diagnostics() {
        let mut server = initialized();
        let outputs = open(
            &mut server,
            "file:///bad.do",
            "fn main() { var item: i32 = true; return; }",
        );
        assert!(!diagnostics(&outputs).as_array().unwrap().is_empty());
    }

    #[test]
    fn publishes_no_diagnostics_for_valid_program() {
        let mut server = initialized();
        let outputs = open(&mut server, "file:///good.do", "fn main() { return; }");
        assert!(diagnostics(&outputs).as_array().unwrap().is_empty());
    }

    #[test]
    fn document_symbols_list_declarations() {
        let mut server = initialized();
        let uri = "file:///symbols.do";
        open(
            &mut server,
            uri,
            "fn main() { return; }\nstruct Point { x: i32, y: i32 }\n",
        );
        let outputs = server.handle(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/documentSymbol",
            "params": { "textDocument": { "uri": uri } }
        }));
        let symbols = outputs[0]["result"].as_array().expect("symbol array");
        let names = symbols
            .iter()
            .filter_map(|symbol| symbol["name"].as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"Point"));
    }

    #[test]
    fn definition_points_to_declaration() {
        let mut server = initialized();
        // 平台绝对路径的 `file://` URI（Windows 为盘符形式；单文件路径经 URI 往返）。
        let uri = path_to_uri(&std::env::temp_dir().join("definition.do"));
        open(
            &mut server,
            &uri,
            "fn helper() { return; }\nfn main() { helper(); return; }\n",
        );
        let outputs = server.handle(json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 1, "character": 12 }
            }
        }));
        let location = &outputs[0]["result"];
        assert_eq!(location["uri"], uri.as_str());
        assert_eq!(location["range"]["start"]["line"], 0);
        assert_eq!(location["range"]["start"]["character"], 3);
    }

    #[test]
    fn hover_local_reports_type_annotation() {
        let mut server = initialized();
        let uri = "file:///hover.do";
        open(
            &mut server,
            uri,
            "fn helper(value: i32): i32 { return value; }\nfn main() { return helper(1); }\n",
        );
        let outputs = server.handle(json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 0, "character": 37 }
            }
        }));
        let result = &outputs[0]["result"];
        assert_eq!(result["contents"]["value"], "local `value`: i32");
        assert_eq!(result["range"]["start"]["character"], 36);
    }

    #[test]
    fn hover_definition_kinds_use_display_names() {
        let mut server = initialized();
        let uri = "file:///kinds.do";
        open(
            &mut server,
            uri,
            "struct Point { x: i32 }\nfn main() { val p = Point(1); return p.x; }\n",
        );
        let outputs = server.handle(json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 1, "character": 22 }
            }
        }));
        assert_eq!(outputs[0]["result"]["contents"]["value"], "struct `Point`");
        // 未解析的字段访问不得回退为文本同名查找。
        let outputs = server.handle(json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 1, "character": 39 }
            }
        }));
        assert_eq!(outputs[0]["result"], Value::Null);
    }

    #[test]
    fn unknown_request_returns_method_not_found() {
        let mut server = initialized();
        let outputs = server.handle(json!({
            "jsonrpc": "2.0",
            "id": 99,
            "method": "workspace/unknownThing",
            "params": {}
        }));
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0]["id"], 99);
        assert_eq!(outputs[0]["error"]["code"], -32601);
        assert!(outputs[0].get("result").is_none());
    }

    #[test]
    fn structured_diagnostics_use_code_message_and_utf16_range() {
        let mut server = initialized();
        let outputs = open(
            &mut server,
            "file:///bad.do",
            "fn main() {\n    return missing;\n}\n",
        );
        let diagnostics = diagnostics(&outputs).as_array().unwrap();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0]["code"], "E0001");
        assert_eq!(diagnostics[0]["message"], "unknown variable `missing`");
        assert_eq!(diagnostics[0]["severity"], 1);
        assert_eq!(diagnostics[0]["source"], "dolphin");
        assert_eq!(
            diagnostics[0]["range"]["start"],
            json!({ "line": 1, "character": 11 })
        );
        assert_eq!(
            diagnostics[0]["range"]["end"],
            json!({ "line": 1, "character": 18 })
        );
    }

    #[test]
    fn related_information_maps_secondary_labels() {
        let base = std::env::temp_dir();
        let primary =
            SourceFile::with_id(SourceId(0), base.join("a.do"), "fn a() {}\n".to_string());
        let related =
            SourceFile::with_id(SourceId(1), base.join("b.do"), "fn b() {}\n".to_string());
        let files = vec![primary, related];
        let map = SourceMap::new(&files);
        let diagnostic = Diagnostic::at(&files[0], Span::new(3, 4), "duplicate").with_label(
            &files[1],
            Span::new(3, 4),
            "previous definition",
        );
        let value = diagnostic_to_lsp(&map, &diagnostic);
        assert_eq!(value["code"], "E0001");
        assert_eq!(value["message"], "duplicate");
        assert_eq!(value["range"]["start"]["character"], 3);
        let related_info = value["relatedInformation"].as_array().unwrap();
        assert_eq!(related_info.len(), 1);
        assert_eq!(
            related_info[0]["location"]["uri"],
            path_to_uri(&files[1].path).as_str()
        );
        assert_eq!(related_info[0]["message"], "previous definition");
        assert_eq!(
            related_info[0]["location"]["range"]["start"]["character"],
            3
        );
    }

    #[test]
    fn anonymous_diagnostic_without_map_entry_gets_zero_range() {
        let anonymous = SourceFile::new(PathBuf::from("/anon.do"), "fn a() {}\n".to_string());
        let diagnostic = Diagnostic::at(&anonymous, Span::new(3, 4), "bad");
        let map = SourceMap::new(std::slice::from_ref(&anonymous));
        let value = diagnostic_to_lsp(&map, &diagnostic);
        assert_eq!(value["range"], zero_range());
    }
}
