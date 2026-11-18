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
    expected: "an event",
    class: |event| {
        let event = event.borrow();
        event.php_class().map(|class| &**class).ok_or_else(|| {
            // TODO(plugin): an event class without a proxy stub cannot cross the boundary.
            PhpThrow::runtime(bformat!(
                "no proxy stub is available yet for the event `{}`",
                event.get_name()
            ))
        })
    },
    classes: [
        r"Composer\EventDispatcher\Event",
        r"Composer\Script\Event",
        r"Composer\Installer\PackageEvent",
        r"Composer\Installer\InstallerEvent",
        r"Composer\Plugin\CommandEvent",
        r"Composer\Plugin\PreCommandRunEvent",
        r"Composer\Plugin\PreFileDownloadEvent",
        r"Composer\Plugin\PostFileDownloadEvent",
        r"Composer\Plugin\PrePoolCreateEvent",
    ],
    types: {
        r"Composer\EventDispatcher\Event": instance __php_binding_event_interface
            for dyn EventInterface {
            via: |event| event.borrow(),
            mut: |event| event.borrow_mut(),
        },
        r"Composer\Script\Event": instance __php_binding_event for ScriptEvent {
            via: |event| std::cell::Ref::map(event.borrow(), |event| {
                event.as_any().downcast_ref::<ScriptEvent>().expect(CONCRETE)
            }),
            mut: |event| std::cell::RefMut::map(event.borrow_mut(), |event| {
                event.as_any_mut().downcast_mut::<ScriptEvent>().expect(CONCRETE)
            }),
        },
        r"Composer\Installer\PackageEvent": instance __php_binding_package_event for PackageEvent {
            via: |event| std::cell::Ref::map(event.borrow(), |event| {
                event.as_any().downcast_ref::<PackageEvent>().expect(CONCRETE)
            }),
            mut: |event| std::cell::RefMut::map(event.borrow_mut(), |event| {
                event.as_any_mut().downcast_mut::<PackageEvent>().expect(CONCRETE)
            }),
        },
        r"Composer\Installer\InstallerEvent": instance __php_binding_installer_event for InstallerEvent {
            via: |event| std::cell::Ref::map(event.borrow(), |event| {
                event.as_any().downcast_ref::<InstallerEvent>().expect(CONCRETE)
            }),
            mut: |event| std::cell::RefMut::map(event.borrow_mut(), |event| {
                event.as_any_mut().downcast_mut::<InstallerEvent>().expect(CONCRETE)
            }),
        },
        r"Composer\Plugin\CommandEvent": instance __php_binding_command_event for CommandEvent {
            via: |event| std::cell::Ref::map(event.borrow(), |event| {
                event.as_any().downcast_ref::<CommandEvent>().expect(CONCRETE)
            }),
            mut: |event| std::cell::RefMut::map(event.borrow_mut(), |event| {
                event.as_any_mut().downcast_mut::<CommandEvent>().expect(CONCRETE)
            }),
        },
        r"Composer\Plugin\PreCommandRunEvent": instance __php_binding_pre_command_run_event for PreCommandRunEvent {
            via: |event| std::cell::Ref::map(event.borrow(), |event| {
                event.as_any().downcast_ref::<PreCommandRunEvent>().expect(CONCRETE)
            }),
            mut: |event| std::cell::RefMut::map(event.borrow_mut(), |event| {
                event.as_any_mut().downcast_mut::<PreCommandRunEvent>().expect(CONCRETE)
            }),
        },
        r"Composer\Plugin\PreFileDownloadEvent": instance __php_binding_pre_file_download_event for PreFileDownloadEvent {
            via: |event| std::cell::Ref::map(event.borrow(), |event| {
                event.as_any().downcast_ref::<PreFileDownloadEvent>().expect(CONCRETE)
            }),
            mut: |event| std::cell::RefMut::map(event.borrow_mut(), |event| {
                event.as_any_mut().downcast_mut::<PreFileDownloadEvent>().expect(CONCRETE)
            }),
        },
        r"Composer\Plugin\PostFileDownloadEvent": instance __php_binding_post_file_download_event for PostFileDownloadEvent {
            via: |event| std::cell::Ref::map(event.borrow(), |event| {
                event.as_any().downcast_ref::<PostFileDownloadEvent>().expect(CONCRETE)
            }),
            mut: |event| std::cell::RefMut::map(event.borrow_mut(), |event| {
                event.as_any_mut().downcast_mut::<PostFileDownloadEvent>().expect(CONCRETE)
            }),
        },
        r"Composer\Plugin\PrePoolCreateEvent": instance __php_binding_pre_pool_create_event for PrePoolCreateEvent {
            via: |event| std::cell::Ref::map(event.borrow(), |event| {
                event.as_any().downcast_ref::<PrePoolCreateEvent>().expect(CONCRETE)
            }),
            mut: |event| std::cell::RefMut::map(event.borrow_mut(), |event| {
                event.as_any_mut().downcast_mut::<PrePoolCreateEvent>().expect(CONCRETE)
            }),
        },
    },
}
