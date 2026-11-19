//! The methods a `Composer\Repository\RepositoryManager` entity answers over RPC.

use crate::repository::{__php_binding_repository_manager_interface, RepositoryManagerInterface};
use shirabe_php_binding_macros::php_entity;

// TODO(plugin): the remaining RepositoryManager surface is widened on demand.
php_entity! {
    handle: std::rc::Rc<std::cell::RefCell<dyn RepositoryManagerInterface>>,
    type_error: "a RepositoryManager",
    classes: {
        r"Composer\Repository\RepositoryManager": { constructor: unimplemented },
    },
    methods: {
        r"Composer\Repository\RepositoryManager": {
            instance: {
                adapters: __php_binding_repository_manager_interface,
                target: dyn RepositoryManagerInterface,
                borrow: |rm| rm.borrow(),
                borrow_mut: |rm| rm.borrow_mut(),
            },
            unimplemented: [
                addRepository,
                createRepository,
                findPackage,
                findPackages,
                getRepositories,
                prependRepository,
                setLocalRepository,
                setRepositoryClass,
            ],
        },
    },
}
