//! ref: composer/src/Composer/Plugin/Capable.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

// The sole implementor is the PHP plugin proxy (Composer itself never implements Capable), so
// the trait is fallible: the answer crosses the RPC boundary. Values are PhpMixed, not String:
// PHP's getCapabilities() may return anything, and PluginManager validates only the queried
// key, so narrowing the type here would reject maps upstream Composer accepts.
pub trait Capable {
    fn get_capabilities(&self) -> anyhow::Result<IndexMap<String, PhpMixed>>;
}
