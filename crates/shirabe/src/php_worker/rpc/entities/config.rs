//! The methods a `Composer\Config` entity answers over RPC.

use crate::config::{__php_binding_config, Config};
use shirabe_php_binding_macros::php_entity;

// TODO(plugin): the config-source surface (getConfigSource / setAuthConfigSource / ...) needs
// proxy stubs for ConfigSourceInterface implementations, and prohibitUrlByConfig needs the IO
// argument decoded back into the Rust-side instance.
php_entity! {
    handle: std::rc::Rc<std::cell::RefCell<Config>>,
    type_error: "a Config",
    classes: {
        r"Composer\Config": { constructor: unimplemented },
    },
    methods: {
        r"Composer\Config": {
            instance: {
                adapters: __php_binding_config,
                target: Config,
                borrow: |config| config.borrow(),
                borrow_mut: |config| config.borrow_mut(),
            },
            unimplemented: [
                getAuthConfigSource,
                getConfigSource,
                getLocalAuthConfigSource,
                prohibitUrlByConfig,
                setAuthConfigSource,
                setBaseDir,
                setConfigSource,
                setLocalAuthConfigSource,
            ],
        },
    },
}
