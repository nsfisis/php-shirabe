use crate::ErrorException;
use crate::PhpMixed;
use crate::{StreamBacking, StreamState};
use indexmap::IndexMap;
use zip::write::SimpleFileOptions;

/// Test-only behaviour mirroring PHPUnit's `getMockBuilder('ZipArchive')->getMock()`, where
/// `open`/`extractTo`/`count` are stubbed via `->willReturn(...)`/`->willThrowException(...)`.
/// Held in [`ZipArchive::mock`]; always `None` in production.
#[derive(Debug, Clone)]
pub struct ZipArchiveMock {
    pub open: Result<(), i64>,
    pub count: i64,
    /// `Ok(bool)` for `extractTo` returning a bool, `Err(..)` to throw an `\ErrorException`.
    pub extract_to: Result<bool, String>,
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
        path: String,
        status: String,
    },
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

    pub fn open(&mut self, filename: &str, flags: i64) -> Result<(), i64> {
        if let Some(mock) = &self.mock {
            return mock.open;
        }
        if flags & Self::CREATE != 0 {
            let file = match std::fs::File::create(filename) {
                Ok(f) => f,
                Err(_) => return Err(Self::ER_OPEN),
            };
            *self.state.borrow_mut() = ZipState::Writer {
                writer: zip::ZipWriter::new(file),
                path: filename.to_string(),
                status: String::new(),
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

    pub fn stat_index(&self, index: i64) -> Option<IndexMap<String, PhpMixed>> {
        let mut state = self.state.borrow_mut();
        let ZipState::Reader(archive) = &mut *state else {
            return None;
        };
        let file = archive.by_index(index as usize).ok()?;
        let mut stat = IndexMap::new();
        stat.insert(
            "name".to_string(),
            PhpMixed::String(file.name().to_string()),
        );
        stat.insert("index".to_string(), PhpMixed::Int(index));
        stat.insert("crc".to_string(), PhpMixed::Int(file.crc32() as i64));
        stat.insert("size".to_string(), PhpMixed::Int(file.size() as i64));
        // PHP exposes the last-modified time as a Unix timestamp. The `zip` crate
        // only surfaces a 2-second-precision MS-DOS datetime; no consumer reads
        // this field, so it is reported as 0 rather than reconstructing it.
        stat.insert("mtime".to_string(), PhpMixed::Int(0));
        stat.insert(
            "comp_size".to_string(),
            PhpMixed::Int(file.compressed_size() as i64),
        );
        let comp_method = match file.compression() {
            zip::CompressionMethod::Stored => 0,
            zip::CompressionMethod::Deflated => 8,
            _ => -1,
        };
        stat.insert("comp_method".to_string(), PhpMixed::Int(comp_method));
        Some(stat)
    }

    pub fn extract_to(&self, path: &str) -> Result<bool, ErrorException> {
        if let Some(mock) = &self.mock {
            return mock.extract_to.clone().map_err(|message| ErrorException {
                message,
                code: 0,
                severity: 1,
                filename: String::new(),
                lineno: 0,
            });
        }
        let mut state = self.state.borrow_mut();
        let ZipState::Reader(archive) = &mut *state else {
            return Ok(false);
        };
        Ok(archive.extract(path).is_ok())
    }

    pub fn locate_name(&self, name: &str) -> Option<i64> {
        let state = self.state.borrow();
        let ZipState::Reader(archive) = &*state else {
            return None;
        };
        archive.index_for_name(name).map(|i| i as i64)
    }

    pub fn get_from_index(&self, index: i64) -> Option<String> {
        let mut state = self.state.borrow_mut();
        let ZipState::Reader(archive) = &mut *state else {
            return None;
        };
        let mut file = archive.by_index(index as usize).ok()?;
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut buf).ok()?;
        Some(String::from_utf8_lossy(&buf).into_owned())
    }

    pub fn get_name_index(&self, index: i64) -> String {
        let mut state = self.state.borrow_mut();
        let ZipState::Reader(archive) = &mut *state else {
            return String::new();
        };
        match archive.by_index(index as usize) {
            Ok(file) => file.name().to_string(),
            Err(_) => String::new(),
        }
    }

    pub fn get_from_name(&self, name: &str) -> Option<String> {
        let mut state = self.state.borrow_mut();
        let ZipState::Reader(archive) = &mut *state else {
            return None;
        };
        let mut file = archive.by_name(name).ok()?;
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut buf).ok()?;
        Some(String::from_utf8_lossy(&buf).into_owned())
    }

    pub fn get_stream(&self, name: &str) -> Option<crate::PhpResource> {
        let mut state = self.state.borrow_mut();
        let ZipState::Reader(archive) = &mut *state else {
            return None;
        };
        let mut file = archive.by_name(name).ok()?;
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut buf).ok()?;
        Some(crate::PhpResource::Stream(std::rc::Rc::new(
            std::cell::RefCell::new(StreamState::new(
                StreamBacking::Memory(std::io::Cursor::new(buf)),
                true,
                false,
                "r".to_string(),
                format!("zip://{}", name),
            )),
        )))
    }

    pub fn add_empty_dir(&self, local_name: &str) -> bool {
        let mut state = self.state.borrow_mut();
        let ZipState::Writer { writer, .. } = &mut *state else {
            return false;
        };
        writer
            .add_directory(local_name, SimpleFileOptions::default())
            .is_ok()
    }

    pub fn add_file(&self, filepath: &str, local_name: &str) -> bool {
        let contents = match std::fs::read(filepath) {
            Ok(c) => c,
            Err(_) => return false,
        };
        let mut state = self.state.borrow_mut();
        let ZipState::Writer { writer, .. } = &mut *state else {
            return false;
        };
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        if writer.start_file(local_name, options).is_err() {
            return false;
        }
        std::io::Write::write_all(writer, &contents).is_ok()
    }

    pub fn set_external_attributes_name(&self, _name: &str, _opsys: i64, _attr: i64) -> bool {
        // TODO(phase-d): PHP's setExternalAttributesName mutates an already-added
        // entry's external attributes (e.g. Unix permissions) after addFile. The
        // `zip` crate fixes external attributes at start_file time via FileOptions
        // and exposes no API to amend a written entry, so this cannot be faithfully
        // reproduced without re-architecting add_file. Left unimplemented rather
        // than silently dropping the permission bits.
        todo!()
    }

    pub fn get_status_string(&self) -> String {
        let state = self.state.borrow();
        match &*state {
            ZipState::Writer { status, .. } => status.clone(),
            _ => String::new(),
        }
    }
}

impl ZipArchive {
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
