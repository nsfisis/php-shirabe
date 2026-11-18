//! The bindings between the Rust side and the PHP worker, generated from the PHP declarations
//! `scripts/plugin-stub-generator/generate-stubs` writes under the calling crate's `php-decls/`.
//!
//! The expansion names paths of the `shirabe` crate (`crate::php_worker::rpc::...`), so only that
//! crate uses these macros.

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    reason = "the crate generates Rust source, which rustc reads as text"
)]

mod binding;
mod decls;
mod entity;

/// Generates, next to a trait or an inherent impl standing for a PHP type, one adapter per member
/// of that type the item answers: a function decoding the arguments of a forwarded call, calling
/// the Rust method and encoding its answer.
///
/// ```ignore
/// #[php_binding(r"Composer\Package\Locker")]
/// pub trait LockerInterface { ... }
///
/// #[php_binding(r"Composer\Package\Locker", members = static)]
/// impl Locker { ... }
/// ```
///
/// The adapters go in a child module named after the item (`__php_binding_locker_interface`),
/// which sees the item's module through `use super::*`. The item answers the members the PHP
/// type declares itself, not the ones it inherits. A Rust method answers the PHP member whose
/// Rust name it has, unless a method names the member with `#[php(name = "...")]`; on an
/// inherent impl, `#[php(name = "__construct")]` names the function answering the constructor,
/// whose adapter returns the value it builds. `members` restricts the members the item answers to
/// `instance`, `static` or `constructor` ones, or a parenthesized list of them. Further
/// annotations:
///
/// * `#[php(skip)]` on a method: no adapter.
/// * `#[php(custom = path)]` on a method: the adapter is the hand-written function at `path`.
/// * `#[php(decode = T)]` on a parameter: the argument is decoded as `T`.
///
/// The child module also holds `__php_params`: for each adapter, the PHP parameter types it
/// decodes the arguments against, which `php_entity!` compares with what the stubs declare.
#[proc_macro_attribute]
pub fn php_binding(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    binding::expand(attr.into(), item.into()).into()
}

/// Generates the `Entity` impl of a handle type, and the `Forwarding` impl of each class its
/// entities cross as, routing each member the class forwards and the Rust side answers to the
/// adapter of the type declaring it.
///
/// ```ignore
/// php_entity! {
///     handle: std::rc::Rc<std::cell::RefCell<dyn LockerInterface>>,
///     expected: "a Locker",
///     class: r"Composer\Package\Locker",
///     types: {
///         r"Composer\Package\Locker": instance crate::package::__php_binding_locker_interface
///             for dyn LockerInterface {
///             via: |handle| handle.borrow(),
///             mut: |handle| handle.borrow_mut(),
///         },
///         r"Composer\Package\Locker": static crate::package::__php_binding_locker,
///     },
/// }
/// ```
///
/// * `class` is the class the entities cross as, or a closure choosing it from the handle, as a
///   byte string in a `Result` whose error is the answer for an entity no class stands for;
///   `classes` then lists every class it chooses from, with their forwarding
///   ancestors.
/// * `address` computes the entity's identity from the handle, the `Rc` itself by default;
///   `clone` and `property` name the hand-written functions answering `__clone` and the
///   property forwarders.
/// * Each entry of `types` names what answers the instance or static methods a PHP type
///   declares, or only the ones it lists after `only [...]`. An `instance` entry gives the type
///   the adapters call (`for`) and how a handle reaches it, for `&self` methods through a
///   `RefCell` borrow (`via`) or a plain reference (`deref`) and for `&mut self` methods (`mut`);
///   a `static` entry the adapters of static methods; a `custom`
///   entry a module of hand-written functions taking the handle.
/// * Each entry of `constructors` gives, for a class whose constructor the Rust side answers,
///   the adapter of the inherent impl and how the value it builds becomes a handle
///   (`module => |value| ...`), or a hand-written function (`custom path`).
///
/// A generated route is checked at compile time against the stub of the class: an adapter whose
/// parameter types differ from the ones the stub declares fails the build.
#[proc_macro]
pub fn php_entity(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    entity::expand(input.into()).into()
}
