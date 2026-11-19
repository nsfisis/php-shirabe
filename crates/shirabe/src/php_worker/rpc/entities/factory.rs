//! The static methods of `Composer\Factory` answered over RPC.

use crate::factory::__php_binding_factory;
use shirabe_php_binding_macros::php_statics;

php_statics! {
    class: r"Composer\Factory",
    methods: {
        r"Composer\Factory": {
            static: { adapters: __php_binding_factory },
        },
    },
}
