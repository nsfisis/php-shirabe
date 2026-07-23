//! ref: composer/vendor/symfony/process/Pipes/WindowsPipes.php

use crate::symfony::process::pipes::abstract_pipes::AbstractPipes;
use crate::symfony::process::pipes::pipes_interface::PipesInterface;
use indexmap::IndexMap;
use shirabe_php_shim::{Descriptor, PhpMixed, PhpResource};

/// WindowsPipes implementation uses temporary files as handles.
#[derive(Debug)]
pub struct WindowsPipes {
    inner: AbstractPipes,
    files: IndexMap<i64, String>,
    file_handles: IndexMap<i64, PhpResource>,
    lock_handles: IndexMap<i64, PhpResource>,
    read_bytes: IndexMap<i64, i64>,
}

impl WindowsPipes {
    pub fn new(_input: PhpMixed) -> Self {
        // Windows-only path: never constructed on POSIX (DIRECTORY_SEPARATOR is "/").
        todo!()
    }
}

impl PipesInterface for WindowsPipes {
    fn get_descriptors(&mut self) -> Vec<Descriptor> {
        let _ = (
            &self.files,
            &self.file_handles,
            &self.lock_handles,
            &self.read_bytes,
        );
        todo!()
    }

    fn get_files(&self) -> IndexMap<i64, String> {
        self.files.clone()
    }

    fn read_and_write(&mut self, _blocking: bool, _close: bool) -> IndexMap<i64, String> {
        todo!()
    }

    fn are_open(&self) -> bool {
        !self.inner.pipes.is_empty() && !self.file_handles.is_empty()
    }

    fn close(&mut self) {
        self.inner.close();
        todo!()
    }

    fn pipes(&self) -> &IndexMap<i64, PhpResource> {
        &self.inner.pipes
    }

    fn pipes_mut(&mut self) -> &mut IndexMap<i64, PhpResource> {
        &mut self.inner.pipes
    }
}
