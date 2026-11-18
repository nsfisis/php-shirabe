//! The methods a `Composer\Config` entity answers over RPC.

use crate::config::{__php_binding_config, Config};
use shirabe_php_binding_macros::php_entity;

// TODO(plugin): the config-source surface (getConfigSource / setAuthConfigSource / ...) needs
// proxy stubs for ConfigSourceInterface implementations, and prohibitUrlByConfig needs the IO
// argument decoded back into the Rust-side instance.
php_entity! {
    handle: std::rc::Rc<std::cell::RefCell<Config>>,
    expected: "a Config",
    class: r"Composer\Config",
    unimplemented: [
        "__construct",
        "getAuthConfigSource",
        "getConfigSource",
        "getLocalAuthConfigSource",
        "prohibitUrlByConfig",
        "setAuthConfigSource",
        "setBaseDir",
        "setConfigSource",
        "setLocalAuthConfigSource",
    ],
    types: {
        r"Composer\Config": instance __php_binding_config for Config {
            via: |config| config.borrow(),
            mut: |config| config.borrow_mut(),
        },
    },
}
