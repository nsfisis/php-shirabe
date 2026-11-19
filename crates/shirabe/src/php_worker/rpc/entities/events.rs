//! The methods the events declare, answered over RPC: the base event's, and those each event
//! class declares beyond it.

use crate::event_dispatcher::{__php_binding_event_interface, EventInterface};
use crate::installer::{
    __php_binding_installer_event, __php_binding_package_event, InstallerEvent, PackageEvent,
};
use crate::plugin::{
    __php_binding_command_event, __php_binding_post_file_download_event,
    __php_binding_pre_command_run_event, __php_binding_pre_file_download_event,
    __php_binding_pre_pool_create_event, CommandEvent, PostFileDownloadEvent, PreCommandRunEvent,
    PreFileDownloadEvent, PrePoolCreateEvent,
};
use crate::script::{__php_binding_event, Event as ScriptEvent};
use shirabe_php_binding_macros::php_entity;
use shirabe_php_rpc::PhpThrow;
use shirabe_php_shim::bformat;

#[expect(clippy::disallowed_types, reason = "the message `expect` takes")]
const CONCRETE: &str = "an event crosses as the class of its concrete type";

// TODO(plugin): the originating event of a script event needs the event it wraps proxied the
// other way round.
php_entity! {
    handle: std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
    type_error: "an event",
    class_of: |event| {
        let event = event.borrow();
        event.php_class().map(|class| &**class).ok_or_else(|| {
            // TODO(plugin): an event class without a proxy stub cannot cross the boundary.
            PhpThrow::runtime(bformat!(
                "no proxy stub is available yet for the event `{}`",
                event.get_name()
            ))
        })
    },
    classes: {
        r"Composer\EventDispatcher\Event": {},
        r"Composer\Script\Event": { constructor: unimplemented },
        r"Composer\Installer\PackageEvent": { constructor: unimplemented },
        r"Composer\Installer\InstallerEvent": { constructor: unimplemented },
        r"Composer\Plugin\CommandEvent": { constructor: unimplemented },
        r"Composer\Plugin\PreCommandRunEvent": {},
        r"Composer\Plugin\PreFileDownloadEvent": { constructor: unimplemented },
        r"Composer\Plugin\PostFileDownloadEvent": { constructor: unimplemented },
        r"Composer\Plugin\PrePoolCreateEvent": { constructor: unimplemented },
    },
    methods: {
        r"Composer\EventDispatcher\Event": {
            instance: {
                adapters: __php_binding_event_interface,
                target: dyn EventInterface,
                borrow: |event| event.borrow(),
                borrow_mut: |event| event.borrow_mut(),
            },
        },
        r"Composer\Script\Event": {
            instance: {
                adapters: __php_binding_event,
                target: ScriptEvent,
                borrow: |event| std::cell::Ref::map(event.borrow(), |event| {
                    event.as_any().downcast_ref::<ScriptEvent>().expect(CONCRETE)
                }),
                borrow_mut: |event| std::cell::RefMut::map(event.borrow_mut(), |event| {
                    event.as_any_mut().downcast_mut::<ScriptEvent>().expect(CONCRETE)
                }),
            },
            unimplemented: [getOriginatingEvent, setOriginatingEvent],
        },
        r"Composer\Installer\PackageEvent": {
            instance: {
                adapters: __php_binding_package_event,
                target: PackageEvent,
                borrow: |event| std::cell::Ref::map(event.borrow(), |event| {
                    event.as_any().downcast_ref::<PackageEvent>().expect(CONCRETE)
                }),
                borrow_mut: |event| std::cell::RefMut::map(event.borrow_mut(), |event| {
                    event.as_any_mut().downcast_mut::<PackageEvent>().expect(CONCRETE)
                }),
            },
        },
        r"Composer\Installer\InstallerEvent": {
            instance: {
                adapters: __php_binding_installer_event,
                target: InstallerEvent,
                borrow: |event| std::cell::Ref::map(event.borrow(), |event| {
                    event.as_any().downcast_ref::<InstallerEvent>().expect(CONCRETE)
                }),
                borrow_mut: |event| std::cell::RefMut::map(event.borrow_mut(), |event| {
                    event.as_any_mut().downcast_mut::<InstallerEvent>().expect(CONCRETE)
                }),
            },
        },
        r"Composer\Plugin\CommandEvent": {
            instance: {
                adapters: __php_binding_command_event,
                target: CommandEvent,
                borrow: |event| std::cell::Ref::map(event.borrow(), |event| {
                    event.as_any().downcast_ref::<CommandEvent>().expect(CONCRETE)
                }),
                borrow_mut: |event| std::cell::RefMut::map(event.borrow_mut(), |event| {
                    event.as_any_mut().downcast_mut::<CommandEvent>().expect(CONCRETE)
                }),
            },
            refused: {
                getInput: "the console input cannot cross the boundary",
                getOutput: "the console output cannot cross the boundary",
            },
        },
        r"Composer\Plugin\PreCommandRunEvent": {
            instance: {
                adapters: __php_binding_pre_command_run_event,
                target: PreCommandRunEvent,
                borrow: |event| std::cell::Ref::map(event.borrow(), |event| {
                    event.as_any().downcast_ref::<PreCommandRunEvent>().expect(CONCRETE)
                }),
                borrow_mut: |event| std::cell::RefMut::map(event.borrow_mut(), |event| {
                    event.as_any_mut().downcast_mut::<PreCommandRunEvent>().expect(CONCRETE)
                }),
            },
            refused: {
                getInput: "the console input cannot cross the boundary",
            },
        },
        r"Composer\Plugin\PreFileDownloadEvent": {
            instance: {
                adapters: __php_binding_pre_file_download_event,
                target: PreFileDownloadEvent,
                borrow: |event| std::cell::Ref::map(event.borrow(), |event| {
                    event.as_any().downcast_ref::<PreFileDownloadEvent>().expect(CONCRETE)
                }),
                borrow_mut: |event| std::cell::RefMut::map(event.borrow_mut(), |event| {
                    event.as_any_mut().downcast_mut::<PreFileDownloadEvent>().expect(CONCRETE)
                }),
            },
        },
        r"Composer\Plugin\PostFileDownloadEvent": {
            instance: {
                adapters: __php_binding_post_file_download_event,
                target: PostFileDownloadEvent,
                borrow: |event| std::cell::Ref::map(event.borrow(), |event| {
                    event.as_any().downcast_ref::<PostFileDownloadEvent>().expect(CONCRETE)
                }),
                borrow_mut: |event| std::cell::RefMut::map(event.borrow_mut(), |event| {
                    event.as_any_mut().downcast_mut::<PostFileDownloadEvent>().expect(CONCRETE)
                }),
            },
        },
        r"Composer\Plugin\PrePoolCreateEvent": {
            instance: {
                adapters: __php_binding_pre_pool_create_event,
                target: PrePoolCreateEvent,
                borrow: |event| std::cell::Ref::map(event.borrow(), |event| {
                    event.as_any().downcast_ref::<PrePoolCreateEvent>().expect(CONCRETE)
                }),
                borrow_mut: |event| std::cell::RefMut::map(event.borrow_mut(), |event| {
                    event.as_any_mut().downcast_mut::<PrePoolCreateEvent>().expect(CONCRETE)
                }),
            },
            refused: {
                getRequest: "the Request cannot cross the boundary",
            },
        },
    },
}
