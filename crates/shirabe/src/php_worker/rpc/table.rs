//! The R table: the Rust-side entities a PHP proxy stub points back to, and the handles that name
//! them on the wire.

use crate::event_dispatcher::{Event, EventInterface};
use crate::installer::{InstallerEvent, PackageEvent};
use crate::package::{AnyPackage, PackageInterfaceHandle};
use crate::php_worker::rpc::dispatch::dispatch_legacy_entity;
use crate::php_worker::rpc::entity::{Call, Crossing, Entity, EntityArg, PropertyAccess};
use crate::plugin::{
    CommandEvent, PostFileDownloadEvent, PreCommandRunEvent, PreFileDownloadEvent,
    PrePoolCreateEvent,
};
use crate::repository::RepositoryInterfaceHandle;
use crate::script::Event as ScriptEvent;
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_rpc::{FromWire, PhpThrow, PluginValue, RustObjHandle, ToWire, wire};
use shirabe_php_shim::{bformat, php_str};

/// A Rust-side entity of a kind still routed by hand.
#[derive(Debug, Clone)]
pub(crate) enum RustEntity {
    Package(std::rc::Rc<std::cell::RefCell<AnyPackage>>),
    Event(std::rc::Rc<std::cell::RefCell<dyn EventInterface>>),
}

fn entity_ptr_id(entity: &RustEntity) -> usize {
    match entity {
        RustEntity::Package(package) => std::rc::Rc::as_ptr(package) as *const () as usize,
        RustEntity::Event(event) => std::rc::Rc::as_ptr(event) as *const () as usize,
    }
}

/// An R-table entry: one registered entity, whatever its kind.
trait Slot: std::fmt::Debug {
    fn address(&self) -> *const ();
    fn call(
        &self,
        method: &BStr,
        args: &[PluginValue],
        out_params: &mut IndexMap<u32, PluginValue>,
    ) -> Result<PluginValue, PhpThrow>;
    fn as_any(&self) -> &dyn std::any::Any;
}

struct Registered<H: Entity> {
    entity: H,
    /// The class the child's stub was built as, fixed when the entity was registered.
    class: &'static Crossing<H>,
}

impl<H: Entity> std::fmt::Debug for Registered<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registered")
            .field("class", &BStr::new(self.class.name))
            .field("address", &self.entity.address())
            .finish()
    }
}

impl<H: Entity> Slot for Registered<H> {
    fn address(&self) -> *const () {
        self.entity.address()
    }

