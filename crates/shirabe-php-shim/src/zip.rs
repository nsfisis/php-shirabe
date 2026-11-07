// TODO(php-semantics): this differs from PHP's `ZipArchive` in details. Its compatibility needs to
// be investigated against the C sources of PHP's `ext/zip`.

use crate::{ErrorException, PhpResource, StreamBacking, StreamState, bconcat};
use bstr::{BStr, BString};
use zip::read::{ExtractOptions, SymlinkExtractAction};
use zip::result::ZipResult;
use zip::write::SimpleFileOptions;

/// Starts a directory entry whose name may hold any bytes, which `ZipWriter::add_directory` does
/// not accept. A name without a trailing separator gets a `/`, as `add_directory` does.
pub(crate) fn start_directory<W: std::io::Write + std::io::Seek>(
    writer: &mut zip::ZipWriter<W>,
    name: &BStr,
    options: SimpleFileOptions,
) -> ZipResult<()> {
    let mut name = BString::from(name);
    if !matches!(name.last(), Some(b'/' | b'\\')) {
        name.push(b'/');
    }
    writer.start_file_with_options(
        options
            .name(&name)
            .compression_method(zip::CompressionMethod::Stored),
    )
}

/// Test-only behaviour mirroring PHPUnit's `getMockBuilder('ZipArchive')->getMock()`, where
/// `open`/`extractTo`/`count` are stubbed via `->willReturn(...)`/`->willThrowException(...)`.
/// Held in [`ZipArchive::mock`]; always `None` in production.
#[derive(Debug, Clone)]
pub struct ZipArchiveMock {
    pub open: Result<(), i64>,
    pub count: i64,
    /// `Ok(bool)` for `extractTo` returning a bool, `Err(..)` to throw an `\ErrorException`.
    pub extract_to: Result<bool, BString>,
}

/// Internal backing of an opened `ZipArchive`. PHP's `ZipArchive` multiplexes
/// reading an existing archive and building a new one through the same handle;
/// the `zip` crate splits these into `ZipArchive` (read) and `ZipWriter` (write),
/// so the open mode determines which variant is live.
#[derive(Debug, Default)]
enum ZipState {
    #[default]
    Closed,
    Reader(zip::ZipArchive<std::fs::File>),
    Writer {
        writer: zip::ZipWriter<std::fs::File>,
        /// The destination path, retained so `close` can confirm the file exists.
        path: std::path::PathBuf,
        status: BString,
    },
}

/// One entry of an archive, as returned by [`ZipArchive::stat_index`]. Only fields that Composer
/// accesses are ported.
#[derive(Debug, Clone)]
pub struct ZipEntryStat {
    pub size: i64,
    pub comp_size: i64,
}

#[derive(Debug)]
pub struct ZipArchive {
    pub num_files: i64,
    state: std::cell::RefCell<ZipState>,
    /// Test-only mock state. `None` in production; set via [`ZipArchive::__mock`] in tests.
    mock: Option<ZipArchiveMock>,
}

impl Default for ZipArchive {
    fn default() -> Self {
        Self::new()
    }
}

impl ZipArchive {
    pub fn new() -> Self {
        Self {
            num_files: 0,
            state: std::cell::RefCell::new(ZipState::Closed),
            mock: None,
        }
    }

    /// For testing only. Builds a mocked ZipArchive whose `open`/`count`/`extract_to` return the
    /// configured values, mirroring PHPUnit's `getMockBuilder('ZipArchive')->getMock()`.
    pub fn __mock(mock: ZipArchiveMock) -> Self {
        Self {
            num_files: mock.count,
            state: std::cell::RefCell::new(ZipState::Closed),
            mock: Some(mock),
        }
    }

    pub fn open(&mut self, filename: impl AsRef<std::path::Path>, flags: i64) -> Result<(), i64> {
        if let Some(mock) = &self.mock {
            return mock.open;
        }
        let filename = filename.as_ref();
        if flags & Self::CREATE != 0 {
            let file = match std::fs::File::create(filename) {
                Ok(f) => f,
                Err(_) => return Err(Self::ER_OPEN),
            };
            *self.state.borrow_mut() = ZipState::Writer {
                writer: zip::ZipWriter::new(file),
                path: filename.to_path_buf(),
                status: BString::default(),
            };
            self.num_files = 0;
            Ok(())
        } else {
            let file = match std::fs::File::open(filename) {
                Ok(f) => f,
                Err(_) => return Err(Self::ER_NOENT),
            };
            let archive = match zip::ZipArchive::new(file) {
                Ok(a) => a,
                Err(_) => return Err(Self::ER_NOZIP),
            };
            self.num_files = archive.len() as i64;
            *self.state.borrow_mut() = ZipState::Reader(archive);
            Ok(())
        }
    }

