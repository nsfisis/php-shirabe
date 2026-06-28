//! ref: composer/vendor/symfony/process/Pipes/UnixPipes.php

use crate::symfony::process::pipes::abstract_pipes::AbstractPipes;
use crate::symfony::process::pipes::pipes_interface::{CHUNK_SIZE, PipesInterface};
use crate::symfony::process::process::Process;
use indexmap::IndexMap;
use shirabe_php_shim::{Descriptor, PhpMixed, PhpResource};

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

fn descriptor(items: &[&str]) -> Descriptor {
    match items {
        ["pipe", mode] => Descriptor::Pipe(mode.to_string()),
        ["file", path, mode] => Descriptor::File(path.to_string(), mode.to_string()),
        ["pty"] => Descriptor::Pty,
        _ => panic!("unsupported descriptor spec: {:?}", items),
    }
}

impl PipesInterface for UnixPipes {
    fn get_descriptors(&mut self) -> Vec<Descriptor> {
        if !self.have_read_support {
            let nullstream =
                shirabe_php_shim::fopen("/dev/null", "c").expect("fopen('/dev/null') failed");
            return vec![
                descriptor(&["pipe", "r"]),
                Descriptor::Resource(nullstream.clone()),
                Descriptor::Resource(nullstream),
            ];
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

    fn read_and_write(&mut self, blocking: bool, close: bool) -> IndexMap<i64, String> {
        self.inner.unblock();
        let w = self.inner.write();

        let mut read: IndexMap<i64, String> = IndexMap::new();
        // $r = $this->pipes; unset($r[0]);
        let r: Vec<(i64, PhpResource)> = self
            .inner
            .pipes
            .iter()
            .filter(|(fd, _)| **fd != 0)
            .map(|(fd, pipe)| (*fd, pipe.clone()))
            .collect();

        // TODO(plugin): set_error_handler/restore_error_handler around stream_select is not modeled.
        let mut r_sel: Vec<PhpResource> = r.iter().map(|(_, p)| p.clone()).collect();
        let mut w_sel: Vec<PhpResource> = w.clone().unwrap_or_default();
        let mut e_sel: Vec<PhpResource> = Vec::new();

        // let's have a look if something changed in streams
        if (!r_sel.is_empty() || w.is_some())
            && shirabe_php_shim::stream_select(
                &mut r_sel,
                &mut w_sel,
                &mut e_sel,
                0,
                Some(if blocking {
                    (Process::TIMEOUT_PRECISION * 1e6) as i64
                } else {
                    0
                }),
            )
            .is_none()
        {
            // if a system call has been interrupted, forget about it, let's try again
            // otherwise, an error occurred, let's reset pipes
            if !self.inner.has_system_call_been_interrupted() {
                self.inner.pipes = IndexMap::new();
            }

            return read;
        }

        for (fd, pipe) in &r {
            let mut data = String::new();
            loop {
                let chunk = shirabe_php_shim::fread(pipe, CHUNK_SIZE).unwrap_or_default();
                let len = chunk.len() as i64;
                data.push_str(&chunk);
                if !(len > 0 && (close || len >= CHUNK_SIZE)) {
                    break;
                }
            }

            if !data.is_empty() {
                read.insert(*fd, data);
            }

            if close && shirabe_php_shim::feof(pipe) {
                shirabe_php_shim::fclose(pipe);
                self.inner.pipes.shift_remove(fd);
            }
        }

        read
    }

    fn have_read_support(&self) -> bool {
        self.have_read_support
    }

    fn are_open(&self) -> bool {
        !self.inner.pipes.is_empty()
    }

    fn close(&mut self) {
        self.inner.close();
    }

    fn pipes(&self) -> &IndexMap<i64, PhpResource> {
        &self.inner.pipes
    }

    fn pipes_mut(&mut self) -> &mut IndexMap<i64, PhpResource> {
        &mut self.inner.pipes
    }
}
