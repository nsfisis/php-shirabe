use std::io::Read as _;
use std::io::Write as _;

/// Handle returned by `gzopen()`. PHP models this as a zlib stream resource;
/// clones share the same underlying stream, like PHP resource copies do.
/// Only the read modes used by Composer are supported.
#[derive(Debug, Clone)]
pub struct GzFile(std::rc::Rc<std::cell::RefCell<flate2::read::MultiGzDecoder<std::fs::File>>>);

pub fn gzopen(file: impl AsRef<std::path::Path>, mode: &str) -> Result<GzFile, std::io::Error> {
    assert!(
        mode.starts_with('r'),
        "gzopen: only read modes are supported (got {mode:?})"
    );
    let f = std::fs::File::open(file.as_ref())?;
    Ok(GzFile(std::rc::Rc::new(std::cell::RefCell::new(
        flate2::read::MultiGzDecoder::new(f),
    ))))
}

/// PHP `gzread()` returns up to `length` bytes of decompressed data, or `false` on
/// error; the `false` case maps to an empty buffer, which stops the caller's read
/// loop just as PHP's falsy check does.
pub fn gzread(file: GzFile, length: i64) -> Vec<u8> {
    let mut decoder = file.0.borrow_mut();
    let mut buf = vec![0u8; length.max(0) as usize];
    let mut filled = 0;
    while filled < buf.len() {
        match decoder.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(_) => {
                filled = 0;
                break;
            }
        }
    }
    buf.truncate(filled);
    buf
}

pub fn gzclose(file: GzFile) {
    drop(file);
}

/// PHP `gzcompress()` with the default level (-1 = zlib default) and ZLIB encoding.
pub fn gzcompress(data: &[u8]) -> Option<Vec<u8>> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).ok()?;
    encoder.finish().ok()
}

/// PHP `bzcompress()` with the default block size (4).
pub fn bzcompress(data: &[u8]) -> Option<Vec<u8>> {
    let mut encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::new(4));
    encoder.write_all(data).ok()?;
    encoder.finish().ok()
}

/// PHP `zlib_decode()` detects the encoding (gzip or zlib) from the data itself.
pub fn zlib_decode(data: &[u8]) -> Option<Vec<u8>> {
    let mut decoded = Vec::new();
    if data.starts_with(&[0x1f, 0x8b]) {
        flate2::read::MultiGzDecoder::new(data)
            .read_to_end(&mut decoded)
            .ok()?;
    } else {
        flate2::read::ZlibDecoder::new(data)
            .read_to_end(&mut decoded)
            .ok()?;
    }
    Some(decoded)
}
