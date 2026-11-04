//! ref: composer/src/Composer/EventDispatcher/Event.php

use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct Event {
    name: BString,
    args: Vec<BString>,
    flags: EventFlags,
    propagation_stopped: bool,
}

impl Event {
    pub fn new(name: impl Into<BString>, args: Vec<BString>, flags: EventFlags) -> Self {
        Self {
            name: name.into(),
            args,
            flags,
            propagation_stopped: false,
        }
    }

    pub fn from_name(name: impl Into<BString>) -> Self {
        Event::new(name, Vec::new(), EventFlags::default())
    }

    pub fn get_name(&self) -> &BStr {
        self.name.as_ref()
    }

    pub fn get_arguments(&self) -> &Vec<BString> {
        &self.args
    }

    pub fn get_flags(&self) -> &EventFlags {
        &self.flags
    }

    pub fn is_propagation_stopped(&self) -> bool {
        self.propagation_stopped
    }

    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
    }
}

/// The flags of an event, i.e. the `$flags` array that PHP's `Event::getFlags()` returns.
#[derive(Debug, Clone, Default)]
pub struct EventFlags {
    /// `script-alias-input`: the command line of a `composer <script-name>` invocation, minus the
    /// script name.
    pub script_alias_input: Option<BString>,
    /// `optimize`: whether the autoload dump scans PSR-0/4 packages into the class map, set on
    /// `pre-autoload-dump` and `post-autoload-dump`.
    pub optimize: Option<bool>,
    /// Other fields. Composer itself does not use them.
    // TODO(plugin): only a plugin can pass such flags (to `dispatchScript()`, or to the constructor
    // of an event it dispatches), and the plugin API does not accept them yet. Once it does,
    // `to_php_array` does not reproduce the array as passed in every case: the known keys always
    // come first in a fixed order, and a known key holding a value of another type has no
    // representation.
    pub extra: IndexMap<BString, PhpMixed>,
}

impl EventFlags {
    pub fn to_php_array(&self) -> IndexMap<BString, PhpMixed> {
        let mut flags = IndexMap::new();
        if let Some(script_alias_input) = &self.script_alias_input {
            flags.insert(
                BString::from("script-alias-input"),
                PhpMixed::string(script_alias_input.clone()),
            );
        }
        if let Some(optimize) = self.optimize {
            flags.insert(BString::from("optimize"), PhpMixed::Bool(optimize));
        }
        flags.extend(
            self.extra
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        flags
    }
}

pub trait EventInterface: std::fmt::Debug {
    /// For downcasting to the concrete event type (PHP `instanceof`), mirroring
    /// `IOInterface::as_any`.
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
    fn get_name(&self) -> &BStr;
    fn get_arguments(&self) -> &Vec<BString>;
    fn get_flags(&self) -> &EventFlags;
    fn is_propagation_stopped(&self) -> bool;
    fn stop_propagation(&mut self);
}

impl EventInterface for Event {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn get_name(&self) -> &BStr {
        self.name.as_ref()
    }

    fn get_arguments(&self) -> &Vec<BString> {
        &self.args
    }

    fn get_flags(&self) -> &EventFlags {
        &self.flags
    }

    fn is_propagation_stopped(&self) -> bool {
        self.propagation_stopped
    }

    fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
    }
}

macro_rules! delegate_event_interface_to_inner {
    ($Type:ty, $field:ident) => {
        impl crate::event_dispatcher::EventInterface for $Type {
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }

            fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
                self
            }

            fn get_name(&self) -> &bstr::BStr {
                self.$field.get_name()
            }

            fn get_arguments(&self) -> &Vec<bstr::BString> {
                self.$field.get_arguments()
            }

            fn get_flags(&self) -> &crate::event_dispatcher::EventFlags {
                self.$field.get_flags()
            }

            fn is_propagation_stopped(&self) -> bool {
                self.$field.is_propagation_stopped()
            }

            fn stop_propagation(&mut self) {
                self.$field.stop_propagation();
            }
        }
    };
}
pub(crate) use delegate_event_interface_to_inner;
