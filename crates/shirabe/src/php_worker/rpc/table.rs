//! The R table: the Rust-side entities a PHP proxy stub points back to, and the handles that name
//! them on the wire.

use crate::package::PackageInterfaceHandle;
use crate::php_worker::rpc::entity::{Call, Crossing, Entity, EntityArg, PropertyAccess};
use crate::repository::RepositoryInterfaceHandle;
use bstr::{BStr, BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe_php_rpc::{FromPluginValue, PhpThrow, PluginValue, codec};
use shirabe_php_shim::bformat;

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
                        value: args.get(1).unwrap_or(&codec::NULL),
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

impl FromPluginValue for PackageInterfaceHandle {
    fn from_plugin_value(value: &PluginValue) -> Result<Self, PhpThrow> {
        EntityArg::from_plugin_value(value).map(|package| package.0)
    }
}

impl FromPluginValue for RepositoryInterfaceHandle {
    fn from_plugin_value(value: &PluginValue) -> Result<Self, PhpThrow> {
        EntityArg::from_plugin_value(value).map(|repository| repository.0)
    }
}