    pub fn close(&self) -> bool {
        let state = std::mem::take(&mut *self.state.borrow_mut());
        match state {
            ZipState::Closed => false,
            ZipState::Reader(_) => true,
            ZipState::Writer { writer, .. } => writer.finish().is_ok(),
        }
    }

    pub fn count(&self) -> i64 {
        if let Some(mock) = &self.mock {
            return mock.count;
        }
        self.num_files
    }

    pub fn stat_index(&self, index: i64) -> Option<ZipEntryStat> {
        let state = self.state.borrow();
        let ZipState::Reader(archive) = &*state else {
            return None;
        };
        let file = archive.by_index_data(index as usize).ok()?;
        Some(ZipEntryStat {
            size: file.size() as i64,
            comp_size: file.compressed_size() as i64,
        })
    }

    pub fn extract_to(&self, path: impl AsRef<std::path::Path>) -> Result<bool, ErrorException> {
        if let Some(mock) = &self.mock {
            return mock.extract_to.clone().map_err(|message| {
                ErrorException::new(message, 0, 1, BString::default(), 0, None)
            });
        }
        let mut state = self.state.borrow_mut();
        let ZipState::Reader(archive) = &mut *state else {
            return Ok(false);
        };
        let options = ExtractOptions::default().symlink_action(SymlinkExtractAction::ExtractAsFile);
        Ok(archive.extract_with_options(path.as_ref(), options).is_ok())
    }

    /// The index of the entry whose name is `name`, compared as the bytes the archive stores.
    pub fn locate_name(&self, name: impl AsRef<[u8]>) -> Option<i64> {
        let state = self.state.borrow();
        let ZipState::Reader(archive) = &*state else {
            return None;
        };
        archive.index_for_name_raw(name.as_ref()).map(|i| i as i64)
    }

