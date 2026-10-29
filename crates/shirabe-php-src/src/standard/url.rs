use bstr::{BString, ByteSlice};

/// php-src: ext/standard/url.h `php_url` (PHP 8.5.5)
///
/// `port` is `Some` exactly when `php_url_parse_ex2` sets `has_port`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PhpUrl {
    pub scheme: Option<BString>,
    pub user: Option<BString>,
    pub pass: Option<BString>,
    pub host: Option<BString>,
    pub port: Option<u16>,
    pub path: Option<BString>,
    pub query: Option<BString>,
    pub fragment: Option<BString>,
}

enum Step {
    ParsePort(usize),
    ParseHost,
    JustPath,
}

/// php-src: ext/standard/url.c `php_url_parse_ex2` (PHP 8.5.5)
///
/// `None` where the original returns `NULL`. The original's `goto`s become the `Step` states, and
/// `ret->port` (tested by `!ret->port`) is kept apart from `has_port`.
pub fn php_url_parse_ex2(str: &[u8]) -> Option<PhpUrl> {
    let mut ret = PhpUrl::default();
    let mut port: u16 = 0;
    let mut has_port = false;
    let ue = str.len();
    let mut s = 0;

    // parse scheme
    let step = match str.find_byte(b':') {
        Some(e) if e != s => 'scheme: {
            // validate scheme
            for &c in &str[s..e] {
                // scheme = 1*[ lowalpha | digit | "+" | "-" | "." ]
                if !c.is_ascii_alphabetic() && !c.is_ascii_digit() && !b"+.-".contains(&c) {
                    if e + 1 < ue && e < binary_strcspn(str, s, ue, b"?#") {
                        break 'scheme Step::ParsePort(e);
                    } else if is_relative_scheme(str, s) {
                        s += 2;
                        break 'scheme Step::ParseHost;
                    } else {
                        break 'scheme Step::JustPath;
                    }
                }
            }

            if e + 1 == ue {
                // only scheme is available
                ret.scheme = Some(component(&str[s..e]));
                return Some(ret);
            }

            // certain schemas like mailto: and zlib: may not have any / after them
            // this check ensures we support those.
            if str[e + 1] != b'/' {
                // check if the data we get is a port this allows us to
                // correctly parse things like a.com:80
                let mut p = e + 1;
                while p < ue && str[p].is_ascii_digit() {
                    p += 1;
                }

                if (p == ue || str[p] == b'/') && (p - e) < 7 {
                    break 'scheme Step::ParsePort(e);
                }

                ret.scheme = Some(component(&str[s..e]));

                s = e + 1;
                Step::JustPath
            } else {
                let scheme = component(&str[s..e]);
                let is_file = scheme.eq_ignore_ascii_case(b"file");
                ret.scheme = Some(scheme);

                if e + 2 < ue && str[e + 2] == b'/' {
                    s = e + 3;
                    if is_file && e + 3 < ue && str[e + 3] == b'/' {
                        // support windows drive letters as in:
                        // file:///c:/somedir/file.txt
                        if e + 5 < ue && str[e + 5] == b':' {
                            s = e + 4;
                        }
                        break 'scheme Step::JustPath;
                    }
                    Step::ParseHost
                } else {
                    s = e + 1;
                    Step::JustPath
                }
            }
        }
        // no scheme; starts with colon: look for port
        Some(e) => Step::ParsePort(e),
        None if is_relative_scheme(str, s) => {
            s += 2;
            Step::ParseHost
        }
        None => Step::JustPath,
    };

    let step = match step {
        Step::ParsePort(e) => {
            let p = e + 1;
            let mut pp = p;

            while pp < ue && pp - p < 6 && str[pp].is_ascii_digit() {
                pp += 1;
            }

            if pp - p > 0 && pp - p < 6 && (pp == ue || str[pp] == b'/') {
                match strtol_port(&str[p..pp]) {
                    Some(parsed) => {
                        has_port = true;
                        port = parsed;
                        if is_relative_scheme(str, s) {
                            s += 2;
                        }
                        Step::ParseHost
                    }
                    None => return None,
                }
            } else if p == pp && pp == ue {
                return None;
            } else if is_relative_scheme(str, s) {
                s += 2;
                Step::ParseHost
            } else {
                Step::JustPath
            }
        }
        step => step,
    };

    if let Step::ParseHost = step {
        let e = binary_strcspn(str, s, ue, b"/?#");

        // check for login and password
        if let Some(p) = str[s..e].rfind_byte(b'@').map(|i| s + i) {
            if let Some(pp) = str[s..p].find_byte(b':').map(|i| s + i) {
                ret.user = Some(component(&str[s..pp]));
                ret.pass = Some(component(&str[pp + 1..p]));
            } else {
                ret.user = Some(component(&str[s..p]));
            }

            s = p + 1;
        }

        // check for port
        let colon = if s < ue && str[s] == b'[' && str[e - 1] == b']' {
            // Short circuit portscan,
            // we're dealing with an
            // IPv6 embedded address
            None
        } else {
            str[s..e].rfind_byte(b':').map(|i| s + i)
        };

        let p = match colon {
            Some(p) => {
                if port == 0 {
                    let digits = &str[p + 1..e];
                    if digits.len() > 5 {
                        // port cannot be longer then 5 characters
                        return None;
                    } else if !digits.is_empty() {
                        match strtol_port(digits) {
                            Some(parsed) => {
                                has_port = true;
                                port = parsed;
                            }
                            None => return None,
                        }
                    }
                }
                p
            }
            None => e,
        };

        // check if we have a valid host, if we don't reject the string as url
        if p < s + 1 {
            return None;
        }

        ret.host = Some(component(&str[s..p]));

        if e == ue {
            ret.port = has_port.then_some(port);
            return Some(ret);
        }

        s = e;
    }

    // just_path
    let mut e = ue;
    if let Some(p) = str[s..e].find_byte(b'#').map(|i| s + i + 1) {
        ret.fragment = Some(if p < e {
            component(&str[p..e])
        } else {
            BString::default()
        });
        e = p - 1;
    }

    if let Some(p) = str[s..e].find_byte(b'?').map(|i| s + i + 1) {
        ret.query = Some(if p < e {
            component(&str[p..e])
        } else {
            BString::default()
        });
        e = p - 1;
    }

    if s < e || s == ue {
        ret.path = Some(component(&str[s..e]));
    }

    ret.port = has_port.then_some(port);
    Some(ret)
}

