use crate::symfony::console::helper::helper::Helper;
use crate::symfony::console::helper::helper_interface::HelperInterface;
use crate::symfony::console::helper::helper_set::HelperSet;
use indexmap::IndexMap;
use std::cell::RefCell;
use std::rc::Rc;

const COLORS: [&str; 9] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white", "default",
];

/// Helps outputting debug information when running an external program from a command.
///
/// An external program can be a Process, an HTTP request, or anything else.
#[derive(Debug)]
pub struct DebugFormatterHelper {
    inner: Helper,
    started: IndexMap<String, DebugFormatterSession>,
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
    pub fn start(&mut self, id: &str, message: &str, prefix: &str) -> String {
        self.count += 1;
        self.started.insert(
            id.to_string(),
            DebugFormatterSession {
                border: self.count % COLORS.len() as i64,
                out: false,
                err: false,
            },
        );

        shirabe_php_shim::sprintf(
            "%s<bg=blue;fg=white> %s </> <fg=blue>%s</>\n",
            &[
                shirabe_php_shim::PhpMixed::String(self.get_border(id)),
                shirabe_php_shim::PhpMixed::String(prefix.to_string()),
                shirabe_php_shim::PhpMixed::String(message.to_string()),
            ],
        )
    }

    /// Adds progress to a formatting session.
    pub fn progress(
        &mut self,
        id: &str,
        buffer: &str,
        error: bool,
        prefix: &str,
        error_prefix: &str,
    ) -> String {
        let mut message = String::new();

        if error {
            if self.started[id].out {
                message.push('\n');
                self.started.get_mut(id).unwrap().out = false;
            }
            if !self.started[id].err {
                message.push_str(&shirabe_php_shim::sprintf(
                    "%s<bg=red;fg=white> %s </> ",
                    &[
                        shirabe_php_shim::PhpMixed::String(self.get_border(id)),
                        shirabe_php_shim::PhpMixed::String(error_prefix.to_string()),
                    ],
                ));
                self.started.get_mut(id).unwrap().err = true;
            }

            message.push_str(&shirabe_php_shim::str_replace(
                "\n",
                &shirabe_php_shim::sprintf(
                    "\n%s<bg=red;fg=white> %s </> ",
                    &[
                        shirabe_php_shim::PhpMixed::String(self.get_border(id)),
                        shirabe_php_shim::PhpMixed::String(error_prefix.to_string()),
                    ],
                ),
                buffer,
            ));
        } else {
            if self.started[id].err {
                message.push('\n');
                self.started.get_mut(id).unwrap().err = false;
            }
            if !self.started[id].out {
                message.push_str(&shirabe_php_shim::sprintf(
                    "%s<bg=green;fg=white> %s </> ",
                    &[
                        shirabe_php_shim::PhpMixed::String(self.get_border(id)),
                        shirabe_php_shim::PhpMixed::String(prefix.to_string()),
                    ],
                ));
                self.started.get_mut(id).unwrap().out = true;
            }

            message.push_str(&shirabe_php_shim::str_replace(
                "\n",
                &shirabe_php_shim::sprintf(
                    "\n%s<bg=green;fg=white> %s </> ",
                    &[
                        shirabe_php_shim::PhpMixed::String(self.get_border(id)),
                        shirabe_php_shim::PhpMixed::String(prefix.to_string()),
                    ],
                ),
                buffer,
            ));
        }

        message
    }

    /// Stops a formatting session.
    pub fn stop(&mut self, id: &str, message: &str, successful: bool, prefix: &str) -> String {
        let trailing_eol = if self.started[id].out || self.started[id].err {
            "\n"
        } else {
            ""
        };

        if successful {
            return shirabe_php_shim::sprintf(
                "%s%s<bg=green;fg=white> %s </> <fg=green>%s</>\n",
                &[
                    shirabe_php_shim::PhpMixed::String(trailing_eol.to_string()),
                    shirabe_php_shim::PhpMixed::String(self.get_border(id)),
                    shirabe_php_shim::PhpMixed::String(prefix.to_string()),
                    shirabe_php_shim::PhpMixed::String(message.to_string()),
                ],
            );
        }

        let message = shirabe_php_shim::sprintf(
            "%s%s<bg=red;fg=white> %s </> <fg=red>%s</>\n",
            &[
                shirabe_php_shim::PhpMixed::String(trailing_eol.to_string()),
                shirabe_php_shim::PhpMixed::String(self.get_border(id)),
                shirabe_php_shim::PhpMixed::String(prefix.to_string()),
                shirabe_php_shim::PhpMixed::String(message.to_string()),
            ],
        );

        if let Some(session) = self.started.get_mut(id) {
            session.out = false;
            session.err = false;
        }

        message
    }

    fn get_border(&self, id: &str) -> String {
        shirabe_php_shim::sprintf(
            "<bg=%s> </>",
            &[shirabe_php_shim::PhpMixed::String(
                COLORS[self.started[id].border as usize].to_string(),
            )],
        )
    }
}

impl HelperInterface for DebugFormatterHelper {
    fn set_helper_set(&mut self, helper_set: Option<Rc<RefCell<HelperSet>>>) {
        self.inner.set_helper_set(helper_set);
    }

    fn get_helper_set(&self) -> Option<Rc<RefCell<HelperSet>>> {
        self.inner.get_helper_set()
    }

    fn get_name(&self) -> String {
        "debug_formatter".to_string()
    }
}