    pub fn get_from_index(&self, index: i64) -> Option<Vec<u8>> {
        let mut state = self.state.borrow_mut();
        let ZipState::Reader(archive) = &mut *state else {
            return None;
        };
        let mut file = archive.by_index(index as usize).ok()?;
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut buf).ok()?;
        Some(buf)
    }

    pub fn get_name_index(&self, index: i64) -> BString {
        let state = self.state.borrow();
        let ZipState::Reader(archive) = &*state else {
            return BString::default();
        };
        match archive.by_index_data(index as usize) {
            Ok(file) => BString::from(file.name_raw()),
            Err(_) => BString::default(),
        }
    }

    pub fn get_from_name(&self, name: impl AsRef<[u8]>) -> Option<Vec<u8>> {
        let index = self.locate_name(name)?;
        self.get_from_index(index)
    }

    pub fn get_stream(&self, name: impl AsRef<[u8]>) -> Option<PhpResource> {
        let name = name.as_ref();
        let index = self.locate_name(name)?;
        let mut state = self.state.borrow_mut();
        let ZipState::Reader(archive) = &mut *state else {
            return None;
        };
        let mut file = archive.by_index(index as usize).ok()?;
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut buf).ok()?;
        Some(PhpResource::Stream(std::rc::Rc::new(
            std::cell::RefCell::new(StreamState::new(
                StreamBacking::Memory(std::io::Cursor::new(buf)),
                true,
                false,
                BString::from("r"),
                bconcat!("zip://", name),
            )),
        )))
    }

    pub fn add_empty_dir(&self, local_name: impl AsRef<[u8]>, opsys: i64, attr: u32) -> bool {
        let mut state = self.state.borrow_mut();
        let ZipState::Writer { writer, .. } = &mut *state else {
            return false;
        };
        start_directory(
            writer,
            BStr::new(local_name.as_ref()),
            Self::entry_options(opsys, attr),
        )
        .is_ok()
    }

    pub fn add_file(
        &self,
        filepath: impl AsRef<std::path::Path>,
        local_name: impl AsRef<[u8]>,
        opsys: i64,
        attr: u32,
    ) -> bool {
        let contents = match std::fs::read(filepath.as_ref()) {
            Ok(c) => c,
            Err(_) => return false,
        };
        let mut state = self.state.borrow_mut();
        let ZipState::Writer { writer, .. } = &mut *state else {
            return false;
        };
        let options = Self::entry_options(opsys, attr)
            .name(local_name.as_ref())
            .compression_method(zip::CompressionMethod::Deflated);
        if writer.start_file_with_options(options).is_err() {
            return false;
        }
        std::io::Write::write_all(writer, &contents).is_ok()
    }

    /// `opsys` and `attr` carry what PHP passes to `setExternalAttributesName`, which
    /// amends an entry after `addFile`. The `zip` crate fixes external attributes when
    /// the entry is started, so they are taken up front instead.
    fn entry_options(opsys: i64, attr: u32) -> SimpleFileOptions {
        let system = match opsys {
            Self::OPSYS_UNIX => zip::System::Unix,
            _ => todo!(),
        };
        SimpleFileOptions::default()
            .system(system)
            .external_attributes(attr)
    }

    pub fn get_status_string(&self) -> BString {
        let state = self.state.borrow();
        match &*state {
            ZipState::Writer { status, .. } => status.clone(),
            _ => BString::default(),
        }
    }

    pub const CREATE: i64 = 1;
    pub const OPSYS_UNIX: i64 = 3;
    pub const ER_SEEK: i64 = 4;
    pub const ER_READ: i64 = 5;
    pub const ER_NOENT: i64 = 9;
    pub const ER_EXISTS: i64 = 10;
    pub const ER_OPEN: i64 = 11;
    pub const ER_MEMORY: i64 = 14;
    pub const ER_INVAL: i64 = 18;
    pub const ER_NOZIP: i64 = 19;
    pub const ER_INCONS: i64 = 21;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[test]
    fn entries_keep_names_that_are_not_valid_utf8_and_verbatim_attributes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::write(&source, b"hello").unwrap();
        let target = dir.path().join("archive.zip");

        let mut zip = ZipArchive::new();
        zip.open(&target, ZipArchive::CREATE).unwrap();
        assert!(zip.add_file(
            &source,
            b"caf\xE9.txt",
            ZipArchive::OPSYS_UNIX,
            0o104755 << 16
        ));
        assert!(zip.add_empty_dir(b"d\xE9", ZipArchive::OPSYS_UNIX, 0o040711 << 16));
        assert!(zip.close());

        let mut zip = ZipArchive::new();
        zip.open(&target, 0).unwrap();
        assert_eq!(zip.count(), 2);
        assert_eq!(zip.locate_name(b"caf\xE9.txt"), Some(0));
        assert_eq!(zip.locate_name(b"d\xE9/"), Some(1));
        assert_eq!(zip.locate_name(b"absent"), None);
        assert_eq!(zip.get_name_index(1), BString::from(&b"d\xE9/"[..]));
        assert_eq!(zip.get_from_name(b"caf\xE9.txt").unwrap(), b"hello");

        let archive = zip::ZipArchive::new(std::fs::File::open(&target).unwrap()).unwrap();
        let file = archive.by_index_data(0).unwrap();
        assert_eq!(file.external_attributes(), 0o104755 << 16);
        let directory = archive.by_index_data(1).unwrap();
        assert!(directory.is_dir());
        assert_eq!(directory.size(), 0);
        assert_eq!(directory.compressed_size(), 0);
        assert_eq!(directory.external_attributes(), 0o040711 << 16);
    }

    #[test]
    fn stat_index_reports_an_encrypted_entry() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("archive.zip");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&target).unwrap());
        writer
            .start_file(
                "secret.txt",
                SimpleFileOptions::default().with_aes_encryption(zip::AesMode::Aes256, "password"),
            )
            .unwrap();
        writer.write_all(b"secret").unwrap();
        writer.finish().unwrap();

        let mut zip = ZipArchive::new();
        zip.open(&target, 0).unwrap();
        assert_eq!(zip.stat_index(0).unwrap().size, 6);
        assert_eq!(zip.get_name_index(0), BString::from("secret.txt"));
    }

    #[test]
    fn extract_to_writes_a_symlink_entry_as_a_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("archive.zip");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&target).unwrap());
        writer
            .add_symlink("link", "../outside", SimpleFileOptions::default())
            .unwrap();
        writer
            .start_file("after.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"after").unwrap();
        writer.finish().unwrap();

        let out = dir.path().join("out");
        let mut zip = ZipArchive::new();
        zip.open(&target, 0).unwrap();
        assert!(zip.extract_to(&out).unwrap());

        let link = std::fs::symlink_metadata(out.join("link")).unwrap();
        assert!(link.file_type().is_file());
        assert_eq!(std::fs::read(out.join("link")).unwrap(), b"../outside");
        assert_eq!(std::fs::read(out.join("after.txt")).unwrap(), b"after");
    }
}
