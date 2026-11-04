//! ref: composer/src/Composer/Console/GithubActionError.php sha1:edfdd2e8cfd2a12c7c484a789141f3a3f0f50b75

use crate::io::{IOInterface, IOInterfaceWrite};
use crate::util::Platform;
use bstr::{BStr, BString};
use shirabe_php_shim::{bconcat, str_replace};

#[derive(Debug)]
pub struct GithubActionError {
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
}

impl GithubActionError {
    pub fn new(io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>) -> Self {
        Self { io }
    }

    pub fn emit(&mut self, message: &BStr, file: Option<&BStr>, line: Option<i64>) {
        if Platform::get_env("GITHUB_ACTIONS").is_some_and(|v| !v.is_empty())
            && Platform::get_env("COMPOSER_TESTS_ARE_RUNNING").is_none_or(|v| v.is_empty())
        {
            let message = self.escape_data(message);

            let file_truthy = file.is_some_and(|f| !f.is_empty());
            let line_truthy = line.is_some_and(|l| l != 0);

            if file_truthy && line_truthy {
                let file = self.escape_property(file.unwrap());
                self.io.write(bconcat!(
                    "::error file=",
                    file,
                    ",line=",
                    line.unwrap(),
                    "::",
                    message
                ));
            } else if file_truthy {
                let file = self.escape_property(file.unwrap());
                self.io
                    .write(bconcat!("::error file=", file, "::", message));
            } else {
                self.io.write(bconcat!("::error ::", message));
            }
        }
    }

    fn escape_data(&self, data: &BStr) -> BString {
        // see https://github.com/actions/toolkit/blob/4f7fb6513a355689f69f0849edeb369a4dc81729/packages/core/src/command.ts#L80-L85
        let data = str_replace("%", "%25", data);
        let data = str_replace("\r", "%0D", &data);

        str_replace("\n", "%0A", &data)
    }

    fn escape_property(&self, property: &BStr) -> BString {
        // see https://github.com/actions/toolkit/blob/4f7fb6513a355689f69f0849edeb369a4dc81729/packages/core/src/command.ts#L87-L94
        let property = str_replace("%", "%25", property);
        let property = str_replace("\r", "%0D", &property);
        let property = str_replace("\n", "%0A", &property);
        let property = str_replace(":", "%3A", &property);

        str_replace(",", "%2C", &property)
    }
}
