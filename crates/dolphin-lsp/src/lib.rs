//! Dolphin 语言服务器（M17）：标准输入/输出上的最小 LSP 实现。
//!
//! 提供文档诊断、文档符号、悬停与跳转定义，覆盖 M1-M15 语法。

use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use dolphin_hir::lower::lower_collecting;
use dolphin_source::diagnostic::{Diagnostic, Label, Severity};
use dolphin_source::lexer::{lex, lex_recovering};
use dolphin_source::source::{SourceFile, SourceId, SourceMap, Span};
use dolphin_source::token::{Token, TokenKind};
use dolphin_syntax::ast;
use dolphin_syntax::parser::{parse, parse_recovering};
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
pub fn serve() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = io::BufReader::new(stdin.lock());
    let mut writer = io::BufWriter::new(stdout.lock());
    let mut server = Server::new();

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
    Ok(())
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
#[derive(Default)]
pub struct Server {
    documents: HashMap<String, String>,
    /// 收到 `exit` 后置位，由传输循环负责终止。
    pub exited: bool,
}

impl Server {
    pub fn new() -> Self {
        Self::default()
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

        match method.as_str() {
            "initialize" => vec![response(
                id,
                json!({
                    "capabilities": {
                        "textDocumentSync": 1,
                        "hoverProvider": true,
                        "definitionProvider": true,
                        "documentSymbolProvider": true,
                    }
                }),
            )],
            "initialized" => Vec::new(),
            "shutdown" => vec![response(id, Value::Null)],
            "exit" => {
                self.exited = true;
                Vec::new()
            }
            "textDocument/didOpen" => self.did_open(&params),
            "textDocument/didChange" => self.did_change(&params),
            "textDocument/didClose" => self.did_close(&params),
            "textDocument/documentSymbol" => vec![response(id, self.document_symbol(&params))],
            "textDocument/hover" => vec![response(id, self.hover(&params))],
            "textDocument/definition" => vec![response(id, self.definition(&params))],
            _ => {
                if id.is_some() {
                    vec![response(id, Value::Null)]
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
            .unwrap_or_default();
        let text = document
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        self.documents.insert(uri.to_string(), text.to_string());
        self.publish(uri, text)
    }

    fn did_change(&mut self, params: &Value) -> Vec<Value> {
        let Some(document) = params.get("textDocument") else {
            return Vec::new();
        };
        let uri = document
            .get("uri")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(text) = params
            .get("contentChanges")
            .and_then(Value::as_array)
            .and_then(|changes| changes.last())
            .and_then(|change| change.get("text"))
            .and_then(Value::as_str)
        else {
            return Vec::new();
        };
        self.documents.insert(uri.to_string(), text.to_string());
        self.publish(uri, text)
    }

    fn did_close(&mut self, params: &Value) -> Vec<Value> {
        let Some(document) = params.get("textDocument") else {
            return Vec::new();
        };
        let uri = document
            .get("uri")
            .and_then(Value::as_str)
            .unwrap_or_default();
        self.documents.remove(uri);
        vec![notification(
            "textDocument/publishDiagnostics",
            json!({ "uri": uri, "diagnostics": [] }),
        )]
    }

    fn publish(&self, uri: &str, text: &str) -> Vec<Value> {
        // 单文件分析的 `SourceId` 固定为 0，与 `SourceMap` 一起让结构化诊断能换算位置。
        let source = SourceFile::with_id(SourceId(0), path_for(uri), text.to_string());
        let map = SourceMap::new(std::slice::from_ref(&source));
        let diagnostics = analyze(&source)
            .iter()
            .map(|diagnostic| diagnostic_to_lsp(&map, diagnostic))
            .collect::<Vec<_>>();
        vec![notification(
            "textDocument/publishDiagnostics",
            json!({ "uri": uri, "diagnostics": diagnostics }),
        )]
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

    fn hover(&self, params: &Value) -> Value {
        let Some((source, program)) = self.parsed(params) else {
            return Value::Null;
        };
        let Some((name, span)) = self.identifier(params, &source) else {
            return Value::Null;
        };
        let Some(declaration) = find_declaration(&program, &name) else {
            return Value::Null;
        };
        json!({
            "contents": {
                "kind": "markdown",
                "value": format!("{} `{}`", declaration.kind_label, declaration.name),
            },
            "range": range_json(&source, span),
        })
    }

    fn definition(&self, params: &Value) -> Value {
        let Some(uri) = params
            .get("textDocument")
            .and_then(|document| document.get("uri"))
            .and_then(Value::as_str)
        else {
            return Value::Null;
        };
        let Some((source, program)) = self.parsed(params) else {
            return Value::Null;
        };
        let Some((name, _)) = self.identifier(params, &source) else {
            return Value::Null;
        };
        let Some(declaration) = find_declaration(&program, &name) else {
            return Value::Null;
        };
        json!({ "uri": uri, "range": range_json(&source, declaration.name_span) })
    }

    fn parsed(&self, params: &Value) -> Option<(SourceFile, ast::Program)> {
        let uri = params.get("textDocument")?.get("uri")?.as_str()?;
        let text = self.documents.get(uri)?;
        let source = SourceFile::new(path_for(uri), text.clone());
        let tokens = lex(&source).ok()?;
        let program = parse(&source, tokens).ok()?;
        Some((source, program))
    }

    fn identifier(&self, params: &Value, source: &SourceFile) -> Option<(String, Span)> {
        let position = params.get("position")?;
        let line = position.get("line").and_then(Value::as_u64).unwrap_or(0);
        let character = position
            .get("character")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let offset = position_to_offset(source, line, character);
        let tokens = lex(source).ok()?;
        identifier_at(&tokens, offset)
    }
}

/// 收集单文件顶层声明的名称、种类与位置。
struct Declaration {
    name: String,
    kind: u32,
    kind_label: &'static str,
    name_span: Span,
    span: Span,
}

fn collect_declarations(program: &ast::Program) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    for function in &program.functions {
        declarations.push(Declaration {
            name: function.name.clone(),
            kind: SYMBOL_FUNCTION,
            kind_label: "function",
            name_span: function.name_span,
            span: function.span,
        });
    }
    for structure in &program.structs {
        declarations.push(Declaration {
            name: structure.name.clone(),
            kind: SYMBOL_STRUCT,
            kind_label: "struct",
            name_span: structure.name_span,
            span: structure.name_span,
        });
    }
    for enumeration in &program.enums {
        declarations.push(Declaration {
            name: enumeration.name.clone(),
            kind: SYMBOL_ENUM,
            kind_label: "enum",
            name_span: enumeration.name_span,
            span: enumeration.name_span,
        });
    }
    for trait_decl in &program.traits {
        declarations.push(Declaration {
            name: trait_decl.name.clone(),
            kind: SYMBOL_INTERFACE,
            kind_label: "trait",
            name_span: trait_decl.name_span,
            span: trait_decl.name_span,
        });
    }
    for impl_block in &program.impls {
        declarations.push(Declaration {
            name: impl_block.type_name.clone(),
            kind: SYMBOL_OBJECT,
            kind_label: "impl",
            name_span: impl_block.type_span,
            span: impl_block.type_span,
        });
    }
    declarations
}

fn find_declaration(program: &ast::Program, name: &str) -> Option<Declaration> {
    collect_declarations(program)
        .into_iter()
        .find(|declaration| declaration.name == name)
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

/// 词法/语法错误全部返回（收集式）；无 `pkg`/`use` 时再运行语义检查。
fn analyze(source: &SourceFile) -> Vec<Diagnostic> {
    let (tokens, lex_diagnostics) = lex_recovering(source);
    if !lex_diagnostics.is_empty() {
        return lex_diagnostics;
    }
    let (program, parse_diagnostics) = parse_recovering(source, tokens);
    if !parse_diagnostics.is_empty() {
        return parse_diagnostics;
    }
    if program.package.is_some() || !program.uses.is_empty() {
        return Vec::new();
    }
    // 库文件或尚未写完 `main` 的文档不做语义检查，避免误报 "missing main"。
    if !program
        .functions
        .iter()
        .any(|function| function.name == "main")
    {
        return Vec::new();
    }
    match lower_collecting(source, &program) {
        Ok(_) => Vec::new(),
        Err(diagnostics) => diagnostics,
    }
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
                    "uri": uri_for_path(map, label.source),
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

/// relatedInformation 的 URI（完整 URI 编解码属 H20-02）。
fn uri_for_path(map: &SourceMap<'_>, id: SourceId) -> String {
    match map.path(id) {
        Some(path) => path_to_uri(path),
        None => String::new(),
    }
}

fn path_to_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if text.starts_with('/') {
        format!("file://{text}")
    } else {
        format!("file:///{text}")
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
    let mut offset = offset.min(source.text.len());
    while !source.text.is_char_boundary(offset) {
        offset -= 1;
    }
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

fn path_for(uri: &str) -> PathBuf {
    PathBuf::from(uri.strip_prefix("file://").unwrap_or(uri))
}

fn response(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": result })
}

fn notification(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn publishes_semantic_diagnostics() {
        let mut server = Server::new();
        let outputs = open(
            &mut server,
            "file:///bad.do",
            "fn main() { var item: i32 = true; return; }",
        );
        assert!(!diagnostics(&outputs).as_array().unwrap().is_empty());
    }

    #[test]
    fn publishes_no_diagnostics_for_valid_program() {
        let mut server = Server::new();
        let outputs = open(&mut server, "file:///good.do", "fn main() { return; }");
        assert!(diagnostics(&outputs).as_array().unwrap().is_empty());
    }

    #[test]
    fn document_symbols_list_declarations() {
        let mut server = Server::new();
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
        let mut server = Server::new();
        let uri = "file:///definition.do";
        open(
            &mut server,
            uri,
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
        assert_eq!(location["uri"], uri);
        assert_eq!(location["range"]["start"]["line"], 0);
        assert_eq!(location["range"]["start"]["character"], 3);
    }

    #[test]
    fn unknown_request_returns_null() {
        let mut server = Server::new();
        let outputs = server.handle(json!({
            "jsonrpc": "2.0",
            "id": 99,
            "method": "workspace/unknownThing",
            "params": {}
        }));
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0]["id"], 99);
        assert_eq!(outputs[0]["result"], Value::Null);
    }

    #[test]
    fn structured_diagnostics_use_code_message_and_utf16_range() {
        let mut server = Server::new();
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
        let primary = SourceFile::with_id(
            SourceId(0),
            PathBuf::from("/a.do"),
            "fn a() {}\n".to_string(),
        );
        let related = SourceFile::with_id(
            SourceId(1),
            PathBuf::from("/b.do"),
            "fn b() {}\n".to_string(),
        );
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
        assert_eq!(related_info[0]["location"]["uri"], "file:///b.do");
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
