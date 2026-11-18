//! The Rust entity table: the Rust-side entities a PHP proxy stub points back to, and the ids that
//! name them in frames.

use crate::package::PackageInterfaceHandle;
use crate::php_worker::rpc::entity::{Bindings, Call, Entity, EntityArg, PropertyAccess};
use crate::repository::RepositoryInterfaceHandle;
use bstr::{BStr, BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe_php_rpc::codec::DecodeError;
use shirabe_php_rpc::{FromRpcValue, PhpThrow, RpcValue, codec};
use shirabe_php_shim::bformat;

/// A Rust entity table entry: one registered entity, whatever its kind.
trait Slot: std::fmt::Debug {
    fn address(&self) -> *const ();
    fn call(
        &self,
        method: &BStr,
        args: &[RpcValue],
        out_params: &mut IndexMap<u32, RpcValue>,
    ) -> Result<RpcValue, PhpThrow>;
    fn as_any(&self) -> &dyn std::any::Any;
}

struct Registered<H: Entity> {
    entity: H,
    /// The class the child's stub was built as, fixed when the entity was registered.
    class: &'static Bindings<H>,
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
        args: &[RpcValue],
        out_params: &mut IndexMap<u32, RpcValue>,
    ) -> Result<RpcValue, PhpThrow> {
        let mut call = Call::new(BStr::new(self.class.name), method, args, out_params);
        match method.as_bytes() {
            b"__shirabe_clone" => {
                let (entity_id, _) = register(self.entity.clone_entity()?)?;
                Ok(RpcValue::Int(entity_id as i64))
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
struct RustEntityTable {
    slots: IndexMap<u64, std::rc::Rc<dyn Slot>>,
    /// `Slot::address` => Rust entity id.
    by_address: IndexMap<usize, u64>,
}

impl RustEntityTable {
    /// The Rust entity id `slot`'s entity is registered under, registering it if it is not yet.
    /// `same_kind` tells whether the slot already registered at that address holds the same kind
    /// of entity.
    fn intern(
        &mut self,
        slot: std::rc::Rc<dyn Slot>,
        same_kind: impl FnOnce(&dyn Slot) -> bool,
    ) -> (u64, std::rc::Rc<dyn Slot>) {
        let address = slot.address() as usize;
        if let Some(entity_id) = self.by_address.get(&address) {
            let existing = self.slots[entity_id].clone();
            // One PHP object belongs to one class, so one allocation crossing as two kinds of
            // entity is a bug of the Rust side.
            assert!(
                same_kind(&*existing),
                "an allocation already registered as {existing:?} is registered as {slot:?}"
            );
            return (*entity_id, existing);
        }
        let entity_id = shirabe_php_rpc::alloc_entity_id();
        self.slots.insert(entity_id, slot.clone());
        self.by_address.insert(address, entity_id);
        (entity_id, slot)
    }

    fn release(&mut self, entity_id: u64) -> Option<std::rc::Rc<dyn Slot>> {
        let slot = self.slots.shift_remove(&entity_id)?;
        self.by_address.shift_remove(&(slot.address() as usize));
        Some(slot)
    }
}

thread_local! {
    /// The Rust entity table. Entries are strong references held while the child-side stub lives;
    /// a ReleaseHandle notification (sent by the stub's `__destruct`) removes the entry. Ids are
    /// monotonically increasing and never reused, so a released id can never be confused with a
    /// later entity (no generation counter is needed). A slot holds its entity strongly, so the
    /// address of a registered entity is not reused either.
    /// TODO(plugin): thread-local while the worker and its stub intern table are
    /// process-global; the session lock serializes calls, and every dispatch currently runs on
    /// the thread that registered the entity, but an id minted on one thread is invisible
    /// to another.
    static RUST_ENTITY_TABLE: std::cell::RefCell<RustEntityTable> = {
        shirabe_php_rpc::set_release_rust_entity_hook(|entity_id| {
            let released = RUST_ENTITY_TABLE.with(|table| table.borrow_mut().release(entity_id));
            drop(released);
        });
        std::cell::RefCell::new(RustEntityTable::default())
    };
}

/// Registers an entity in the Rust entity table, interned by the allocation it points to, and
/// answers its Rust entity id and the class it crosses as.
pub(crate) fn register<H: Entity>(entity: H) -> Result<(u64, &'static Bindings<H>), PhpThrow> {
    let class = entity.class()?;
    let slot: std::rc::Rc<dyn Slot> = std::rc::Rc::new(Registered { entity, class });
    let (entity_id, slot) = RUST_ENTITY_TABLE.with(|table| {
        table
            .borrow_mut()
            .intern(slot, |existing| existing.as_any().is::<Registered<H>>())
    });
    let registered = slot
        .as_any()
        .downcast_ref::<Registered<H>>()
        .expect("interning keeps the kind of entity");
    Ok((entity_id, registered.class))
}

/// The entity of kind `H` a Rust entity id names.
pub(crate) fn lookup<H: Entity>(entity_id: u64) -> Option<H> {
    let slot = RUST_ENTITY_TABLE.with(|table| table.borrow().slots.get(&entity_id).cloned())?;
    slot.as_any()
        .downcast_ref::<Registered<H>>()
        .map(|registered| registered.entity.clone())
}

/// Every registered entity of kind `H`.
pub(crate) fn entities<H: Entity>() -> Vec<H> {
    RUST_ENTITY_TABLE.with(|table| {
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

/// Serves a method call on an entity of the Rust entity table. The table is not borrowed while the
/// call runs: the method may register further entities.
pub(crate) fn call(
    entity_id: u64,
    method: &BStr,
    args: &[RpcValue],
    out_params: &mut IndexMap<u32, RpcValue>,
) -> Result<RpcValue, PhpThrow> {
    let slot = RUST_ENTITY_TABLE.with(|table| table.borrow().slots.get(&entity_id).cloned());
    match slot {
        Some(slot) => slot.call(method, args, out_params),
        None => Err(PhpThrow::runtime(bformat!(
            "unknown Rust entity id {entity_id}"
        ))),
    }
}

impl FromRpcValue for PackageInterfaceHandle {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError> {
        EntityArg::from_rpc_value(value).map(|package| package.0)
    }
}

impl FromRpcValue for RepositoryInterfaceHandle {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError> {
        EntityArg::from_rpc_value(value).map(|repository| repository.0)
    }
}
