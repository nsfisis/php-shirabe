//! The Rust-side entities a PHP proxy stub points back to, and the context one forwarded call is
//! answered in.
//!
//! An entity crosses the boundary as one of the worker-side classes that forward members to the
//! Rust side (a proxy stub, a runtime class). `forwarding` holds, for each of those classes, the
//! members it forwards as a trait and the `match` routing a call to them; both are generated
//! from the PHP side. What the Rust side answers is written by hand, as the impl of that trait.

use crate::php_worker::rpc::dispatch::error_throw;
use crate::php_worker::rpc::table;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_rpc::{FromWire, PhpThrow, PluginValue, RustObjHandle, wire};
use shirabe_php_shim::bformat;

/// A handle type pointing to a Rust-side entity that crosses the boundary.
pub trait Entity: Clone + 'static {
    /// How an rhandle naming another kind of entity is reported ("a Locker").
    const EXPECTED: &'static [u8];

    /// The allocation that is the PHP object's identity (`===`).
    fn address(&self) -> *const ();

    /// The class this entity crosses as. An error for a concrete type no forwarding class
    /// stands for.
    fn class(&self) -> Result<&'static Crossing<Self>, PhpThrow>;

    /// `__clone`. Only the kinds of entity whose Rust type models the PHP clone override this.
    fn clone_entity(&self) -> Result<Self, PhpThrow> {
        Err(PhpThrow::runtime(
            "cloning this Rust-side entity over RPC is not supported",
        ))
    }

    /// `__get` / `__set` / `__isset` / `__unset`.
    fn property(&self, access: PropertyAccess<'_>) -> Result<PluginValue, PhpThrow> {
        // TODO(plugin): the instance properties the proxied classes expose are widened on
        // demand. Each one has to decide how the state the real class keeps in that property is
        // served from the Rust-side entity.
        Err(PhpThrow::runtime(bformat!(
            "the property `{}` is not available over RPC yet",
            access.name()
        )))
    }

    /// A method only `worker.php` itself calls on this entity (`__shirabe_loop_services` on the
    /// Composer instance); `None` for every other name.
    fn worker_method(&self, call: &mut Call<'_>) -> Option<Result<PluginValue, PhpThrow>> {
        let _ = call;
        None
    }
}

/// One of the property forwarders every stub carries.
#[derive(Debug)]
pub enum PropertyAccess<'a> {
    Get {
        name: &'a BStr,
    },
    Set {
        name: &'a BStr,
        value: &'a PluginValue,
    },
    Isset {
        name: &'a BStr,
    },
    Unset {
        name: &'a BStr,
    },
}

impl<'a> PropertyAccess<'a> {
    pub fn name(&self) -> &'a BStr {
        match self {
            PropertyAccess::Get { name }
            | PropertyAccess::Set { name, .. }
            | PropertyAccess::Isset { name }
            | PropertyAccess::Unset { name } => name,
        }
    }
}

/// An entity crossing as one forwarding class: the name the child builds its stub as, and the
/// routing of the members that class forwards.
pub struct Crossing<H: 'static> {
    pub name: &'static [u8],
    pub dispatch: fn(&H, &mut Call<'_>) -> Result<PluginValue, PhpThrow>,
}

impl<H> std::fmt::Debug for Crossing<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Crossing")
            .field("name", &BStr::new(self.name))
            .finish_non_exhaustive()
    }
}

/// One forwarded call: a method, a static method or a constructor of a forwarding class.
#[derive(Debug)]
pub struct Call<'a> {
    /// The forwarding class the call was dispatched through, for messages.
    class: &'a BStr,
    /// The method, static method or `__construct` being called.
    method: &'a BStr,
    args: &'a [PluginValue],
    out_params: &'a mut IndexMap<u32, PluginValue>,
}

impl<'a> Call<'a> {
    pub(crate) fn new(
        class: &'a BStr,
        method: &'a BStr,
        args: &'a [PluginValue],
        out_params: &'a mut IndexMap<u32, PluginValue>,
    ) -> Self {
        Self {
            class,
            method,
            args,
            out_params,
        }
    }

