//! ref: composer/vendor/symfony/process/Pipes/AbstractPipes.php

use indexmap::IndexMap;
use shirabe_php_shim::{PhpMixed, PhpResource};

#[derive(Debug)]
pub struct AbstractPipes {
    pub pipes: IndexMap<i64, PhpResource>,

    input_buffer: String,
    input: PhpMixed,
    blocked: bool,
    last_error: Option<String>,
}

impl AbstractPipes {
    pub fn new(input: PhpMixed) -> Self {
        let input_buffer;
        let stored_input;
        // TODO(plugin): `$input instanceof \Iterator` is not modeled. The PHP `is_resource($input)`
        // branch never applies: a PhpMixed is never a resource, so input is never stored as-is here.
        if let PhpMixed::String(s) = &input {
            input_buffer = s.clone();
            stored_input = PhpMixed::Null;
        } else {
            input_buffer = input.as_string().map(|s| s.to_string()).unwrap_or_default();
            stored_input = PhpMixed::Null;
        }

        Self {
            pipes: IndexMap::new(),
            input_buffer,
            input: stored_input,
            blocked: true,
            last_error: None,
        }
    }

    pub fn close(&mut self) {
        for (_, pipe) in &self.pipes {
            shirabe_php_shim::fclose(pipe);
        }
        self.pipes = IndexMap::new();
    }

    /// Returns true if a system call has been interrupted.
    pub(crate) fn has_system_call_been_interrupted(&mut self) -> bool {
        let last_error = self.last_error.take();

        // stream_select returns false when the `select` system call is interrupted by an incoming signal
        last_error
            .map(|e| e.to_lowercase().contains("interrupted system call"))
            .unwrap_or(false)
    }

    /// Unblocks streams.
    pub(crate) fn unblock(&mut self) {
        if !self.blocked {
            return;
        }

        for (_, pipe) in &self.pipes {
            shirabe_php_shim::stream_set_blocking(pipe, false);
        }
        // The `is_resource($this->input)` branch does not apply: `input` is never a resource in this
        // port (is_resource on a PhpMixed is always false).

        self.blocked = false;
    }

    /// Writes input to stdin.
    pub(crate) fn write(&mut self) -> Option<Vec<PhpResource>> {
        let stdin = self.pipes.get(&0)?.clone();

        // TODO(plugin): the `$input instanceof \Iterator` branch is not modeled. `input` is never a
        // resource here, so the fread($input)/stream_set_blocking($input) paths do not apply and
        // only the input buffer is written to stdin.

        let mut r: Vec<PhpResource> = Vec::new();
        let mut e: Vec<PhpResource> = Vec::new();
        let mut w: Vec<PhpResource> = vec![stdin.clone()];

        // let's have a look if something changed in streams
        shirabe_php_shim::stream_select(&mut r, &mut w, &mut e, 0, Some(0))?;

        if !self.input_buffer.is_empty() {
            let written =
                shirabe_php_shim::fwrite(&stdin, &self.input_buffer, None).unwrap_or(0) as usize;
            self.input_buffer = self.input_buffer.get(written..).unwrap_or("").to_string();
            if !self.input_buffer.is_empty() {
                return Some(vec![stdin]);
            }
        }

        // no input to read on resource, buffer is empty
        if self.input_buffer.is_empty() && !shirabe_php_shim::php_truthy(&self.input) {
            self.input = PhpMixed::Null;
            shirabe_php_shim::fclose(&stdin);
            self.pipes.shift_remove(&0);
        }

        None
    }
}
