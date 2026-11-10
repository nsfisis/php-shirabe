//! ref: composer/vendor/symfony/console/Helper/DebugFormatterHelper.php sha1:461237b23d2cff0bf7b0875647a78d54bcb79af5

use crate::helper::{Helper, HelperInterface, HelperSet};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{bformat, bwrite, php_str, str_replace};

const COLORS: [&BStr; 9] = [
    php_str(b"black"),
    php_str(b"red"),
    php_str(b"green"),
    php_str(b"yellow"),
    php_str(b"blue"),
    php_str(b"magenta"),
    php_str(b"cyan"),
    php_str(b"white"),
    php_str(b"default"),
];

/// Helps outputting debug information when running an external program from a command.
///
/// An external program can be a Process, an HTTP request, or anything else.
#[derive(Debug)]
pub struct DebugFormatterHelper {
    inner: Helper,
    started: IndexMap<BString, DebugFormatterSession>,
    count: i64,
}

/// Per-id session state. PHP stores this as `['border' => int, 'out' => true, 'err' => true]`
/// where presence of `out`/`err` keys is tested via `isset` and removed via `unset`.
#[derive(Debug, Default)]
struct DebugFormatterSession {
    border: i64,
    out: bool,
    err: bool,
}

impl Default for DebugFormatterHelper {
    fn default() -> Self {
        Self {
            inner: Helper::default(),
            started: IndexMap::new(),
            count: -1,
        }
    }
}

impl DebugFormatterHelper {
    /// Starts a debug formatting session.
    pub fn start(&mut self, id: &BStr, message: &BStr, prefix: &BStr) -> BString {
        self.count += 1;
        self.started.insert(
            id.to_owned(),
            DebugFormatterSession {
                border: self.count % COLORS.len() as i64,
                out: false,
                err: false,
            },
        );

        bformat!(
            "{}<bg=blue;fg=white> {prefix} </> <fg=blue>{message}</>\n",
            self.get_border(id)
        )
    }

    /// Adds progress to a formatting session.
    pub fn progress(
        &mut self,
        id: &BStr,
        buffer: &BStr,
        error: bool,
        prefix: &BStr,
        error_prefix: &BStr,
    ) -> BString {
        let mut message = BString::default();

        if error {
            if self.started[id].out {
                message.push(b'\n');
                self.started.get_mut(id).unwrap().out = false;
            }
            if !self.started[id].err {
                bwrite!(
                    message,
                    "{}<bg=red;fg=white> {error_prefix} </> ",
                    self.get_border(id)
                );
                self.started.get_mut(id).unwrap().err = true;
            }

            message.extend_from_slice(&str_replace(
                "\n",
                bformat!(
                    "\n{}<bg=red;fg=white> {error_prefix} </> ",
                    self.get_border(id)
                ),
                buffer,
            ));
        } else {
            if self.started[id].err {
                message.push(b'\n');
                self.started.get_mut(id).unwrap().err = false;
            }
            if !self.started[id].out {
                bwrite!(
                    message,
                    "{}<bg=green;fg=white> {prefix} </> ",
                    self.get_border(id)
                );
                self.started.get_mut(id).unwrap().out = true;
            }

            message.extend_from_slice(&str_replace(
                "\n",
                bformat!("\n{}<bg=green;fg=white> {prefix} </> ", self.get_border(id)),
                buffer,
            ));
        }

        message
    }

    /// Stops a formatting session.
    pub fn stop(&mut self, id: &BStr, message: &BStr, successful: bool, prefix: &BStr) -> BString {
        let trailing_eol = if self.started[id].out || self.started[id].err {
            "\n"
        } else {
            ""
        };

        if successful {
            return bformat!(
                "{trailing_eol}{}<bg=green;fg=white> {prefix} </> <fg=green>{message}</>\n",
                self.get_border(id)
            );
        }

        let message = bformat!(
            "{trailing_eol}{}<bg=red;fg=white> {prefix} </> <fg=red>{message}</>\n",
            self.get_border(id)
        );

        if let Some(session) = self.started.get_mut(id) {
            session.out = false;
            session.err = false;
        }

        message
    }

    fn get_border(&self, id: &BStr) -> BString {
        bformat!("<bg={}> </>", COLORS[self.started[id].border as usize])
    }
}

impl HelperInterface for DebugFormatterHelper {
    fn set_helper_set(&mut self, helper_set: Option<std::rc::Rc<std::cell::RefCell<HelperSet>>>) {
        self.inner.set_helper_set(helper_set);
    }

    fn get_helper_set(&self) -> Option<std::rc::Rc<std::cell::RefCell<HelperSet>>> {
        self.inner.get_helper_set()
    }

    fn get_name(&self) -> BString {
        "debug_formatter".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bstr::ByteSlice as _;

    #[test]
    fn id_prefix_and_message_keep_their_bytes() {
        let id = BStr::new(b"id\xE9");
        let mut helper = DebugFormatterHelper::default();
        helper.start(id, "start".into(), "RUN".into());

        let stopped = helper.stop(id, BStr::new(b"stop\xE9"), true, BStr::new(b"RES\xE9"));

        assert!(stopped.contains_str(b"RES\xE9"));
        assert!(stopped.contains_str(b"stop\xE9"));
    }
}
