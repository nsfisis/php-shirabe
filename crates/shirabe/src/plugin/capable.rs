//! ref: composer/src/Composer/Plugin/Capable.php

use indexmap::IndexMap;

// The sole implementor is the PHP plugin proxy (Composer itself never implements Capable), so
// the trait is fallible: the answer crosses the RPC boundary.
pub trait Capable {
    fn get_capabilities(&self) -> anyhow::Result<IndexMap<String, String>>;
}