    fn call(
        &self,
        method: &BStr,
        args: &[PluginValue],
        out_params: &mut IndexMap<u32, PluginValue>,
    ) -> Result<PluginValue, PhpThrow> {
        let mut call = Call::new(BStr::new(self.class.name), method, args, out_params);
        match method.as_bytes() {
            b"__shirabe_clone" => {
                let (rhandle, _) = register(self.entity.clone_entity()?)?;
                Ok(PluginValue::Int(rhandle as i64))
            }
            b"__get" | b"__set" | b"__isset" | b"__unset" => {
                let name = call.arg::<BString>(0)?;
                let name = name.as_bstr();
                self.entity.property(match method.as_bytes() {
                    b"__get" => PropertyAccess::Get { name },
                    b"__set" => PropertyAccess::Set {
                        name,
                        value: args.get(1).unwrap_or(&wire::NULL),
                    },
                    b"__isset" => PropertyAccess::Isset { name },
                    b"__unset" => PropertyAccess::Unset { name },
                    _ => unreachable!(),
                })
            }
            _ => match self.entity.worker_method(&mut call) {
                Some(answer) => answer,
                None => (self.class.dispatch)(&self.entity, &mut call),
            },
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// An entity of a kind still served by the `RustEntity` routing.
#[derive(Debug)]
struct Legacy(RustEntity);

impl Slot for Legacy {
    fn address(&self) -> *const () {
        entity_ptr_id(&self.0) as *const ()
    }

    fn call(
        &self,
        method: &BStr,
        args: &[PluginValue],
        out_params: &mut IndexMap<u32, PluginValue>,
    ) -> Result<PluginValue, PhpThrow> {
        dispatch_legacy_entity(&self.0, method, args, out_params)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Debug, Default)]
struct RTable {
    slots: IndexMap<u64, std::rc::Rc<dyn Slot>>,
    /// `Slot::address` => rhandle.
    by_address: IndexMap<usize, u64>,
}

impl RTable {
    /// The rhandle `slot`'s entity is registered under, registering it if it is not yet.
    /// `same_kind` tells whether the slot already registered at that address holds the same kind
    /// of entity.
    fn intern(
        &mut self,
        slot: std::rc::Rc<dyn Slot>,
        same_kind: impl FnOnce(&dyn Slot) -> bool,
    ) -> (u64, std::rc::Rc<dyn Slot>) {
        let address = slot.address() as usize;
        if let Some(rhandle) = self.by_address.get(&address) {
            let existing = self.slots[rhandle].clone();
            // One PHP object belongs to one class, so one allocation crossing as two kinds of
            // entity is a bug of the Rust side.
            assert!(
                same_kind(&*existing),
                "an allocation already registered as {existing:?} is registered as {slot:?}"
            );
            return (*rhandle, existing);
        }
        let rhandle = shirabe_php_rpc::alloc_rhandle();
        self.slots.insert(rhandle, slot.clone());
        self.by_address.insert(address, rhandle);
        (rhandle, slot)
    }

    fn release(&mut self, rhandle: u64) -> Option<std::rc::Rc<dyn Slot>> {
        let slot = self.slots.shift_remove(&rhandle)?;
        self.by_address.shift_remove(&(slot.address() as usize));
        Some(slot)
    }
}

thread_local! {
    /// The R table. Entries are strong references held while the child-side stub lives; a
    /// ReleaseRustHandle notification (sent by the stub's `__destruct`) removes the entry.
    /// Handles are monotonically increasing and never reused, so a released handle can never
    /// be confused with a later entity (no generation counter is needed). A slot holds its
    /// entity strongly, so the address of a registered entity is not reused either.
    /// TODO(plugin): thread-local while the worker and its stub intern table are
    /// process-global; the session lock serializes calls, and every dispatch currently runs on
    /// the thread that registered the handle, but a handle minted on one thread is invisible
    /// to another.
    static R_TABLE: std::cell::RefCell<RTable> = {
        shirabe_php_rpc::set_release_rust_handle_hook(|rhandle| {
            let released = R_TABLE.with(|table| table.borrow_mut().release(rhandle));
            drop(released);
        });
        std::cell::RefCell::new(RTable::default())
    };
}

/// Registers an entity in the R table, interned by the allocation it points to, and answers its
/// rhandle and the class it crosses as.
pub(crate) fn register<H: Entity>(entity: H) -> Result<(u64, &'static Crossing<H>), PhpThrow> {
    let class = entity.class()?;
    let slot: std::rc::Rc<dyn Slot> = std::rc::Rc::new(Registered { entity, class });
    let (rhandle, slot) = R_TABLE.with(|table| {
        table
            .borrow_mut()
            .intern(slot, |existing| existing.as_any().is::<Registered<H>>())
    });
    let registered = slot
        .as_any()
        .downcast_ref::<Registered<H>>()
        .expect("interning keeps the kind of entity");
    Ok((rhandle, registered.class))
}

/// The entity of kind `H` an rhandle names.
pub(crate) fn lookup<H: Entity>(rhandle: u64) -> Option<H> {
    let slot = R_TABLE.with(|table| table.borrow().slots.get(&rhandle).cloned())?;
    slot.as_any()
        .downcast_ref::<Registered<H>>()
        .map(|registered| registered.entity.clone())
}

/// Every registered entity of kind `H`.
pub(crate) fn entities<H: Entity>() -> Vec<H> {
    R_TABLE.with(|table| {
        table
            .borrow()
            .slots
            .values()
            .filter_map(|slot| {
                slot.as_any()
                    .downcast_ref::<Registered<H>>()
                    .map(|registered| registered.entity.clone())
            })
            .collect()
    })
}

/// Serves a method call on an R-table entity. The table is not borrowed while the call runs: the
/// method may register further entities.
pub(crate) fn call(
    rhandle: u64,
    method: &BStr,
    args: &[PluginValue],
    out_params: &mut IndexMap<u32, PluginValue>,
) -> Result<PluginValue, PhpThrow> {
    let slot = R_TABLE.with(|table| table.borrow().slots.get(&rhandle).cloned());
    match slot {
        Some(slot) => slot.call(method, args, out_params),
        None => Err(PhpThrow::runtime(bformat!("unknown Rust handle {rhandle}"))),
    }
}

/// Registers an entity in the R table, interned by shared-pointer identity.
pub(crate) fn register_entity(entity: RustEntity) -> u64 {
    let discriminant = std::mem::discriminant(&entity);
    let slot: std::rc::Rc<dyn Slot> = std::rc::Rc::new(Legacy(entity));
    R_TABLE.with(|table| {
        table
            .borrow_mut()
            .intern(slot, |existing| {
                existing
                    .as_any()
                    .downcast_ref::<Legacy>()
                    .is_some_and(|legacy| std::mem::discriminant(&legacy.0) == discriminant)
            })
            .0
    })
}

fn legacy_entity(rhandle: u64) -> Option<RustEntity> {
    let slot = R_TABLE.with(|table| table.borrow().slots.get(&rhandle).cloned())?;
    slot.as_any()
        .downcast_ref::<Legacy>()
        .map(|legacy| legacy.0.clone())
}

/// A `RustHandle` wire descriptor for a freshly registered (or re-interned) entity.
pub(crate) fn rust_handle_value(rhandle: u64, class: impl AsRef<[u8]>) -> PluginValue {
    PluginValue::RustHandle(RustObjHandle {
        rhandle,
        class: BString::from(class.as_ref()),
    })
}

/// The proxy stub class matching a package's concrete variant.
fn package_stub_class(package: &std::rc::Rc<std::cell::RefCell<AnyPackage>>) -> &'static BStr {
    match &*package.borrow() {
        AnyPackage::Package(_) => php_str(b"Composer\\Package\\Package"),
        AnyPackage::CompletePackage(_) => php_str(b"Composer\\Package\\CompletePackage"),
        AnyPackage::RootPackage(_) => php_str(b"Composer\\Package\\RootPackage"),
        AnyPackage::AliasPackage(_) => php_str(b"Composer\\Package\\AliasPackage"),
        AnyPackage::CompleteAliasPackage(_) => php_str(b"Composer\\Package\\CompleteAliasPackage"),
        AnyPackage::RootAliasPackage(_) => php_str(b"Composer\\Package\\RootAliasPackage"),
    }
}

/// Registers a package and returns its wire descriptor.
pub(crate) fn package_handle_value(
    package: &std::rc::Rc<std::cell::RefCell<AnyPackage>>,
) -> PluginValue {
    let class = package_stub_class(package);
    let rhandle = register_entity(RustEntity::Package(package.clone()));
    rust_handle_value(rhandle, class)
}

/// The proxy stub class (same FQCN as the real event class) an event crosses the RPC
/// boundary as, or `None` when no stub exists for it yet.
pub(crate) fn event_stub_class(event: &dyn EventInterface) -> Option<&'static BStr> {
    if event.as_any().downcast_ref::<ScriptEvent>().is_some() {
        Some(BStr::new("Composer\\Script\\Event"))
    } else if event.as_any().downcast_ref::<PackageEvent>().is_some() {
        Some(BStr::new("Composer\\Installer\\PackageEvent"))
    } else if event.as_any().downcast_ref::<InstallerEvent>().is_some() {
        Some(BStr::new("Composer\\Installer\\InstallerEvent"))
    } else if event
        .as_any()
        .downcast_ref::<PreFileDownloadEvent>()
        .is_some()
    {
        Some(BStr::new("Composer\\Plugin\\PreFileDownloadEvent"))
    } else if event
        .as_any()
        .downcast_ref::<PostFileDownloadEvent>()
        .is_some()
    {
        Some(BStr::new("Composer\\Plugin\\PostFileDownloadEvent"))
    } else if event
        .as_any()
        .downcast_ref::<PrePoolCreateEvent>()
        .is_some()
    {
        Some(BStr::new("Composer\\Plugin\\PrePoolCreateEvent"))
    } else if event.as_any().downcast_ref::<CommandEvent>().is_some() {
        Some(BStr::new("Composer\\Plugin\\CommandEvent"))
    } else if event
        .as_any()
        .downcast_ref::<PreCommandRunEvent>()
        .is_some()
    {
        Some(BStr::new("Composer\\Plugin\\PreCommandRunEvent"))
    } else if event.as_any().downcast_ref::<Event>().is_some() {
        Some(BStr::new("Composer\\EventDispatcher\\Event"))
    } else {
        None
    }
}

/// A Rust-side entity a proxy stub stands for, as the `RustEntity` variant holding it.
pub(crate) trait FromRustEntity: Sized {
    /// What the value has to be, for the failure of a handle naming another kind of entity.
    const EXPECTED: &'static [u8];

