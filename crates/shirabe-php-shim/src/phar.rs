//! The native phar binary layout implemented here follows the php.net manual
//! (`https://www.php.net/manual/en/phar.fileformat.php`); no PHP C sources were referenced.

// TODO(php-semantics): this differs from PHP's `Phar` and `PharData` in details. Its compatibility
// needs to be investigated against the C sources of PHP's `ext/phar`.

use crate::{
    Exception, UnexpectedValueException, bconcat, calculate_hash, display_to_bytes, file_exists,
    impl_php_exception, os_str_from_bytes, path_from_bytes, path_to_bytes, rtrim_chars,
    start_directory, trim_chars,
};
use bstr::{BStr, BString, ByteSlice};
use std::io::{Read as _, Write as _};

#[derive(Debug, Clone)]
enum PharEntryData {
    /// Queued by `build_from_iterator`; the file is read from disk when the archive is written.
    Disk(std::path::PathBuf),
    /// Loaded from an existing archive.
    Memory(Vec<u8>),
    /// A file of an opened tar-based archive: the byte range of its data in `ArchiveSource::Tar`.
    TarRange(std::ops::Range<usize>),
    /// A file of an opened zip-based archive: its index in `ArchiveSource::Zip`.
    ZipIndex(usize),
    Dir,
}

/// The decompressed bytes of an opened tar- or zip-based archive. The content of an entry is read
/// out of them only when it is accessed.
#[derive(Debug)]
enum ArchiveSource {
    Tar(Vec<u8>),
    Zip(std::cell::RefCell<zip::ZipArchive<std::io::Cursor<Vec<u8>>>>),
}

fn entry_content<'a>(
    archive_path: &std::path::Path,
    source: Option<&'a ArchiveSource>,
    data: &'a PharEntryData,
) -> anyhow::Result<std::borrow::Cow<'a, [u8]>> {
    match (data, source) {
        (PharEntryData::Memory(content), _) => Ok(std::borrow::Cow::Borrowed(content)),
        (PharEntryData::TarRange(range), Some(ArchiveSource::Tar(bytes))) => {
            Ok(std::borrow::Cow::Borrowed(&bytes[range.clone()]))
        }
        (PharEntryData::ZipIndex(index), Some(ArchiveSource::Zip(zip))) => {
            let invalid_entry = |e: &dyn std::fmt::Display| {
                corruption_error(
                    archive_path,
                    bconcat!("invalid zip entry: ", display_to_bytes(e)),
                )
            };
            let mut zip = zip.borrow_mut();
            let mut file = zip.by_index(*index).map_err(|e| invalid_entry(&e))?;
            let mut content = Vec::new();
            file.read_to_end(&mut content)
                .map_err(|e| invalid_entry(&e))?;
            Ok(std::borrow::Cow::Owned(content))
        }
        (PharEntryData::Disk(path), _) => Ok(std::borrow::Cow::Owned(std::fs::read(path)?)),
        (PharEntryData::Dir, _) => Ok(std::borrow::Cow::Borrowed(&[])),
        (PharEntryData::TarRange(_) | PharEntryData::ZipIndex(_), _) => {
            unreachable!("an archived entry is read from the archive it was listed from")
        }
    }
}

#[derive(Debug, Clone)]
struct PharEntry {
    localname: BString,
    data: PharEntryData,
    mode: Option<u32>,
    mtime: Option<u64>,
}

fn corruption_error(path: &std::path::Path, detail: impl AsRef<[u8]>) -> anyhow::Error {
    let detail = detail.as_ref();
    UnexpectedValueException::new(bconcat!(
        "internal corruption of phar \"",
        path,
        "\" (",
        detail,
        ")"
    ))
    .into()
}

/// Returns the uncompressed size recorded in the ISIZE trailer of the last gzip member, for use as
/// a capacity hint. ISIZE is the size modulo 2^32 and covers only the last member, so the value is
/// capped and the buffer still grows past it when needed.
fn gzip_isize_hint(bytes: &[u8]) -> usize {
    const MAX_HINT: usize = 64 * 1024 * 1024;
    let Some(trailer) = bytes.last_chunk::<4>() else {
        return 0;
    };
    (u32::from_le_bytes(*trailer) as usize).min(MAX_HINT)
}