    pub fn class(&self) -> &'a BStr {
        self.class
    }

    pub fn method(&self) -> &'a BStr {
        self.method
    }

    /// The arguments as they came over the wire, for a parameter `arg` cannot decode alone.
    pub fn args(&self) -> &'a [PluginValue] {
        self.args
    }

    /// Decodes the argument at `position`. A missing argument decodes the same way PHP's `null`
    /// does; a parameter's own default belongs to `arg_or`.
    pub fn arg<T: FromWire>(&self, position: usize) -> Result<T, PhpThrow> {
        T::from_wire(self.args.get(position).unwrap_or(&wire::NULL))
            .map_err(|throw| self.arg_throw(position, throw))
    }

    /// Decodes the argument at `position` for a parameter whose PHP declaration carries a
    /// default: an omitted argument, and the `null` PHP passes in its place, take `default`.
    pub fn arg_or<T: FromWire>(&self, position: usize, default: T) -> Result<T, PhpThrow> {
        match self.args.get(position) {
            None | Some(PluginValue::Null) => Ok(default),
            Some(value) => T::from_wire(value).map_err(|throw| self.arg_throw(position, throw)),
        }
    }

    pub fn entity<H: Entity>(&self, position: usize) -> Result<H, PhpThrow> {
        self.arg::<EntityArg<H>>(position).map(|entity| entity.0)
    }

    pub fn optional_entity<H: Entity>(&self, position: usize) -> Result<Option<H>, PhpThrow> {
        self.arg::<Option<EntityArg<H>>>(position)
            .map(|entity| entity.map(|entity| entity.0))
    }

    /// The value written back to the by-reference argument at `position`.
    pub fn set_out_param(&mut self, position: u32, value: PluginValue) {
        self.out_params.insert(position, value);
    }

    /// Names the argument a decoding failure is about.
    pub fn arg_throw(&self, position: usize, throw: PhpThrow) -> PhpThrow {
        PhpThrow {
            message: bformat!(
                "{}::{} argument {position}: {}",
                self.class,
                self.method,
                throw.message
            ),
            ..throw
        }
    }

    /// The `Throw` a failed Rust-side call of this member crosses the boundary as.
    pub fn failed(&self) -> impl Fn(anyhow::Error) -> PhpThrow + 'static {
        let context = bformat!("{}::{} failed", self.class, self.method);
        move |error| error_throw(&context, &error)
    }

    /// The answer of a forwarded member the Rust side does not serve.
    pub fn unsupported(&self) -> PhpThrow {
        PhpThrow::runtime(bformat!(
            "the {} method `{}` is not available over RPC yet",
            self.class,
            self.method
        ))
    }

    /// The answer of a forwarded member the Rust side declines to serve, for `reason`.
    pub fn unavailable(&self, reason: impl AsRef<[u8]>) -> PhpThrow {
        PhpThrow::runtime(bformat!(
            "the {} method `{}` is not available over RPC yet: {}",
            self.class,
            self.method,
            reason.as_ref()
        ))
    }
}

/// A member name the class a call was dispatched through does not forward. The stubs send only
/// the members of their own class, and an entity's class is fixed when it is registered, so this
/// is a disagreement between the two sides of the protocol.
pub fn protocol_violation(call: &Call<'_>) -> ! {
    panic!(
        "protocol violation: {:?} does not forward {:?}",
        call.class(),
        call.method()
    )
}

/// Registers `entity` in the R table and answers the descriptor the child builds its stub from.
pub fn entity_value<H: Entity>(entity: &H) -> Result<PluginValue, PhpThrow> {
    let (rhandle, class) = table::register(entity.clone())?;
    Ok(PluginValue::RustHandle(RustObjHandle {
        rhandle,
        class: BString::from(class.name),
    }))
}

/// An argument naming an entity of the R table.
#[derive(Debug)]
pub struct EntityArg<H>(pub H);

impl<H: Entity> FromWire for EntityArg<H> {
    fn from_wire(value: &PluginValue) -> Result<Self, PhpThrow> {
        let PluginValue::RustHandle(handle) = value else {
            return Err(wire::unexpected(H::EXPECTED, value));
        };
        table::lookup::<H>(handle.rhandle)
            .map(EntityArg)
            .ok_or_else(|| {
                PhpThrow::runtime(bformat!(
                    "expected {}, got Rust handle {}",
                    BStr::new(H::EXPECTED),
                    handle.rhandle
                ))
            })
    }
}
