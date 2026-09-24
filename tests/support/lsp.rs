//! H20-03 LSP 测试协议（§7.6）：真实 `dc lsp <项目>` 子进程 stdio 会话。
//!
//! `Content-Length` 帧、大小写不敏感头部、`\r\n\r\n` 结束；10s 超时到达即 kill
//! 并在 panic 信息里带上 stderr。不得只调用 `Server::handle` 内部 helper 作为验收。

#![allow(dead_code)]

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

/// 单条消息的等待上限。
pub const TIMEOUT: Duration = Duration::from_secs(10);

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// 一个真实的 `dc lsp` stdio 会话。
pub struct LspSession {
    child: Child,
    stdin: Option<ChildStdin>,
    messages: mpsc::Receiver<Result<Value, String>>,
    pending: VecDeque<Value>,
    stderr: Arc<Mutex<String>>,
    stderr_thread: Option<JoinHandle<()>>,
    home: PathBuf,
}

impl LspSession {
    /// 启动 `dc lsp <project>`；使用独立临时 `DOLPHIN_HOME`，退出时清理。
    pub fn start(project: &Path) -> Self {
        let home = unique_dir("dolphin-m20-lsp-home");
        Self::spawn(Some(project), &home)
    }

    /// 启动不带位置参数的 `dc lsp`（默认当前目录）。
    pub fn start_default() -> Self {
        let home = unique_dir("dolphin-m20-lsp-home");
        Self::spawn(None, &home)
    }

    /// 与 [`Self::start`] 相同，但显式指定缓存根（测试隔离）。
    pub fn start_with_home(project: &Path, home: &Path) -> Self {
        Self::spawn(Some(project), home)
    }

    fn spawn(project: Option<&Path>, home: &Path) -> Self {
        std::fs::create_dir_all(home).expect("lsp home should be created");
        let mut command = Command::new(env!("CARGO_BIN_EXE_dc"));
        command
            .arg("lsp")
            .env("DOLPHIN_HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(project) = project {
            command.arg(project);
        }
        let mut child = command.spawn().expect("`dc lsp` should start");
        let stdin = child.stdin.take().expect("stdin must be piped");
        let stdout = child.stdout.take().expect("stdout must be piped");
        let stderr_pipe = child.stderr.take().expect("stderr must be piped");

        let (sender, messages) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_frame(&mut reader) {
                    Ok(Some(body)) => {
                        let value = serde_json::from_slice::<Value>(&body)
                            .map_err(|error| format!("invalid JSON body: {error}"));
                        if sender.send(value).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            }
        });

        let stderr = Arc::new(Mutex::new(String::new()));
        let collected = Arc::clone(&stderr);
        let stderr_thread = std::thread::spawn(move || {
            let mut text = String::new();
            BufReader::new(stderr_pipe).read_to_string(&mut text).ok();
            *collected.lock().expect("stderr lock") = text;
        });

        LspSession {
            child,
            stdin: Some(stdin),
            messages,
            pending: VecDeque::new(),
            stderr,
            stderr_thread: Some(stderr_thread),
            home: home.to_path_buf(),
        }
    }

    /// 写出一条 `Content-Length` 帧消息。
    pub fn send(&mut self, message: Value) {
        let body = serde_json::to_vec(&message).expect("message should serialize");
        let stdin = self.stdin.as_mut().expect("session stdin is open");
        write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).expect("write header");
        stdin.write_all(&body).expect("write body");
        stdin.flush().expect("flush body");
    }

    /// 直接写入原始字节（用于构造非法 `Content-Length` 帧）。
    pub fn send_raw(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("session stdin is open");
        stdin.write_all(bytes).expect("write raw bytes");
        stdin.flush().expect("flush raw bytes");
    }

    /// 读取指定 `id` 的响应；其他消息进入 pending 缓冲。
    pub fn recv_response(&mut self, id: i64) -> Value {
        loop {
            let message = self.recv_channel();
            let matched = message.get("id").and_then(Value::as_i64) == Some(id)
                && (message.get("result").is_some() || message.get("error").is_some());
            if matched {
                return message;
            }
            self.pending.push_back(message);
        }
    }

    /// 读取指定方法的通知；其他消息进入 pending 缓冲。
    pub fn recv_notification(&mut self, method: &str) -> Value {
        loop {
            let message = self.recv_channel();
            if message.get("method").and_then(Value::as_str) == Some(method) {
                return message;
            }
            self.pending.push_back(message);
        }
    }

    /// 固定收尾：`shutdown`（断言 `result: null`）→ `exit`。
    pub fn shutdown(&mut self) {
        self.send(json!({ "jsonrpc": "2.0", "id": 9999, "method": "shutdown" }));
        let response = self.recv_response(9999);
        assert!(
            response.get("error").is_none() && response["result"].is_null(),
            "shutdown must return null: {response}"
        );
        self.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
    }

    /// 关闭 stdin 并等待进程退出；超时则 kill 后 panic。
    pub fn finish(mut self) -> ExitStatus {
        self.stdin.take();
        self.wait(TIMEOUT)
    }

    /// 与 [`Self::finish`] 相同，但返回收集完整的 stderr 文本。
    pub fn finish_with_stderr(mut self) -> (ExitStatus, String) {
        self.stdin.take();
        let status = self.wait(TIMEOUT);
        let stderr = self.join_stderr();
        (status, stderr)
    }

    /// 会话 stderr（进程结束后可用）。
    pub fn stderr_text(&self) -> String {
        self.stderr.lock().expect("stderr lock").clone()
    }

    fn join_stderr(&mut self) -> String {
        if let Some(handle) = self.stderr_thread.take() {
            handle.join().ok();
        }
        self.stderr_text()
    }

    fn recv_channel(&mut self) -> Value {
        match self.messages.recv_timeout(TIMEOUT) {
            Ok(Ok(message)) => message,
            Ok(Err(error)) => {
                self.child.kill().ok();
                panic!(
                    "language server sent an invalid frame: {error}; stderr:\n{}",
                    self.stderr_text()
                );
            }
            Err(_) => {
                self.child.kill().ok();
                panic!(
                    "language server did not respond within {TIMEOUT:?}; stderr:\n{}",
                    self.stderr_text()
                );
            }
        }
    }

    fn wait(&mut self, timeout: Duration) -> ExitStatus {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait().expect("try_wait should succeed") {
                return status;
            }
            if Instant::now() >= deadline {
                self.child.kill().ok();
                panic!(
                    "language server did not exit within {timeout:?}; stderr:\n{}",
                    self.stderr_text()
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for LspSession {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
        self.join_stderr();
        std::fs::remove_dir_all(&self.home).ok();
    }
}

fn read_frame(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, String> {
    let mut content_length = None;
    let mut saw_header = false;
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Ok(None);
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            if saw_header {
                break;
            }
            continue;
        }
        saw_header = true;
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse::<usize>().ok();
        }
    }
    let length = content_length.ok_or_else(|| "missing Content-Length header".to_string())?;
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .map_err(|error| error.to_string())?;
    Ok(Some(body))
}

fn unique_dir(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "{tag}-{}-{unique}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ))
}
