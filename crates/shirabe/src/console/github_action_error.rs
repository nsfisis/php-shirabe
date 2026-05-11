//! ref: composer/src/Composer/Console/GithubActionError.php

use crate::io::io_interface::IOInterface;
use crate::util::platform::Platform;

#[derive(Debug)]
pub struct GithubActionError {
    pub(crate) io: Box<dyn IOInterface>,
}

impl GithubActionError {
    pub fn new(io: Box<dyn IOInterface>) -> Self {
        Self { io }
    }

    pub fn emit(&mut self, message: &str, file: Option<&str>, line: Option<i64>) {
        if Platform::get_env("GITHUB_ACTIONS").map_or(false, |v| !v.is_empty())
            && !Platform::get_env("COMPOSER_TESTS_ARE_RUNNING").map_or(false, |v| !v.is_empty())
        {
            let message = self.escape_data(message);

            let file_truthy = file.map_or(false, |f| !f.is_empty());
            let line_truthy = line.map_or(false, |l| l != 0);

            if file_truthy && line_truthy {
                let file = self.escape_property(file.unwrap());
                self.io.write(&format!("::error file={},line={}::{}", file, line.unwrap(), message));
            } else if file_truthy {
                let file = self.escape_property(file.unwrap());
                self.io.write(&format!("::error file={}::{}", file, message));
            } else {
                self.io.write(&format!("::error ::{}", message));
            }
        }
    }

    fn escape_data(&self, data: &str) -> String {
        // see https://github.com/actions/toolkit/blob/4f7fb6513a355689f69f0849edeb369a4dc81729/packages/core/src/command.ts#L80-L85
        let data = data.replace('%', "%25");
        let data = data.replace('\r', "%0D");
        let data = data.replace('\n', "%0A");
        data
    }

    fn escape_property(&self, property: &str) -> String {
        // see https://github.com/actions/toolkit/blob/4f7fb6513a355689f69f0849edeb369a4dc81729/packages/core/src/command.ts#L87-L94
        let property = property.replace('%', "%25");
        let property = property.replace('\r', "%0D");
        let property = property.replace('\n', "%0A");
        let property = property.replace(':', "%3A");
        let property = property.replace(',', "%2C");
        property
    }
}
