/// php-src: ext/standard/exec.c `php_escape_shell_cmd` (PHP 8.5.2)
///
/// Unix branch only. Shell metacharacters are backslash-escaped; quote characters are escaped
/// only when unpaired, paired quotes being left intact. The multibyte skip (`php_mblen`), the
/// command length check and the Windows branch are not ported.
pub fn escapeshellcmd(command: &str) -> String {
    let bytes = command.as_bytes();
    let len = bytes.len();
    let mut out: Vec<u8> = Vec::with_capacity(len);
    // Byte index of the matching closing quote while inside a paired quote run.
    let mut paired: Option<usize> = None;
    let mut x = 0;
    while x < len {
        let c = bytes[x];
        match c {
            b'"' | b'\'' => {
                if paired.is_none() {
                    if let Some(rel) = bytes[x + 1..].iter().position(|&b| b == c) {
                        paired = Some(x + 1 + rel);
                    } else {
                        out.push(b'\\');
                    }
                } else if paired == Some(x) {
                    paired = None;
                } else {
                    out.push(b'\\');
                }
                out.push(c);
            }
            b'#' | b'&' | b';' | b'`' | b'|' | b'*' | b'?' | b'~' | b'<' | b'>' | b'^' | b'('
            | b')' | b'[' | b']' | b'{' | b'}' | b'$' | b'\\' | 0x0A | 0xFF => {
                out.push(b'\\');
                out.push(c);
            }
            _ => out.push(c),
        }
        x += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
