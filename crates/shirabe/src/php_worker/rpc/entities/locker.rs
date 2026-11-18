//! The methods a `Composer\Package\Locker` entity answers over RPC.

use crate::package::{__php_binding_locker, __php_binding_locker_interface, LockerInterface};
use shirabe_php_binding_macros::php_entity;

php_entity! {
    handle: std::rc::Rc<std::cell::RefCell<dyn LockerInterface>>,
    expected: "a Locker",
    class: r"Composer\Package\Locker",
    unimplemented: [
        "__construct",
    ],
    refused: {
        "getJsonFile": "JsonFile cannot cross the boundary",
        "updateHash": "JsonFile cannot cross the boundary",
    },
    types: {
        r"Composer\Package\Locker": instance __php_binding_locker_interface
            for dyn LockerInterface {
            via: |handle| handle.borrow(),
            mut: |handle| handle.borrow_mut(),
        },
        r"Composer\Package\Locker": static __php_binding_locker,
    },
}