/// Reads a tar- or zip-based archive (optionally gzip/bzip2 compressed as a whole) into memory and
/// lists its entries without reading their contents. Returns the entries, the source to read the
/// contents from, and the detected `Phar::TAR`/`Phar::ZIP` format.
fn read_archive_entries(
    path: &std::path::Path,
) -> anyhow::Result<(Vec<PharEntry>, ArchiveSource, i64)> {
    let bytes = std::fs::read(path).map_err(|e| {
        corruption_error(
            path,
            bconcat!("unable to open archive: ", display_to_bytes(e)),
        )
    })?;
    let bytes = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::with_capacity(gzip_isize_hint(&bytes));
        flate2::read::MultiGzDecoder::new(&bytes[..])
            .read_to_end(&mut out)
            .map_err(|e| {
                corruption_error(path, bconcat!("invalid gzip data: ", display_to_bytes(e)))
            })?;
        out
    } else if bytes.starts_with(b"BZh") {
        let mut out = Vec::new();
        bzip2::read::MultiBzDecoder::new(&bytes[..])
            .read_to_end(&mut out)
            .map_err(|e| {
                corruption_error(path, bconcat!("invalid bzip2 data: ", display_to_bytes(e)))
            })?;
        out
    } else {
        bytes
    };

    if bytes.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| {
            corruption_error(path, bconcat!("invalid zip data: ", display_to_bytes(e)))
        })?;
        let mut entries = Vec::new();
        for i in 0..zip.len() {
            let file = zip.by_index_raw(i).map_err(|e| {
                corruption_error(path, bconcat!("invalid zip entry: ", display_to_bytes(e)))
            })?;
            let localname = rtrim_chars(file.name_raw(), "/");
            if file.is_dir() {
                entries.push(PharEntry {
                    localname,
                    data: PharEntryData::Dir,
                    mode: file.unix_mode().map(|m| m & 0o7777),
                    mtime: None,
                });
            } else {
                entries.push(PharEntry {
                    localname,
                    data: PharEntryData::ZipIndex(i),
                    mode: file.unix_mode().map(|m| m & 0o7777),
                    mtime: None,
                });
            }
        }
        return Ok((
            entries,
            ArchiveSource::Zip(std::cell::RefCell::new(zip)),
            Phar::ZIP,
        ));
    }

    let len = bytes.len();
    let mut archive = tar::Archive::new(std::io::Cursor::new(bytes));
    let mut entries = Vec::new();
    for entry in archive
        .entries()
        .map_err(|e| corruption_error(path, bconcat!("invalid tar data: ", display_to_bytes(e))))?
    {
        let mut entry = entry.map_err(|e| {
            corruption_error(path, bconcat!("invalid tar entry: ", display_to_bytes(e)))
        })?;
        let entry_type = entry.header().entry_type();
        // The pax interchange headers (typeflags x and g) are silently ignored, like phar does.
        if entry_type.is_pax_global_extensions() || entry_type.is_pax_local_extensions() {
            continue;
        }
        let localname = rtrim_chars(entry.path_bytes(), "/");
        let mode = entry.header().mode().ok().map(|m| m & 0o7777);
        let mtime = entry.header().mtime().ok();
        if entry_type.is_dir() {
            entries.push(PharEntry {
                localname,
                data: PharEntryData::Dir,
                mode,
                mtime,
            });
        } else if entry_type.is_gnu_sparse() {
            // The data of a sparse file is not stored contiguously, so it is read out here.
            let mut content = Vec::new();
            entry.read_to_end(&mut content).map_err(|e| {
                corruption_error(path, bconcat!("invalid tar entry: ", display_to_bytes(e)))
            })?;
            entries.push(PharEntry {
                localname,
                data: PharEntryData::Memory(content),
                mode,
                mtime,
            });
        } else {
            // Symlink and hardlink entries become empty regular files, matching how
            // PharData::extractTo materializes them.
            let start = entry.raw_file_position() as usize;
            let end = start + entry.size() as usize;
            if end > len {
                return Err(corruption_error(
                    path,
                    BStr::new("invalid tar entry: truncated file contents"),
                ));
            }
            entries.push(PharEntry {
                localname,
                data: PharEntryData::TarRange(start..end),
                mode,
                mtime,
            });
        }
    }
    let bytes = archive.into_inner().into_inner();
    Ok((entries, ArchiveSource::Tar(bytes), Phar::TAR))
}

/// Appends `suffix` to the file name, as phar does when it names the compressed archive
/// (`out.tar` + `.gz` = `out.tar.gz`).
fn sibling_with_suffix(path: &std::path::Path, suffix: &BStr) -> std::path::PathBuf {
    let mut name = path.to_path_buf().into_os_string();
    name.push(os_str_from_bytes(suffix));
    std::path::PathBuf::from(name)
}

fn unix_mtime(time: std::time::SystemTime) -> u64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn extract_entries(
    archive_path: &std::path::Path,
    source: Option<&ArchiveSource>,
    entries: &[PharEntry],
    directory: &std::path::Path,
    files: Option<&[&BStr]>,
    overwrite: bool,
) -> anyhow::Result<()> {
    let extract_error = |detail: BString| {
        PharException::new(bconcat!(
            "Extracting from phar \"",
            archive_path,
            "\" failed: ",
            detail
        ))
        .into()
    };

    if let Some(files) = files
        && let Some(missing) = files.iter().find(|file| {
            !entries
                .iter()
                .any(|entry| entry.localname == *file.as_bytes())
        })
    {
        return Err(extract_error(bconcat!(
            "\"",
            missing,
            "\" is not in the phar"
        )));
    }

    std::fs::create_dir_all(directory).map_err(|e| extract_error(display_to_bytes(e)))?;
    for entry in entries {
        if let Some(files) = files
            && !files
                .iter()
                .any(|file| file.as_bytes() == entry.localname.as_slice())
        {
            continue;
        }
        let rel = path_from_bytes(&entry.localname);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(extract_error(bconcat!(
                "path \"",
                entry.localname,
                "\" is invalid"
            )));
        }
        let dest = directory.join(rel);
        match &entry.data {
            PharEntryData::Dir => {
                std::fs::create_dir_all(&dest).map_err(|e| extract_error(display_to_bytes(e)))?;
            }
            PharEntryData::Memory(_) | PharEntryData::TarRange(_) | PharEntryData::ZipIndex(_) => {
                if !overwrite && dest.exists() {
                    return Err(extract_error(bconcat!("\"", dest, "\" already exists")));
                }
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| extract_error(display_to_bytes(e)))?;
                }
                let content = entry_content(archive_path, source, &entry.data)?;
                std::fs::write(&dest, content).map_err(|e| extract_error(display_to_bytes(e)))?;
            }
            PharEntryData::Disk(src) => {
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| extract_error(display_to_bytes(e)))?;
                }
                std::fs::copy(src, &dest).map_err(|e| extract_error(display_to_bytes(e)))?;
            }
        }
        if let Some(mode) = entry.mode {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(mode & 0o7777))
                .map_err(|e| extract_error(display_to_bytes(e)))?;
        }
    }
    Ok(())
}

fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for (i, slot) in table.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *slot = c;
        }
        table
    });
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc = table[((crc ^ byte as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

struct ByteReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let slice = self.bytes.get(self.pos..self.pos + n)?;
        self.pos += n;
        Some(slice)
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
}

const PHAR_HAS_SIGNATURE: u32 = 0x0001_0000;
const PHAR_FILE_COMPRESSED_GZ: u32 = 0x0000_1000;
const PHAR_FILE_COMPRESSED_BZ2: u32 = 0x0000_2000;

fn verify_phar_signature(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    let broken = || corruption_error(path, BStr::new("phar has a broken or missing signature"));
    let n = bytes.len();
    if n < 8 || &bytes[n - 4..] != b"GBMB" {
        return Err(broken());
    }
    let sig_flags = u32::from_le_bytes(bytes[n - 8..n - 4].try_into().unwrap());
    let algo = match sig_flags {
        0x0001 => "md5",
        0x0002 => "sha1",
        0x0003 => "sha256",
        0x0004 => "sha512",
        // TODO(php-semantics): OPENSSL phar signatures need an RSA verification decision; they are
        // accepted unverified for now.
        0x0010 => return Ok(()),
        _ => return Err(broken()),
    };
    let sig_len = match algo {
        "md5" => 16,
        "sha1" => 20,
        "sha256" => 32,
        _ => 64,
    };
    if n < 8 + sig_len {
        return Err(broken());
    }
    let sig_start = n - 8 - sig_len;
    if calculate_hash(algo, &bytes[..sig_start]) != bytes[sig_start..n - 8] {
        return Err(corruption_error(
            path,
            BStr::new("phar has a broken signature"),
        ));
    }
    Ok(())
}

/// The special token to separate a phar stub and contents.
///
/// The Shirabe executable embeds the Composer runtime bundle as a phar archive, so the token must
/// not appear in the binary in any letter case. See `docs/dev/composer-runtime-bundle.md`. We use
/// `black_box()` to prevent the Rust compiler from inlining the function to a constant.
fn halt_compiler_token() -> [u8; 18] {
    let mut token = *std::hint::black_box(b"__UNYG_PBZCVYRE();");
    for byte in &mut token {
        if byte.is_ascii_uppercase() {
            *byte = b'A' + (*byte - b'A' + 13) % 26;
        }
    }
    token
}

/// Scans the stub for `token`, which phar matches in any letter case.
fn find_halt_compiler_token(blob: &[u8], token: &[u8]) -> Option<usize> {
    // The token starts with a byte that ASCII case folding leaves alone, so scanning for it first
    // narrows the windows that need the full case-insensitive compare.
    memchr::memchr_iter(token[0], blob).find(|&start| {
        blob.get(start..start + token.len())
            .is_some_and(|slice| slice.eq_ignore_ascii_case(token))
    })
}

fn parse_native_phar(path: &std::path::Path) -> anyhow::Result<Vec<PharEntry>> {
    let bytes = std::fs::read(path).map_err(|e| {
        corruption_error(path, bconcat!("unable to open phar: ", display_to_bytes(e)))
    })?;

    let halt = halt_compiler_token();
    let halt_pos = find_halt_compiler_token(&bytes, &halt)
        .ok_or_else(|| corruption_error(path, bconcat!(halt.as_bstr(), " not found in stub")))?;
    let mut offset = halt_pos + halt.len();
    for close_tag in [&b" ?>"[..], &b"\n?>"[..]] {
        if bytes[offset..].starts_with(close_tag) {
            offset += close_tag.len();
            break;
        }
    }
    if bytes[offset..].starts_with(b"\r\n") {
        offset += 2;
    } else if bytes[offset..].starts_with(b"\n") {
        offset += 1;
    }

    let truncated = || corruption_error(path, BStr::new("truncated manifest"));
    let mut reader = ByteReader {
        bytes: &bytes,
        pos: offset,
    };
    let manifest_len = reader.u32().ok_or_else(truncated)? as usize;
    let contents_offset = reader.pos + manifest_len;
    let file_count = reader.u32().ok_or_else(truncated)?;
    let _api_version = reader.u16().ok_or_else(truncated)?;
    let global_flags = reader.u32().ok_or_else(truncated)?;
    let alias_len = reader.u32().ok_or_else(truncated)? as usize;
    reader.take(alias_len).ok_or_else(truncated)?;
    let metadata_len = reader.u32().ok_or_else(truncated)? as usize;
    reader.take(metadata_len).ok_or_else(truncated)?;

    struct RawEntry {
        name: BString,
        size: u32,
        timestamp: u32,
        compressed_size: u32,
        crc: u32,
        flags: u32,
    }
    let mut raw_entries = Vec::new();
    for _ in 0..file_count {
        let name_len = reader.u32().ok_or_else(truncated)? as usize;
        let name = BString::from(reader.take(name_len).ok_or_else(truncated)?);
        let size = reader.u32().ok_or_else(truncated)?;
        let timestamp = reader.u32().ok_or_else(truncated)?;
        let compressed_size = reader.u32().ok_or_else(truncated)?;
        let crc = reader.u32().ok_or_else(truncated)?;
        let flags = reader.u32().ok_or_else(truncated)?;
        let metadata_len = reader.u32().ok_or_else(truncated)? as usize;
        reader.take(metadata_len).ok_or_else(truncated)?;
        raw_entries.push(RawEntry {
            name,
            size,
            timestamp,
            compressed_size,
            crc,
            flags,
        });
    }

    if global_flags & PHAR_HAS_SIGNATURE != 0 {
        verify_phar_signature(path, &bytes)?;
    }

    let mut entries = Vec::new();
    let mut offset = contents_offset;
    for raw in raw_entries {
        let end = offset + raw.compressed_size as usize;
        let data = bytes
            .get(offset..end)
            .ok_or_else(|| corruption_error(path, BStr::new("truncated file contents")))?;
        offset = end;
        let mode = Some(raw.flags & 0o777);
        let mtime = Some(raw.timestamp as u64);
        if raw.name.ends_with(b"/") {
            entries.push(PharEntry {
                localname: rtrim_chars(&raw.name, "/"),
                data: PharEntryData::Dir,
                mode,
                mtime,
            });
            continue;
        }
        let content = if raw.flags & PHAR_FILE_COMPRESSED_GZ != 0 {
            let mut out = Vec::new();
            flate2::read::DeflateDecoder::new(data)
                .read_to_end(&mut out)
                .map_err(|e| {
                    corruption_error(
                        path,
                        bconcat!("invalid deflate data: ", display_to_bytes(e)),
                    )
                })?;
            out
        } else if raw.flags & PHAR_FILE_COMPRESSED_BZ2 != 0 {
            let mut out = Vec::new();
            bzip2::read::BzDecoder::new(data)
                .read_to_end(&mut out)
                .map_err(|e| {
                    corruption_error(path, bconcat!("invalid bzip2 data: ", display_to_bytes(e)))
                })?;
            out
        } else {
            data.to_vec()
        };
        if content.len() != raw.size as usize {
            return Err(corruption_error(path, BStr::new("file size mismatch")));
        }
        if crc32(&content) != raw.crc {
            return Err(corruption_error(path, BStr::new("crc32 mismatch")));
        }
        entries.push(PharEntry {
            localname: raw.name,
            data: PharEntryData::Memory(content),
            mode,
            mtime,
        });
    }
    Ok(entries)
}

#[derive(Debug)]
pub struct Phar {
    path: std::path::PathBuf,
    entries: Vec<PharEntry>,
}

impl Phar {
    pub const ZIP: i64 = 1;
    pub const TAR: i64 = 2;
    pub const GZ: i64 = 4096;
    pub const BZ2: i64 = 8192;

    pub fn new(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let entries = parse_native_phar(&path)?;
        Ok(Self { path, entries })
    }

    pub fn extract_to(
        &self,
        directory: impl AsRef<std::path::Path>,
        files: Option<&[&BStr]>,
        overwrite: bool,
    ) -> anyhow::Result<()> {
        extract_entries(
            &self.path,
            None,
            &self.entries,
            directory.as_ref(),
            files,
            overwrite,
        )
    }
}

#[derive(Debug)]
pub struct PharException {
    inner: Exception,
}

impl PharException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self {
            inner: Exception::new(message),
        }
    }
}

