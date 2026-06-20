//! ref: composer/vendor/symfony/process/Pipes/WindowsPipes.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use crate::symfony::process::pipes::abstract_pipes::AbstractPipes;
use crate::symfony::process::pipes::pipes_interface::PipesInterface;

/// WindowsPipes implementation uses temporary files as handles.
#[derive(Debug)]
pub struct WindowsPipes {
    inner: AbstractPipes,
    files: IndexMap<i64, String>,
    file_handles: IndexMap<i64, PhpMixed>,
    lock_handles: IndexMap<i64, PhpMixed>,
    read_bytes: IndexMap<i64, i64>,
    have_read_support: bool,
}

impl WindowsPipes {
    pub fn new(_input: PhpMixed, _have_read_support: bool) -> Self {
        // Windows-only path: never constructed on POSIX (DIRECTORY_SEPARATOR is "/").
        todo!()
    }
}

impl PipesInterface for WindowsPipes {
    fn get_descriptors(&mut self) -> Vec<PhpMixed> {
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

    fn have_read_support(&self) -> bool {
        self.have_read_support
    }

    fn are_open(&self) -> bool {
        shirabe_php_shim::php_truthy(&self.inner.pipes) && !self.file_handles.is_empty()
    }

    fn close(&mut self) {
        self.inner.close();
        todo!()
    }

    fn pipes(&self) -> &PhpMixed {
        &self.inner.pipes
    }

    fn pipes_mut(&mut self) -> &mut PhpMixed {
        &mut self.inner.pipes
    }
}
