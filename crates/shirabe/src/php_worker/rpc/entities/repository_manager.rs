//! The methods a `Composer\Repository\RepositoryManager` entity answers over RPC.

use crate::repository::{__php_binding_repository_manager_interface, RepositoryManagerInterface};
use shirabe_php_binding_macros::php_entity;

// TODO(plugin): the remaining RepositoryManager surface is widened on demand.
php_entity! {
    handle: std::rc::Rc<std::cell::RefCell<dyn RepositoryManagerInterface>>,
    expected: "a RepositoryManager",
    class: r"Composer\Repository\RepositoryManager",
    unimplemented: [
        "__construct",
        "addRepository",
        "createRepository",
        "findPackage",
        "findPackages",
        "getRepositories",
        "prependRepository",
        "setLocalRepository",
        "setRepositoryClass",
    ],
    types: {
        r"Composer\Repository\RepositoryManager": instance
            __php_binding_repository_manager_interface for dyn RepositoryManagerInterface {
            via: |rm| rm.borrow(),
            mut: |rm| rm.borrow_mut(),
        },
    },
}
