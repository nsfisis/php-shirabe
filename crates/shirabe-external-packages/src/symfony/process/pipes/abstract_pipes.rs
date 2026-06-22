//! ref: composer/vendor/symfony/process/Pipes/AbstractPipes.php

use indexmap::IndexMap;
use shirabe_php_shim::{self as php, PhpMixed};

#[derive(Debug)]
pub struct AbstractPipes {
    pub pipes: PhpMixed,

    input_buffer: String,
    input: PhpMixed,
    blocked: bool,
    last_error: Option<String>,
}

impl AbstractPipes {
    pub fn new(input: PhpMixed) -> Self {
        let mut input_buffer = String::new();
        let stored_input;
        if php::is_resource(&input) {
            stored_input = input;
        } else if let PhpMixed::String(s) = &input {
            input_buffer = s.clone();
            stored_input = PhpMixed::Null;
        } else {
            input_buffer = input.as_string().map(|s| s.to_string()).unwrap_or_default();
            stored_input = PhpMixed::Null;
        }

        Self {
            pipes: PhpMixed::List(Vec::new()),
            input_buffer,
            input: stored_input,
            blocked: true,
            last_error: None,
        }
    }

    pub fn close(&mut self) {
        // TODO(phase-d): each pipe is a PHP stream resource that should be fclose()d, but the pipe
        // list is a PhpMixed that cannot hold a PhpResource; the handles are dropped instead.
        self.pipes = PhpMixed::List(Vec::new());
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
        let _ = &self.input_buffer;
        let _ = &self.blocked;
        todo!()
    }

    /// Writes input to stdin.
    pub(crate) fn write(&mut self) -> Option<IndexMap<i64, PhpMixed>> {
        todo!()
    }

    pub fn handle_error(&mut self, _type: i64, msg: String) {
        self.last_error = Some(msg);
    }
}