impl_php_exception!(PharException, inner, r"PharException");

#[derive(Debug)]
pub struct PharFileInfo {
    file_name: BString,
    archive_path: std::path::PathBuf,
    source: Option<std::rc::Rc<ArchiveSource>>,
    data: PharEntryData,
}

impl PharFileInfo {
    pub fn get_content(&self) -> anyhow::Result<Vec<u8>> {
        Ok(entry_content(&self.archive_path, self.source.as_deref(), &self.data)?.into_owned())
    }

    pub fn get_basename(&self) -> BString {
        match self.file_name.rfind_byte(b'/') {
            Some(slash) => BString::from(&self.file_name[slash + 1..]),
            None => self.file_name.clone(),
        }
    }

    pub fn is_dir(&self) -> bool {
        matches!(self.data, PharEntryData::Dir)
    }
}

#[derive(Debug)]
pub struct PharData {
    path: std::path::PathBuf,
    format: i64,
    entries: std::cell::RefCell<Vec<PharEntry>>,
    source: Option<std::rc::Rc<ArchiveSource>>,
}

impl PharData {
    pub fn new(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        Self::open(path.as_ref().to_path_buf(), None)
    }

    pub fn new_with_format(
        path: impl AsRef<std::path::Path>,
        _flags: i64,
        _alias: impl AsRef<[u8]>,
        format: i64,
    ) -> anyhow::Result<Self> {
        let _alias = BStr::new(_alias.as_ref());
        Self::open(path.as_ref().to_path_buf(), Some(format))
    }

