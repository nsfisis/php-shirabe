//! The native phar binary layout implemented here follows the php.net manual
//! (`https://www.php.net/manual/en/phar.fileformat.php`); no PHP C sources were referenced.

use crate::UnexpectedValueException;
use std::io::Read as _;
use std::io::Write as _;

#[derive(Debug, Clone)]
enum PharEntryData {
    /// Queued by `build_from_iterator`; the file is read from disk when the archive is written.
    Disk(std::path::PathBuf),
    /// Loaded from an existing archive.
    Memory(Vec<u8>),
    Dir,
}

#[derive(Debug, Clone)]
struct PharEntry {
    localname: String,
    data: PharEntryData,
    mode: Option<u32>,
    mtime: Option<u64>,
}

fn corruption_error(path: &std::path::Path, detail: &str) -> anyhow::Error {
    UnexpectedValueException::new(format!(
        "internal corruption of phar \"{}\" ({})",
        path.display(),
        detail
    ))
    .into()
}

/// Reads a tar- or zip-based archive (optionally gzip/bzip2 compressed as a whole)
/// into memory. Returns the entries and the detected `Phar::TAR`/`Phar::ZIP` format.
fn read_archive_entries(path: &std::path::Path) -> anyhow::Result<(Vec<PharEntry>, i64)> {
    let bytes = std::fs::read(path)
        .map_err(|e| corruption_error(path, &format!("unable to open archive: {}", e)))?;
    let bytes = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::MultiGzDecoder::new(&bytes[..])
            .read_to_end(&mut out)
            .map_err(|e| corruption_error(path, &format!("invalid gzip data: {}", e)))?;
        out
    } else if bytes.starts_with(b"BZh") {
        let mut out = Vec::new();
        bzip2::read::MultiBzDecoder::new(&bytes[..])
            .read_to_end(&mut out)
            .map_err(|e| corruption_error(path, &format!("invalid bzip2 data: {}", e)))?;
        out
    } else {
        bytes
    };

    if bytes.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|e| corruption_error(path, &format!("invalid zip data: {}", e)))?;
        let mut entries = Vec::new();
        for i in 0..zip.len() {
            let mut file = zip
                .by_index(i)
                .map_err(|e| corruption_error(path, &format!("invalid zip entry: {}", e)))?;
            let localname = file.name().trim_end_matches('/').to_string();
            if file.is_dir() {
                entries.push(PharEntry {
                    localname,
                    data: PharEntryData::Dir,
                    mode: file.unix_mode().map(|m| m & 0o7777),
                    mtime: None,
                });
            } else {
                let mut content = Vec::new();
                file.read_to_end(&mut content)
                    .map_err(|e| corruption_error(path, &format!("invalid zip entry: {}", e)))?;
                entries.push(PharEntry {
                    localname,
                    data: PharEntryData::Memory(content),
                    mode: file.unix_mode().map(|m| m & 0o7777),
                    mtime: None,
                });
            }
        }
        return Ok((entries, Phar::ZIP));
    }

    let mut archive = tar::Archive::new(std::io::Cursor::new(bytes));
    let mut entries = Vec::new();
    for entry in archive
        .entries()
        .map_err(|e| corruption_error(path, &format!("invalid tar data: {}", e)))?
    {
        let mut entry =
            entry.map_err(|e| corruption_error(path, &format!("invalid tar entry: {}", e)))?;
        let entry_type = entry.header().entry_type();
        // The pax interchange headers (typeflags x and g) are silently ignored, like phar does.
        if entry_type.is_pax_global_extensions() || entry_type.is_pax_local_extensions() {
            continue;
        }
        let localname = entry
            .path()
            .map_err(|e| corruption_error(path, &format!("invalid tar entry name: {}", e)))?
            .to_string_lossy()
            .trim_end_matches('/')
            .to_string();
        let mode = entry.header().mode().ok().map(|m| m & 0o7777);
        let mtime = entry.header().mtime().ok();
        if entry_type.is_dir() {
            entries.push(PharEntry {
                localname,
                data: PharEntryData::Dir,
                mode,
                mtime,
            });
        } else {
            // Symlink and hardlink entries become empty regular files, matching how
            // PharData::extractTo materializes them.
            let mut content = Vec::new();
            entry
                .read_to_end(&mut content)
                .map_err(|e| corruption_error(path, &format!("invalid tar entry: {}", e)))?;
            entries.push(PharEntry {
                localname,
                data: PharEntryData::Memory(content),
                mode,
                mtime,
            });
        }
    }
    Ok((entries, Phar::TAR))
}

