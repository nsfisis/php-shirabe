//! Configurable Config stub, equivalent to PHPUnit's
//! `getMockBuilder(Config::class)` where `get('x')` is stubbed via
//! `->method('get')->willReturn(y)`.
//!
//! `Config` is a concrete (non-trait) struct, so rather than mocking it we build a
//! real `Config` and seed the requested keys through `Config::merge`, which is how
//! `get('key')` actually resolves its value. This keeps the stub faithful to the
//! real resolution logic instead of intercepting `get`.
#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe_php_shim::PhpMixed;

pub struct ConfigStubBuilder {
    use_environment: bool,
    base_dir: Option<String>,
    // Config values to seed, applied via Config::merge under the `config` section.
    values: IndexMap<String, PhpMixed>,
    source: String,
}

impl Default for ConfigStubBuilder {
    fn default() -> Self {
        Self {
            use_environment: false,
            base_dir: None,
            values: IndexMap::new(),
            source: Config::SOURCE_UNKNOWN.to_string(),
        }
    }
}

impl ConfigStubBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn use_environment(mut self, value: bool) -> Self {
        self.use_environment = value;
        self
    }

    pub fn base_dir(mut self, base_dir: impl Into<String>) -> Self {
        self.base_dir = Some(base_dir.into());
        self
    }

    /// Sets the value `Config::get(key)` will return. Equivalent to PHPUnit's
    /// `->method('get')->with(key)->willReturn(value)`.
    pub fn with(mut self, key: impl Into<String>, value: PhpMixed) -> Self {
        self.values.insert(key.into(), value);
        self
    }

    pub fn build(self) -> Config {
        let mut config = Config::new(self.use_environment, self.base_dir);
        if !self.values.is_empty() {
            let mut merge: IndexMap<String, PhpMixed> = IndexMap::new();
            merge.insert("config".to_string(), PhpMixed::Array(self.values));
            config.merge(&merge, &self.source);
        }
        config
    }

    pub fn build_shared(self) -> Rc<RefCell<Config>> {
        Rc::new(RefCell::new(self.build()))
    }
}

// For testing only. Convenience for the common case of a default `Config(false)`.
pub fn get_config_stub() -> Config {
    ConfigStubBuilder::new().build()
}