    fn open(path: std::path::PathBuf, format: Option<i64>) -> anyhow::Result<Self> {
        if file_exists(&path) {
            let (entries, source, detected_format) = read_archive_entries(&path)?;
            return Ok(Self {
                path,
                format: detected_format,
                entries: std::cell::RefCell::new(entries),
                source: Some(std::rc::Rc::new(source)),
            });
        }
        let parent_exists = match path.parent() {
            Some(parent) if parent.as_os_str().is_empty() => true,
            Some(parent) => parent.is_dir(),
            None => false,
        };
        if !parent_exists {
            return Err(UnexpectedValueException::new(bconcat!(
                "Cannot create phar '",
                path,
                "', file extension (or combination) not recognised or the directory does not exist"
            ))
            .into());
        }
        let format = format.unwrap_or(if path.as_os_str().as_encoded_bytes().ends_with(b".zip") {
            Phar::ZIP
        } else {
            Phar::TAR
        });
        Ok(Self {
            path,
            format,
            entries: std::cell::RefCell::new(Vec::new()),
            source: None,
        })
    }

    pub fn can_compress(algo: i64) -> bool {
        matches!(algo, Phar::GZ | Phar::BZ2)
    }

    pub fn valid(&self) -> bool {
        !self.entries.borrow().is_empty()
    }

    pub fn get(&self, key: impl AsRef<[u8]>) -> Option<PharFileInfo> {
        let entries = self.entries.borrow();
        let key = rtrim_chars(key, "/");
        for entry in entries.iter() {
            if entry.localname == key {
                return Some(self.file_info(entry.localname.clone(), entry.data.clone()));
            }
        }
        let prefix = bconcat!(&key, "/");
        if entries
            .iter()
            .any(|e| e.localname.starts_with(prefix.as_slice()))
        {
            return Some(self.file_info(key, PharEntryData::Dir));
        }
        None
    }

    /// Iterates the top level of the archive (files, plus explicit and implicit
    /// directories) in sorted order, like a `DirectoryIterator` over the phar root.
    pub fn iter(&self) -> impl Iterator<Item = PharFileInfo> {
        let entries = self.entries.borrow();
        let mut top_level: indexmap::IndexMap<BString, PharEntryData> = indexmap::IndexMap::new();
        for entry in entries.iter() {
            match entry.localname.find_byte(b'/') {
                Some(slash) => {
                    top_level
                        .entry(BString::from(&entry.localname[..slash]))
                        .or_insert(PharEntryData::Dir);
                }
                None => {
                    top_level.insert(entry.localname.clone(), entry.data.clone());
                }
            }
        }
        let mut items: Vec<PharFileInfo> = top_level
            .into_iter()
            .map(|(file_name, data)| self.file_info(file_name, data))
            .collect();
        items.sort_by(|a, b| a.file_name.cmp(&b.file_name));
        items.into_iter()
    }

    fn file_info(&self, file_name: BString, data: PharEntryData) -> PharFileInfo {
        PharFileInfo {
            file_name,
            archive_path: self.path.clone(),
            source: self.source.clone(),
            data,
        }
    }

    pub fn extract_to(
        &self,
        directory: impl AsRef<std::path::Path>,
        files: Option<&[&BStr]>,
        overwrite: bool,
    ) -> anyhow::Result<()> {
        extract_entries(
            &self.path,
            self.source.as_deref(),
            &self.entries.borrow(),
            directory.as_ref(),
            files,
            overwrite,
        )
    }

    pub fn add_empty_dir(&self, dirname: impl AsRef<[u8]>) -> anyhow::Result<()> {
        let localname = trim_chars(dirname, "/");
        {
            let mut entries = self.entries.borrow_mut();
            if !entries.iter().any(|e| e.localname == localname) {
                entries.push(PharEntry {
                    localname,
                    data: PharEntryData::Dir,
                    mode: Some(0o777),
                    mtime: None,
                });
            }
        }
        self.flush()
    }

    pub fn build_from_iterator(
        &self,
        iter: &mut dyn Iterator<Item = std::path::PathBuf>,
        base_directory: impl AsRef<std::path::Path>,
    ) -> anyhow::Result<()> {
        let base_directory = base_directory.as_ref();
        {
            let mut entries = self.entries.borrow_mut();
            for file in iter {
                let localname = BString::from(path_to_bytes(
                    file.strip_prefix(base_directory)
                        .map_err(|_| -> anyhow::Error {
                            UnexpectedValueException::new(bconcat!(
                                "Iterator returned a path \"",
                                file,
                                "\" that is not in the base directory \"",
                                base_directory,
                                "\""
                            ))
                            .into()
                        })?,
                ));
                entries.push(PharEntry {
                    localname,
                    data: PharEntryData::Disk(file),
                    mode: None,
                    mtime: None,
                });
            }
        }
        self.flush()
    }