/// Appends `suffix` to the file name, as phar does when it names the compressed archive
/// (`out.tar` + `.gz` = `out.tar.gz`).
fn sibling_with_suffix(path: &std::path::Path, suffix: &str) -> std::path::PathBuf {
    let mut name = path.to_path_buf().into_os_string();
    name.push(suffix);
    std::path::PathBuf::from(name)
}

fn unix_mtime(time: std::time::SystemTime) -> u64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn extract_entries(
    archive_path: &std::path::Path,
    entries: &[PharEntry],
    directory: &std::path::Path,
    overwrite: bool,
) -> anyhow::Result<()> {
    let extract_error = |detail: String| {
        PharException::new(format!(
            "Extracting from phar \"{}\" failed: {}",
            archive_path.display(),
            detail
        ))
        .into()
    };

    std::fs::create_dir_all(directory).map_err(|e| extract_error(e.to_string()))?;
    for entry in entries {
        let rel = std::path::Path::new(&entry.localname);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(extract_error(format!(
                "path \"{}\" is invalid",
                entry.localname
            )));
        }
        let dest = directory.join(rel);
        match &entry.data {
            PharEntryData::Dir => {
                std::fs::create_dir_all(&dest).map_err(|e| extract_error(e.to_string()))?;
            }
            PharEntryData::Memory(content) => {
                if !overwrite && dest.exists() {
                    return Err(extract_error(format!(
                        "\"{}\" already exists",
                        dest.display()
                    )));
                }
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| extract_error(e.to_string()))?;
                }
                std::fs::write(&dest, content).map_err(|e| extract_error(e.to_string()))?;
            }
            PharEntryData::Disk(src) => {
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| extract_error(e.to_string()))?;
                }
                std::fs::copy(src, &dest).map_err(|e| extract_error(e.to_string()))?;
            }
        }
        if let Some(mode) = entry.mode {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(mode & 0o7777))
                .map_err(|e| extract_error(e.to_string()))?;
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
    let broken = || corruption_error(path, "phar has a broken or missing signature");
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
        // TODO(phase-c): OPENSSL phar signatures need an RSA verification decision; they are
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
    if crate::hash::calculate_hash(algo, &bytes[..sig_start]) != bytes[sig_start..n - 8] {
        return Err(corruption_error(path, "phar has a broken signature"));
    }
    Ok(())
}

