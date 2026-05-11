//! ref: composer/src/Composer/Plugin/Capable.php

use indexmap::IndexMap;

// TODO(plugin): Plugin API - interface for plugins that expose capability implementations
pub trait Capable {
    fn get_capabilities(&self) -> IndexMap<String, String>;
}