    /// Compresses the entire tar archive into a sibling file with the added
    /// `.gz`/`.bz2` extension; the uncompressed archive is left in place.
    pub fn compress(&self, algo: i64) -> anyhow::Result<()> {
        assert_eq!(
            self.format,
            Phar::TAR,
            "PharData::compress: only tar-based archives can be compressed as a whole"
        );
        let tar_bytes = self.build_tar_bytes()?;
        let write_error = |e: std::io::Error| -> anyhow::Error {
            PharException::new(bconcat!(
                "Unable to compress phar archive \"",
                self.path,
                "\": ",
                display_to_bytes(e)
            ))
            .into()
        };
        let (target, compressed) = match algo {
            Phar::GZ => {
                let mut encoder =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                encoder.write_all(&tar_bytes).map_err(write_error)?;
                (
                    sibling_with_suffix(&self.path, BStr::new(".gz")),
                    encoder.finish().map_err(write_error)?,
                )
            }
            Phar::BZ2 => {
                let mut encoder =
                    bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::new(4));
                encoder.write_all(&tar_bytes).map_err(write_error)?;
                (
                    sibling_with_suffix(&self.path, BStr::new(".bz2")),
                    encoder.finish().map_err(write_error)?,
                )
            }
            _ => panic!("PharData::compress: unsupported compression algorithm {algo}"),
        };
        std::fs::write(target, compressed).map_err(write_error)?;
        Ok(())
    }

    /// Writes the in-memory entries out to `self.path`. Like phar, nothing is
    /// written as long as the archive holds no entries.
    fn flush(&self) -> anyhow::Result<()> {
        if self.entries.borrow().is_empty() {
            return Ok(());
        }
        if self.format == Phar::ZIP {
            return self.write_zip();
        }
        let bytes = self.build_tar_bytes()?;
        std::fs::write(&self.path, bytes).map_err(|e| {
            PharException::new(bconcat!(
                "Unable to write phar archive \"",
                self.path,
                "\": ",
                display_to_bytes(e)
            ))
            .into()
        })
    }

    fn build_tar_bytes(&self) -> anyhow::Result<Vec<u8>> {
        let write_error = |e: std::io::Error| {
            PharException::new(bconcat!(
                "Unable to write phar archive \"",
                self.path,
                "\": ",
                display_to_bytes(e)
            ))
            .into()
        };
        let mut builder = tar::Builder::new(Vec::new());
        for entry in self.entries.borrow().iter() {
            let mut header = tar::Header::new_ustar();
            header.set_uid(0);
            header.set_gid(0);
            match &entry.data {
                PharEntryData::Disk(source) => {
                    use std::os::unix::fs::PermissionsExt;
                    let metadata = std::fs::metadata(source).map_err(write_error)?;
                    header.set_entry_type(tar::EntryType::Regular);
                    header.set_size(metadata.len());
                    header.set_mode(metadata.permissions().mode() & 0o7777);
                    header.set_mtime(
                        metadata
                            .modified()
                            .map(unix_mtime)
                            .unwrap_or_else(|_| unix_mtime(std::time::SystemTime::now())),
                    );
                    let file = std::fs::File::open(source).map_err(write_error)?;
                    builder
                        .append_data(&mut header, path_from_bytes(&entry.localname), file)
                        .map_err(write_error)?;
                }
                PharEntryData::Memory(_)
                | PharEntryData::TarRange(_)
                | PharEntryData::ZipIndex(_) => {
                    let content = entry_content(&self.path, self.source.as_deref(), &entry.data)?;
                    header.set_entry_type(tar::EntryType::Regular);
                    header.set_size(content.len() as u64);
                    header.set_mode(entry.mode.unwrap_or(0o644));
                    header.set_mtime(
                        entry
                            .mtime
                            .unwrap_or_else(|| unix_mtime(std::time::SystemTime::now())),
                    );
                    builder
                        .append_data(&mut header, path_from_bytes(&entry.localname), &content[..])
                        .map_err(write_error)?;
                }
                PharEntryData::Dir => {
                    header.set_entry_type(tar::EntryType::Directory);
                    header.set_size(0);
                    header.set_mode(entry.mode.unwrap_or(0o777));
                    header.set_mtime(
                        entry
                            .mtime
                            .unwrap_or_else(|| unix_mtime(std::time::SystemTime::now())),
                    );
                    builder
                        .append_data(
                            &mut header,
                            path_from_bytes(&entry.localname),
                            std::io::empty(),
                        )
                        .map_err(write_error)?;
                }
            }
        }
        builder.into_inner().map_err(write_error)
    }

    fn write_zip(&self) -> anyhow::Result<()> {
        let write_error = |e: BString| -> anyhow::Error {
            PharException::new(bconcat!(
                "Unable to write phar archive \"",
                self.path,
                "\": ",
                e
            ))
            .into()
        };
        let file =
            std::fs::File::create(&self.path).map_err(|e| write_error(display_to_bytes(e)))?;
        let mut writer = zip::ZipWriter::new(file);
        for entry in self.entries.borrow().iter() {
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            let localname = entry.localname.as_bstr();
            match &entry.data {
                PharEntryData::Disk(source) => {
                    use std::os::unix::fs::PermissionsExt;
                    let metadata =
                        std::fs::metadata(source).map_err(|e| write_error(display_to_bytes(e)))?;
                    writer
                        .start_file_with_options(
                            options
                                .name(localname)
                                .unix_permissions(metadata.permissions().mode() & 0o7777),
                        )
                        .map_err(|e| write_error(display_to_bytes(e)))?;
                    let mut file = std::fs::File::open(source)
                        .map_err(|e| write_error(display_to_bytes(e)))?;
                    std::io::copy(&mut file, &mut writer)
                        .map_err(|e| write_error(display_to_bytes(e)))?;
                }
                PharEntryData::Memory(_)
                | PharEntryData::TarRange(_)
                | PharEntryData::ZipIndex(_) => {
                    let content = entry_content(&self.path, self.source.as_deref(), &entry.data)?;
                    let options = match entry.mode {
                        Some(mode) => options.unix_permissions(mode),
                        None => options,
                    };
                    writer
                        .start_file_with_options(options.name(localname))
                        .map_err(|e| write_error(display_to_bytes(e)))?;
                    writer
                        .write_all(&content)
                        .map_err(|e| write_error(display_to_bytes(e)))?;
                }
                PharEntryData::Dir => {
                    start_directory(
                        &mut writer,
                        localname,
                        options.external_attributes(0o040755 << 16),
                    )
                    .map_err(|e| write_error(display_to_bytes(e)))?;
                }
            }
        }
        writer
            .finish()
            .map_err(|e| write_error(display_to_bytes(e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Catch as _, error_message};

    #[test]
    fn tar_entry_name_keeps_bytes_that_are_not_valid_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let file = src.join(path_from_bytes(b"caf\xE9.txt"));
        std::fs::write(&file, b"hello").unwrap();
        let tar_path = dir.path().join("out.tar");

        let phar = PharData::new(&tar_path).unwrap();
        phar.build_from_iterator(&mut vec![file].into_iter(), &src)
            .unwrap();

        let read_back = PharData::new(&tar_path).unwrap();
        assert_eq!(
            read_back
                .get(b"caf\xE9.txt")
                .unwrap()
                .get_content()
                .unwrap(),
            b"hello"
        );
    }

    #[test]
    fn zip_entry_name_keeps_bytes_that_are_not_valid_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let file = src.join(path_from_bytes(b"caf\xE9.txt"));
        std::fs::write(&file, b"hello").unwrap();
        let zip_path = dir.path().join("out.zip");

        let phar = PharData::new(&zip_path).unwrap();
        phar.build_from_iterator(&mut vec![file].into_iter(), &src)
            .unwrap();
        phar.add_empty_dir(b"d\xE9").unwrap();

        let read_back = PharData::new(&zip_path).unwrap();
        assert_eq!(
            read_back
                .get(b"caf\xE9.txt")
                .unwrap()
                .get_content()
                .unwrap(),
            b"hello"
        );
        let archive = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
        let directory = archive
            .by_index_data(archive.index_for_name_raw(b"d\xE9/").unwrap())
            .unwrap();
        assert!(directory.is_dir());
        assert_eq!(directory.unix_mode(), Some(0o040755));
    }

    #[test]
    fn halt_compiler_token_decodes() {
        assert_eq!(
            halt_compiler_token(),
            [
                b'_', b'_', b'H', b'A', b'L', b'T', b'_', b'C', b'O', b'M', b'P', b'I', b'L', b'E',
                b'R', b'(', b')', b';',
            ],
        );
    }

    fn write_file(dir: &std::path::Path, name: &BStr, content: &[u8]) -> std::path::PathBuf {
        let path = dir.join(path_from_bytes(name));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn phar_data_tar_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let a = write_file(&src, BStr::new("a.txt"), b"hello");
        let b = write_file(&src, BStr::new("sub/b.txt"), b"world");
        let tar_path = dir.path().join("out.tar");

        let phar = PharData::new(&tar_path).unwrap();
        assert!(!phar.valid());
        phar.build_from_iterator(&mut vec![a, b].into_iter(), &src)
            .unwrap();
        phar.add_empty_dir("emptydir").unwrap();
        assert!(tar_path.exists());

        let read_back = PharData::new(&tar_path).unwrap();
        assert!(read_back.valid());
        assert_eq!(
            read_back.get("a.txt").unwrap().get_content().unwrap(),
            b"hello"
        );
        assert_eq!(
            read_back.get("sub/b.txt").unwrap().get_content().unwrap(),
            b"world"
        );
        assert!(read_back.get("sub").unwrap().is_dir());
        assert!(read_back.get("missing").is_none());
        let top: Vec<(BString, bool)> = read_back
            .iter()
            .map(|info| (info.get_basename(), info.is_dir()))
            .collect();
        assert_eq!(
            top,
            vec![
                ("a.txt".into(), false),
                ("emptydir".into(), true),
                ("sub".into(), true),
            ]
        );

        let out = dir.path().join("extracted");
        read_back.extract_to(&out, None, true).unwrap();
        assert_eq!(std::fs::read(out.join("a.txt")).unwrap(), b"hello");
        assert_eq!(std::fs::read(out.join("sub/b.txt")).unwrap(), b"world");
        assert!(out.join("emptydir").is_dir());
    }

    #[test]
    fn phar_data_compress_creates_sibling_archives() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let a = write_file(&src, BStr::new("a.txt"), b"hello");
        let tar_path = dir.path().join("out.tar");

        let phar = PharData::new(&tar_path).unwrap();
        phar.build_from_iterator(&mut vec![a].into_iter(), &src)
            .unwrap();
        phar.compress(Phar::GZ).unwrap();
        phar.compress(Phar::BZ2).unwrap();

        assert!(tar_path.exists());
        for compressed in ["out.tar.gz", "out.tar.bz2"] {
            let read_back = PharData::new(dir.path().join(compressed)).unwrap();
            assert_eq!(
                read_back.get("a.txt").unwrap().get_content().unwrap(),
                b"hello"
            );
        }
    }

    #[test]
    fn phar_data_missing_file_in_missing_directory_is_rejected() {
        let error = PharData::new("/nonexistent-dir/foo.tar").unwrap_err();
        assert!(
            error
                .catch::<UnexpectedValueException>()
                .unwrap()
                .get_message()
                .starts_with(b"Cannot create phar")
        );
    }

    /// Builds a minimal native phar (one stored file, one deflated file, SHA-1
    /// signature) following the php.net manual layout, with `token` as the stub token.
    fn build_native_phar(tampered: bool, token: [u8; 18]) -> Vec<u8> {
        let stored = (b"Hello World".to_vec(), "dir/hello.txt");
        let big = "abc".repeat(1000).into_bytes();
        let mut deflated = Vec::new();
        {
            let mut encoder =
                flate2::write::DeflateEncoder::new(&mut deflated, flate2::Compression::default());
            encoder.write_all(&big).unwrap();
            encoder.finish().unwrap();
        }

        let mut manifest = Vec::new();
        manifest.extend_from_slice(&2u32.to_le_bytes());
        manifest.extend_from_slice(&[0x11, 0x10]);
        manifest.extend_from_slice(&(PHAR_HAS_SIGNATURE | 0x1000).to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes());
        for (name, size, csize, crc, flags) in [
            (
                stored.1,
                stored.0.len(),
                stored.0.len(),
                crc32(&stored.0),
                0o644u32,
            ),
            (
                "big.txt",
                big.len(),
                deflated.len(),
                crc32(&big),
                0o644 | PHAR_FILE_COMPRESSED_GZ,
            ),
        ] {
            manifest.extend_from_slice(&(name.len() as u32).to_le_bytes());
            manifest.extend_from_slice(name.as_bytes());
            manifest.extend_from_slice(&(size as u32).to_le_bytes());
            manifest.extend_from_slice(&0u32.to_le_bytes());
            manifest.extend_from_slice(&(csize as u32).to_le_bytes());
            manifest.extend_from_slice(&crc.to_le_bytes());
            manifest.extend_from_slice(&flags.to_le_bytes());
            manifest.extend_from_slice(&0u32.to_le_bytes());
        }

        let mut bytes = b"<?php ".to_vec();
        bytes.extend_from_slice(&token);
        bytes.extend_from_slice(b" ?>\r\n");
        bytes.extend_from_slice(&(manifest.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&manifest);
        bytes.extend_from_slice(&stored.0);
        bytes.extend_from_slice(&deflated);
        let signature = calculate_hash("sha1", &bytes);
        if tampered {
            let content_start = bytes.len() - stored.0.len() - deflated.len();
            bytes[content_start] ^= 0xFF;
        }
        bytes.extend_from_slice(&signature);
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(b"GBMB");
        bytes
    }

    #[test]
    fn phar_native_read_and_extract() {
        let dir = tempfile::tempdir().unwrap();
        let phar_path = dir.path().join("test.phar");
        std::fs::write(&phar_path, build_native_phar(false, halt_compiler_token())).unwrap();

        let phar = Phar::new(&phar_path).unwrap();
        let out = dir.path().join("extracted");
        phar.extract_to(&out, None, true).unwrap();
        assert_eq!(
            std::fs::read(out.join("dir/hello.txt")).unwrap(),
            b"Hello World"
        );
        assert_eq!(
            std::fs::read(out.join("big.txt")).unwrap(),
            "abc".repeat(1000).into_bytes()
        );
    }

    #[test]
    fn phar_native_stub_token_is_read_in_any_letter_case() {
        let dir = tempfile::tempdir().unwrap();
        let mut token = halt_compiler_token();
        token.make_ascii_lowercase();
        let phar_path = dir.path().join("lowercase.phar");
        std::fs::write(&phar_path, build_native_phar(false, token)).unwrap();

        let out = dir.path().join("extracted");
        Phar::new(&phar_path)
            .unwrap()
            .extract_to(&out, None, true)
            .unwrap();

        assert_eq!(
            std::fs::read(out.join("dir/hello.txt")).unwrap(),
            b"Hello World"
        );
    }

    #[test]
    fn phar_native_extract_takes_the_named_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let phar_path = dir.path().join("selected.phar");
        std::fs::write(&phar_path, build_native_phar(false, halt_compiler_token())).unwrap();
        let phar = Phar::new(&phar_path).unwrap();

        let out = dir.path().join("extracted");
        phar.extract_to(&out, Some(&[BStr::new("dir/hello.txt")]), true)
            .unwrap();
        assert_eq!(
            std::fs::read(out.join("dir/hello.txt")).unwrap(),
            b"Hello World"
        );
        assert!(!out.join("big.txt").exists());

        let error = phar
            .extract_to(&out, Some(&[BStr::new("dir/absent.txt")]), true)
            .unwrap_err();
        assert!(
            error_message(&error).contains_str("\"dir/absent.txt\" is not in the phar"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn phar_native_broken_signature_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let phar_path = dir.path().join("tampered.phar");
        std::fs::write(&phar_path, build_native_phar(true, halt_compiler_token())).unwrap();

        let error = Phar::new(&phar_path).unwrap_err();
        assert!(
            error
                .catch::<UnexpectedValueException>()
                .unwrap()
                .get_message()
                .contains_str("broken signature")
        );
    }
}