/// php-src: ext/standard/url.c `binary_strcspn` (PHP 8.5.5)
fn binary_strcspn(str: &[u8], s: usize, mut e: usize, chars: &[u8]) -> usize {
    for &c in chars {
        if let Some(p) = str[s..e].find_byte(c) {
            e = s + p;
        }
    }
    e
}

/// `s + 1 < ue && *s == '/' && *(s + 1) == '/'` of `php_url_parse_ex2`.
fn is_relative_scheme(str: &[u8], s: usize) -> bool {
    str[s..].starts_with(b"//")
}

/// php-src: ext/standard/url.c `php_replace_controlchars` (PHP 8.5.5), applied to a copy as
/// `php_url_parse_ex2` does to each `zend_string_init` result.
fn component(bytes: &[u8]) -> BString {
    bytes
        .iter()
        .map(|&b| if b.is_ascii_control() { b'_' } else { b })
        .collect::<Vec<u8>>()
        .into()
}

/// The port check of `php_url_parse_ex2`: `ZEND_STRTOL(port_buf, &end, 10)` on the copied bytes,
/// accepted when `port >= 0 && port <= 65535 && end != port_buf`.
fn strtol_port(buf: &[u8]) -> Option<u16> {
    // port_buf is NUL-terminated, so strtol stops at an embedded NUL.
    let buf = &buf[..buf.find_byte(0).unwrap_or(buf.len())];
    let mut i = 0;
    while i < buf.len() && matches!(buf[i], b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r') {
        i += 1;
    }
    let negative = match buf.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let digits_start = i;
    let mut value: i64 = 0;
    while i < buf.len() && buf[i].is_ascii_digit() {
        value = value * 10 + i64::from(buf[i] - b'0');
        i += 1;
    }
    if i == digits_start {
        // end == port_buf
        return None;
    }
    let value = if negative { -value } else { value };
    u16::try_from(value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(url: &str) -> Option<PhpUrl> {
        php_url_parse_ex2(url.as_bytes())
    }

    fn some(s: &str) -> Option<BString> {
        Some(s.into())
    }

    #[test]
    fn full_url() {
        assert_eq!(
            Some(PhpUrl {
                scheme: some("https"),
                user: some("user"),
                pass: some("pw"),
                host: some("Example.com"),
                port: Some(8080),
                path: some("/a/b"),
                query: some("q=1"),
                fragment: some("frag"),
            }),
            parse("https://user:pw@Example.com:8080/a/b?q=1#frag"),
        );
    }

    #[test]
    fn host_without_path() {
        let url = parse("http://example.com").unwrap();
        assert_eq!(some("example.com"), url.host);
        assert_eq!(None, url.path);
        assert_eq!(None, url.port);
    }

    #[test]
    fn host_and_port_without_scheme() {
        let url = parse("example.com:80").unwrap();
        assert_eq!(None, url.scheme);
        assert_eq!(some("example.com"), url.host);
        assert_eq!(Some(80), url.port);
    }

    #[test]
    fn relative_and_partial() {
        assert_eq!(some("/just/a/path"), parse("/just/a/path").unwrap().path);
        assert_eq!(some("example.com"), parse("//example.com/x").unwrap().host);
        let mailto = parse("mailto:someone@example.com").unwrap();
        assert_eq!(some("mailto"), mailto.scheme);
        assert_eq!(some("someone@example.com"), mailto.path);
    }

    #[test]
    fn file_urls() {
        let url = parse("file:///tmp/foo").unwrap();
        assert_eq!(some("file"), url.scheme);
        assert_eq!(None, url.host);
        assert_eq!(some("/tmp/foo"), url.path);
        assert_eq!(some("c:/dir"), parse("file:///c:/dir").unwrap().path);
    }

    #[test]
    fn ipv6_host() {
        let url = parse("http://[::1]:8080/").unwrap();
        assert_eq!(some("[::1]"), url.host);
        assert_eq!(Some(8080), url.port);
    }

    #[test]
    fn invalid() {
        assert_eq!(None, parse("http://"));
        assert_eq!(None, parse("http://host:123456"));
        assert_eq!(None, parse("http://host:abc"));
        assert_eq!(None, parse(":"));
    }

    #[test]
    fn empty_query_and_fragment() {
        let url = parse("http://h/p?#").unwrap();
        assert_eq!(some(""), url.query);
        assert_eq!(some(""), url.fragment);
    }

    #[test]
    fn bytes_and_control_characters() {
        let url = php_url_parse_ex2(b"http://h/\xff\x01").unwrap();
        assert_eq!(Some(BString::from(&b"/\xff_"[..])), url.path);
    }
}