fn parse_native_phar(path: &std::path::Path) -> anyhow::Result<Vec<PharEntry>> {
    let bytes = std::fs::read(path)
        .map_err(|e| corruption_error(path, &format!("unable to open phar: {}", e)))?;

    let halt = b"__HALT_COMPILER();";
    let halt_pos = bytes
        .windows(halt.len())
        .position(|window| window == halt)
        .ok_or_else(|| corruption_error(path, "__HALT_COMPILER(); not found in stub"))?;
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

    let truncated = || corruption_error(path, "truncated manifest");
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
        name: String,
        size: u32,
        timestamp: u32,
        compressed_size: u32,
        crc: u32,
        flags: u32,
    }
    let mut raw_entries = Vec::new();
    for _ in 0..file_count {
        let name_len = reader.u32().ok_or_else(truncated)? as usize;
        let name =
            String::from_utf8_lossy(reader.take(name_len).ok_or_else(truncated)?).into_owned();
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
            .ok_or_else(|| corruption_error(path, "truncated file contents"))?;
        offset = end;
        let mode = Some(raw.flags & 0o777);
        let mtime = Some(raw.timestamp as u64);
        if raw.name.ends_with('/') {
            entries.push(PharEntry {
                localname: raw.name.trim_end_matches('/').to_string(),
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
                .map_err(|e| corruption_error(path, &format!("invalid deflate data: {}", e)))?;
            out
        } else if raw.flags & PHAR_FILE_COMPRESSED_BZ2 != 0 {
            let mut out = Vec::new();
            bzip2::read::BzDecoder::new(data)
                .read_to_end(&mut out)
                .map_err(|e| corruption_error(path, &format!("invalid bzip2 data: {}", e)))?;
            out
        } else {
            data.to_vec()
        };
        if content.len() != raw.size as usize {
            return Err(corruption_error(path, "file size mismatch"));
        }
        if crc32(&content) != raw.crc {
            return Err(corruption_error(path, "crc32 mismatch"));
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
        _files: Option<()>,
        overwrite: bool,
    ) -> anyhow::Result<()> {
        extract_entries(&self.path, &self.entries, directory.as_ref(), overwrite)
    }
}

#[derive(Debug)]
pub struct PharException {
    inner: crate::Exception,
}

impl PharException {
    pub fn new(message: String) -> Self {
        Self {
            inner: crate::Exception::new(message),
        }
    }
}

crate::impl_php_exception!(PharException, inner, r"PharException");

#[derive(Debug)]
pub struct PharFileInfo {
    file_name: String,
    is_dir: bool,
    content: Vec<u8>,
}

impl PharFileInfo {
    pub fn get_content(&self) -> Vec<u8> {
        self.content.clone()
    }

    pub fn get_basename(&self) -> String {
        self.file_name
            .rsplit('/')
            .next()
            .unwrap_or(&self.file_name)
            .to_string()
    }

    pub fn is_dir(&self) -> bool {
        self.is_dir
    }
}

#[derive(Debug)]
pub struct PharData {
    path: std::path::PathBuf,
    format: i64,
    entries: std::cell::RefCell<Vec<PharEntry>>,
}

impl PharData {
    pub fn new(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        Self::open(path.as_ref().to_path_buf(), None)
    }

    pub fn new_with_format(
        path: impl AsRef<std::path::Path>,
        _flags: i64,
        _alias: &str,
        format: i64,
    ) -> anyhow::Result<Self> {
        Self::open(path.as_ref().to_path_buf(), Some(format))
    }

    fn open(path: std::path::PathBuf, format: Option<i64>) -> anyhow::Result<Self> {
        if crate::file_exists(&path) {
            let (entries, detected_format) = read_archive_entries(&path)?;
            return Ok(Self {
                path,
                format: detected_format,
                entries: std::cell::RefCell::new(entries),
            });
        }
        let parent_exists = match path.parent() {
            Some(parent) if parent.as_os_str().is_empty() => true,
            Some(parent) => parent.is_dir(),
            None => false,
        };
        if !parent_exists {
            return Err(UnexpectedValueException::new(format!(
                "Cannot create phar '{}', file extension (or combination) not recognised or the directory does not exist",
                path.display()
            )).into());
        }
        let format = format.unwrap_or(if path.to_string_lossy().ends_with(".zip") {
            Phar::ZIP
        } else {
            Phar::TAR
        });
        Ok(Self {
            path,
            format,
            entries: std::cell::RefCell::new(Vec::new()),
        })
    }

    pub fn can_compress(algo: i64) -> bool {
        matches!(algo, Phar::GZ | Phar::BZ2)
    }

    pub fn valid(&self) -> bool {
        !self.entries.borrow().is_empty()
    }

    pub fn get(&self, key: &str) -> Option<PharFileInfo> {
        let entries = self.entries.borrow();
        let key = key.trim_end_matches('/');
        for entry in entries.iter() {
            if entry.localname == key {
                return Some(PharFileInfo {
                    file_name: entry.localname.clone(),
                    is_dir: matches!(entry.data, PharEntryData::Dir),
                    content: match &entry.data {
                        PharEntryData::Memory(content) => content.clone(),
                        _ => Vec::new(),
                    },
                });
            }
        }
        let prefix = format!("{}/", key);
        if entries.iter().any(|e| e.localname.starts_with(&prefix)) {
            return Some(PharFileInfo {
                file_name: key.to_string(),
                is_dir: true,
                content: Vec::new(),
            });
        }
        None
    }

    /// Iterates the top level of the archive (files, plus explicit and implicit
    /// directories) in sorted order, like a `DirectoryIterator` over the phar root.
    pub fn iter(&self) -> impl Iterator<Item = PharFileInfo> {
        let entries = self.entries.borrow();
        let mut top_level: indexmap::IndexMap<String, (bool, Vec<u8>)> = indexmap::IndexMap::new();
        for entry in entries.iter() {
            match entry.localname.split_once('/') {
                Some((first_segment, _)) => {
                    top_level
                        .entry(first_segment.to_string())
                        .or_insert((true, Vec::new()));
                }
                None => {
                    top_level.insert(
                        entry.localname.clone(),
                        (
                            matches!(entry.data, PharEntryData::Dir),
                            match &entry.data {
                                PharEntryData::Memory(content) => content.clone(),
                                _ => Vec::new(),
                            },
                        ),
                    );
                }
            }
        }
        let mut items: Vec<PharFileInfo> = top_level
            .into_iter()
            .map(|(file_name, (is_dir, content))| PharFileInfo {
                file_name,
                is_dir,
                content,
            })
            .collect();
        items.sort_by(|a, b| a.file_name.cmp(&b.file_name));
        items.into_iter()
    }

    pub fn extract_to(
        &self,
        directory: impl AsRef<std::path::Path>,
        _files: Option<()>,
        overwrite: bool,
    ) -> anyhow::Result<()> {
        extract_entries(
            &self.path,
            &self.entries.borrow(),
            directory.as_ref(),
            overwrite,
        )
    }

    pub fn add_empty_dir(&self, dirname: &str) -> anyhow::Result<()> {
        let localname = dirname.trim_matches('/').to_string();
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
                let localname = file
                    .strip_prefix(base_directory)
                    .map_err(|_| -> anyhow::Error {
                        UnexpectedValueException::new(format!(
                            "Iterator returned a path \"{}\" that is not in the base directory \"{}\"",
                            file.display(),
                            base_directory.display()
                        ))
                        .into()
                    })?
                    .to_string_lossy()
                    .into_owned();
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
            PharException::new(format!(
                "Unable to compress phar archive \"{}\": {}",
                self.path.display(),
                e
            ))
            .into()
        };
        let (target, compressed) = match algo {
            Phar::GZ => {
                let mut encoder =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                encoder.write_all(&tar_bytes).map_err(write_error)?;
                (
                    sibling_with_suffix(&self.path, ".gz"),
                    encoder.finish().map_err(write_error)?,
                )
            }
            Phar::BZ2 => {
                let mut encoder =
                    bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::new(4));
                encoder.write_all(&tar_bytes).map_err(write_error)?;
                (
                    sibling_with_suffix(&self.path, ".bz2"),
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
            PharException::new(format!(
                "Unable to write phar archive \"{}\": {}",
                self.path.display(),
                e
            ))
            .into()
        })
    }

    fn build_tar_bytes(&self) -> anyhow::Result<Vec<u8>> {
        let write_error = |e: std::io::Error| {
            PharException::new(format!(
                "Unable to write phar archive \"{}\": {}",
                self.path.display(),
                e
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
                        .append_data(&mut header, &entry.localname, file)
                        .map_err(write_error)?;
                }
                PharEntryData::Memory(content) => {
                    header.set_entry_type(tar::EntryType::Regular);
                    header.set_size(content.len() as u64);
                    header.set_mode(entry.mode.unwrap_or(0o644));
                    header.set_mtime(
                        entry
                            .mtime
                            .unwrap_or_else(|| unix_mtime(std::time::SystemTime::now())),
                    );
                    builder
                        .append_data(&mut header, &entry.localname, &content[..])
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
                        .append_data(&mut header, &entry.localname, std::io::empty())
                        .map_err(write_error)?;
                }
            }
        }
        builder.into_inner().map_err(write_error)
    }

    fn write_zip(&self) -> anyhow::Result<()> {
        let write_error = |e: String| -> anyhow::Error {
            PharException::new(format!(
                "Unable to write phar archive \"{}\": {}",
                self.path.display(),
                e
            ))
            .into()
        };
        let file = std::fs::File::create(&self.path).map_err(|e| write_error(e.to_string()))?;
        let mut writer = zip::ZipWriter::new(file);
        for entry in self.entries.borrow().iter() {
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            match &entry.data {
                PharEntryData::Disk(source) => {
                    use std::os::unix::fs::PermissionsExt;
                    let metadata =
                        std::fs::metadata(source).map_err(|e| write_error(e.to_string()))?;
                    writer
                        .start_file(
                            entry.localname.as_str(),
                            options.unix_permissions(metadata.permissions().mode() & 0o7777),
                        )
                        .map_err(|e| write_error(e.to_string()))?;
                    let mut file =
                        std::fs::File::open(source).map_err(|e| write_error(e.to_string()))?;
                    std::io::copy(&mut file, &mut writer)
                        .map_err(|e| write_error(e.to_string()))?;
                }
                PharEntryData::Memory(content) => {
                    let options = match entry.mode {
                        Some(mode) => options.unix_permissions(mode),
                        None => options,
                    };
                    writer
                        .start_file(entry.localname.as_str(), options)
                        .map_err(|e| write_error(e.to_string()))?;
                    writer
                        .write_all(content)
                        .map_err(|e| write_error(e.to_string()))?;
                }
                PharEntryData::Dir => {
                    writer
                        .add_directory(entry.localname.as_str(), options)
                        .map_err(|e| write_error(e.to_string()))?;
                }
            }
        }
        writer.finish().map_err(|e| write_error(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Catch as _;

    fn write_file(dir: &std::path::Path, name: &str, content: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn phar_data_tar_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let a = write_file(&src, "a.txt", b"hello");
        let b = write_file(&src, "sub/b.txt", b"world");
        let tar_path = dir.path().join("out.tar");

        let phar = PharData::new(&tar_path).unwrap();
        assert!(!phar.valid());
        phar.build_from_iterator(&mut vec![a, b].into_iter(), &src)
            .unwrap();
        phar.add_empty_dir("emptydir").unwrap();
        assert!(tar_path.exists());

        let read_back = PharData::new(&tar_path).unwrap();
        assert!(read_back.valid());
        assert_eq!(read_back.get("a.txt").unwrap().get_content(), b"hello");
        assert_eq!(read_back.get("sub/b.txt").unwrap().get_content(), b"world");
        assert!(read_back.get("sub").unwrap().is_dir());
        assert!(read_back.get("missing").is_none());
        let top: Vec<(String, bool)> = read_back
            .iter()
            .map(|info| (info.get_basename(), info.is_dir()))
            .collect();
        assert_eq!(
            top,
            vec![
                ("a.txt".to_string(), false),
                ("emptydir".to_string(), true),
                ("sub".to_string(), true),
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
        let a = write_file(&src, "a.txt", b"hello");
        let tar_path = dir.path().join("out.tar");

        let phar = PharData::new(&tar_path).unwrap();
        phar.build_from_iterator(&mut vec![a].into_iter(), &src)
            .unwrap();
        phar.compress(Phar::GZ).unwrap();
        phar.compress(Phar::BZ2).unwrap();

        assert!(tar_path.exists());
        for compressed in ["out.tar.gz", "out.tar.bz2"] {
            let read_back = PharData::new(dir.path().join(compressed)).unwrap();
            assert_eq!(read_back.get("a.txt").unwrap().get_content(), b"hello");
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
                .starts_with("Cannot create phar")
        );
    }

    /// Builds a minimal native phar (one stored file, one deflated file, SHA-1
    /// signature) following the php.net manual layout.
    fn build_native_phar(tampered: bool) -> Vec<u8> {
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

        let mut bytes = b"<?php __HALT_COMPILER(); ?>\r\n".to_vec();
        bytes.extend_from_slice(&(manifest.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&manifest);
        bytes.extend_from_slice(&stored.0);
        bytes.extend_from_slice(&deflated);
        let signature = crate::hash::calculate_hash("sha1", &bytes);
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
        std::fs::write(&phar_path, build_native_phar(false)).unwrap();

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
    fn phar_native_broken_signature_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let phar_path = dir.path().join("tampered.phar");
        std::fs::write(&phar_path, build_native_phar(true)).unwrap();

        let error = Phar::new(&phar_path).unwrap_err();
        assert!(
            error
                .catch::<UnexpectedValueException>()
                .unwrap()
                .get_message()
                .contains("broken signature")
        );
    }
}
