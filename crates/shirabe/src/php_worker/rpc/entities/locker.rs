//! The methods a `Composer\Package\Locker` entity answers over RPC.

use crate::package::{__php_binding_locker, __php_binding_locker_interface, LockerInterface};
use shirabe_php_binding_macros::php_entity;

php_entity! {
    handle: std::rc::Rc<std::cell::RefCell<dyn LockerInterface>>,
    type_error: "a Locker",
    classes: {
        r"Composer\Package\Locker": { constructor: unimplemented },
    },
    methods: {
        r"Composer\Package\Locker": {
            instance: {
                adapters: __php_binding_locker_interface,
                target: dyn LockerInterface,
                borrow: |handle| handle.borrow(),
                borrow_mut: |handle| handle.borrow_mut(),
            },
            static: { adapters: __php_binding_locker },
            refused: {
                getJsonFile: "JsonFile cannot cross the boundary",
                updateHash: "JsonFile cannot cross the boundary",
            },
        },
    },
}
