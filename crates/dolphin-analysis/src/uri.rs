//! LSP `file://` URI 与本地路径互转（M20/H20-02，§6.3）。
//!
//! 只支持 `file` scheme；百分号解码空格/Unicode/中文路径；Windows
//! `file:///C:/x` 映射为 `C:\x`；含反斜杠的 URI 与非 `file` scheme 返回 `None`。

use std::path::{Path, PathBuf};

/// URI -> 本地路径；非 `file://`、非法转义或反斜杠 URI 返回 `None`。
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    uri_to_path_impl(uri, cfg!(windows))
}

/// 与 [`uri_to_path`] 相同，但显式指定宿主风格；供单元测试覆盖两个平台分支。
pub(crate) fn uri_to_path_impl(uri: &str, windows: bool) -> Option<PathBuf> {
    let rest = uri.get(..7)?;
    if !rest.eq_ignore_ascii_case("file://") {
        return None;
    }
    let rest = &uri[7..];
    // 反斜杠 URI 明确拒绝，不做静默替换。
    if rest.contains('\\') {
        return None;
    }
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, ""),
    };
    if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
        return None;
    }
    let decoded = percent_decode(path)?;
    if decoded.is_empty() {
        return None;
    }
    if windows {
        let bytes = decoded.as_bytes();
        let has_drive = bytes.len() >= 3
            && bytes[0] == b'/'
            && bytes[1].is_ascii_alphabetic()
            && bytes[2] == b':';
        let without = if has_drive {
            &decoded[1..]
        } else {
            &decoded[..]
        };
        Some(PathBuf::from(without.replace('/', "\\")))
    } else {
        Some(PathBuf::from(decoded))
    }
}

/// 本地路径 -> `file://` URI；相对路径先按当前目录绝对化。
pub fn path_to_uri(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
    };
    let text = absolute.to_string_lossy().replace('\\', "/");
    let encoded = percent_encode(&text);
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return None;
            }
            let high = hex_digit(bytes[index + 1])?;
            let low = hex_digit(bytes[index + 2])?;
            output.push(high * 16 + low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).ok()
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn percent_encode(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for byte in text.bytes() {
        let unreserved =
            byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b':' | b'-' | b'.' | b'_' | b'~');
        if unreserved {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push_str(&format!("{byte:02X}"));
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_percent_escapes_and_unicode() {
        assert_eq!(
            uri_to_path_impl("file:///tmp/a%20b.do", false),
            Some(PathBuf::from("/tmp/a b.do"))
        );
        assert_eq!(
            uri_to_path_impl("file:///tmp/%E6%B5%8B%E8%AF%95.do", false),
            Some(PathBuf::from("/tmp/测试.do"))
        );
        assert_eq!(
            uri_to_path_impl("file:///tmp/%E6%B5%8B%E8%AF%95.do", true),
            Some(PathBuf::from("\\tmp\\测试.do"))
        );
    }

    #[test]
    fn windows_drive_letters_map_to_backslash_paths() {
        assert_eq!(
            uri_to_path_impl("file:///C:/proj/src/main.do", true),
            Some(PathBuf::from("C:\\proj\\src\\main.do"))
        );
        assert_eq!(
            uri_to_path_impl("file:///c%3A/x.do", true),
            Some(PathBuf::from("c:\\x.do"))
        );
    }

    #[test]
    fn rejects_non_file_scheme_backslashes_and_bad_escapes() {
        assert_eq!(uri_to_path_impl("http://example.com/x", false), None);
        assert_eq!(uri_to_path_impl("untitled:Untitled-1", false), None);
        assert_eq!(uri_to_path_impl("file:///tmp/a\\b.do", false), None);
        assert_eq!(uri_to_path_impl("file:///tmp/a%2.do", false), None);
        assert_eq!(uri_to_path_impl("file:///tmp/a%ZZ.do", false), None);
        assert_eq!(uri_to_path_impl("file://server/share/x.do", false), None);
        assert_eq!(uri_to_path_impl("file://", false), None);
    }

    #[test]
    fn path_to_uri_round_trips_spaces_and_unicode() {
        let path = if cfg!(windows) {
            PathBuf::from("C:\\proj dir\\测试.do")
        } else {
            PathBuf::from("/proj dir/测试.do")
        };
        let uri = path_to_uri(&path);
        assert!(!uri.contains(' '), "{uri}");
        assert!(!uri.contains('测'), "{uri}");
        let decoded = uri_to_path_impl(&uri, cfg!(windows)).expect("round trip");
        assert_eq!(decoded, path);
        assert_eq!(path_to_uri(&path), uri);
    }
}
