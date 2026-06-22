//! ref: composer/vendor/symfony/process/Pipes/UnixPipes.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use crate::symfony::process::pipes::abstract_pipes::AbstractPipes;
use crate::symfony::process::pipes::pipes_interface::PipesInterface;
use crate::symfony::process::process::Process;

/// UnixPipes implementation uses unix pipes as handles.
#[derive(Debug)]
pub struct UnixPipes {
    inner: AbstractPipes,
    tty_mode: Option<bool>,
    pty_mode: bool,
    have_read_support: bool,
}

impl UnixPipes {
    pub fn new(
        tty_mode: Option<bool>,
        pty_mode: bool,
        input: PhpMixed,
        have_read_support: bool,
    ) -> Self {
        Self {
            inner: AbstractPipes::new(input),
            tty_mode,
            pty_mode,
            have_read_support,
        }
    }
}

fn descriptor(items: &[&str]) -> PhpMixed {
    PhpMixed::List(
        items
            .iter()
            .map(|s| PhpMixed::String(s.to_string()))
            .collect(),
    )
}

impl PipesInterface for UnixPipes {
    fn get_descriptors(&mut self) -> Vec<PhpMixed> {
        if !self.have_read_support {
            // TODO(phase-d): /dev/null is opened as a stream resource and placed directly into the
            // proc_open descriptor spec, but the descriptor list is a Vec<PhpMixed> that cannot
            // carry a PhpResource.
            todo!(
                "UnixPipes::get_descriptors: the /dev/null resource cannot be represented in a PhpMixed descriptor list"
            );
        }

        if self.tty_mode == Some(true) {
            return vec![
                descriptor(&["file", "/dev/tty", "r"]),
                descriptor(&["file", "/dev/tty", "w"]),
                descriptor(&["file", "/dev/tty", "w"]),
            ];
        }

        if self.pty_mode && Process::is_pty_supported() {
            return vec![
                descriptor(&["pty"]),
                descriptor(&["pty"]),
                descriptor(&["pty"]),
            ];
        }

        vec![
            descriptor(&["pipe", "r"]),
            descriptor(&["pipe", "w"]),
            descriptor(&["pipe", "w"]),
        ]
    }

    fn get_files(&self) -> IndexMap<i64, String> {
        IndexMap::new()
    }

    fn read_and_write(&mut self, _blocking: bool, _close: bool) -> IndexMap<i64, String> {
        todo!()
    }

    fn have_read_support(&self) -> bool {
        self.have_read_support
    }

    fn are_open(&self) -> bool {
        shirabe_php_shim::php_truthy(&self.inner.pipes)
    }

    fn close(&mut self) {
        self.inner.close();
    }

    fn pipes(&self) -> &PhpMixed {
        &self.inner.pipes
    }

    fn pipes_mut(&mut self) -> &mut PhpMixed {
        &mut self.inner.pipes
    }
}
