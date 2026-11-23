//! ref: composer/src/Composer/Installer/InstallerEvent.php sha1:1d678ec1d927d7a151afb1cf1cf37a70809ffa57

use crate::composer::ComposerWeakHandle;
use crate::dependency_resolver::AnyTransaction;
use crate::event_dispatcher::{Event, EventFlags, delegate_event_interface_to_inner};
use crate::io::IOInterface;
use bstr::BString;
use shirabe_php_binding_macros::php_binding;

#[derive(Debug)]
pub struct InstallerEvent {
    inner: Event,
    composer: ComposerWeakHandle,
    io: std::rc::Rc<dyn IOInterface>,
    dev_mode: bool,
    execute_operations: bool,
    transaction: std::rc::Rc<AnyTransaction>,
}

#[php_binding(r"Composer\Installer\InstallerEvent", members = instance)]
impl InstallerEvent {
    pub fn new(
        event_name: impl Into<BString>,
        composer: ComposerWeakHandle,
        io: std::rc::Rc<dyn IOInterface>,
        dev_mode: bool,
        execute_operations: bool,
        transaction: std::rc::Rc<AnyTransaction>,
    ) -> Self {
        let inner = Event::new(event_name, vec![], EventFlags::default());
        Self {
            inner,
            composer,
            io,
            dev_mode,
            execute_operations,
            transaction,
        }
    }

    pub fn get_composer(&self) -> &ComposerWeakHandle {
        &self.composer
    }

    pub fn get_io(&self) -> std::rc::Rc<dyn IOInterface> {
        self.io.clone()
    }

    pub fn is_dev_mode(&self) -> bool {
        self.dev_mode
    }

    pub fn is_executing_operations(&self) -> bool {
        self.execute_operations
    }

    pub fn get_transaction(&self) -> Option<&std::rc::Rc<AnyTransaction>> {
        Some(&self.transaction)
    }
}

delegate_event_interface_to_inner!(InstallerEvent, inner, "Composer\\Installer\\InstallerEvent");
