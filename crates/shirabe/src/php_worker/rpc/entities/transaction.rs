//! The methods a `Composer\DependencyResolver\Transaction` entity answers over RPC.

use crate::dependency_resolver::{__php_binding_any_transaction, AnyTransaction};
use shirabe_php_binding_macros::php_entity;

php_entity! {
    handle: std::rc::Rc<AnyTransaction>,
    type_error: "a Transaction",
    class_of: |transaction| Ok(match &**transaction {
        AnyTransaction::Transaction(_) => br"Composer\DependencyResolver\Transaction",
        AnyTransaction::LocalRepoTransaction(_) => {
            br"Composer\DependencyResolver\LocalRepoTransaction"
        }
    }),
    classes: {
        r"Composer\DependencyResolver\Transaction": { constructor: unimplemented },
        r"Composer\DependencyResolver\LocalRepoTransaction": { constructor: unimplemented },
    },
    methods: {
        r"Composer\DependencyResolver\Transaction": {
            instance: {
                adapters: __php_binding_any_transaction,
                target: AnyTransaction,
                deref: |transaction| &**transaction,
            },
        },
    },
}