    fn from_entity(entity: RustEntity) -> Option<Self>;
}

/// Resolves a handle descriptor back to the R-table entity it names.
fn rust_entity_from_wire<T: FromRustEntity>(value: &PluginValue) -> Result<T, PhpThrow> {
    let PluginValue::RustHandle(handle) = value else {
        return Err(wire::unexpected(T::EXPECTED, value));
    };
    legacy_entity(handle.rhandle)
        .and_then(T::from_entity)
        .ok_or_else(|| {
            PhpThrow::runtime(bformat!(
                "expected {}, got Rust handle {}",
                T::EXPECTED,
                handle.rhandle
            ))
        })
}

impl FromRustEntity for PackageInterfaceHandle {
    const EXPECTED: &'static [u8] = b"a package";

    fn from_entity(entity: RustEntity) -> Option<Self> {
        match entity {
            RustEntity::Package(package) => {
                Some(PackageInterfaceHandle::from_rc_unchecked(package))
            }
            _ => None,
        }
    }
}

impl FromWire for PackageInterfaceHandle {
    fn from_wire(value: &PluginValue) -> Result<Self, PhpThrow> {
        rust_entity_from_wire(value)
    }
}

impl ToWire for PackageInterfaceHandle {
    fn to_wire(self) -> PluginValue {
        package_handle_value(self.as_rc())
    }
}

impl FromWire for RepositoryInterfaceHandle {
    fn from_wire(value: &PluginValue) -> Result<Self, PhpThrow> {
        EntityArg::from_wire(value).map(|repository| repository.0)
    }
}
