//! The methods a `Composer\DependencyResolver\Transaction` entity answers over RPC.

use crate::dependency_resolver::{__php_binding_any_transaction, AnyTransaction};
use shirabe_php_binding_macros::php_entity;

php_entity! {
    handle: std::rc::Rc<AnyTransaction>,
    expected: "a Transaction",
    class: |transaction| Ok(match &**transaction {
        AnyTransaction::Transaction(_) => br"Composer\DependencyResolver\Transaction",
        AnyTransaction::LocalRepoTransaction(_) => {
            br"Composer\DependencyResolver\LocalRepoTransaction"
        }
    }),
    classes: [
        r"Composer\DependencyResolver\Transaction",
        r"Composer\DependencyResolver\LocalRepoTransaction",
    ],
    unimplemented: [
        r"Composer\DependencyResolver\LocalRepoTransaction::__construct",
        r"Composer\DependencyResolver\Transaction::__construct",
    ],
    types: {
        r"Composer\DependencyResolver\Transaction": instance __php_binding_any_transaction
            for AnyTransaction {
            deref: |transaction| &**transaction,
        },
    },
}
