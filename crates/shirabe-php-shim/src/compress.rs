use crate::PhpMixed;

pub fn gzopen(_file: &str, _mode: &str) -> PhpMixed {
    todo!()
}

pub fn gzread(_file: PhpMixed, _length: i64) -> String {
    todo!()
}

pub fn gzclose(_file: PhpMixed) {
    todo!()
}

pub fn gzcompress(_data: &[u8]) -> Option<Vec<u8>> {
    todo!()
}

pub fn bzcompress(_data: &[u8]) -> Option<Vec<u8>> {
    todo!()
}

pub fn zlib_decode(_data: &str) -> Option<String> {
    todo!()
}
