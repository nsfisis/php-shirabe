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
/// On a trait, `proxy = Path` generates instead the impl of the trait for the adapter `Path` of an
/// entity living in the worker (which implements `PhpProxy`): each method calls the PHP method of
/// the same name on the entity and decodes its answer, waiting for the promise an `async fn`'s PHP
/// method answers with. A method only the Rust side has keeps its default body, or delegates to
/// the adapter's inherent method of the same name with `#[php(rust_only)]`. `#[php(custom = path)]`
/// delegates a method to the hand-written function `path(proxy, args...)`; a method whose PHP
/// counterpart may answer with null or an untyped value a non-`Option` Rust type cannot hold has
/// to.
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

/// Generates the `Entity` impl of a handle type, and for each class its entities cross as, the
/// routing of each member the class forwards to the adapter of the type declaring it.
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
///   `clone` and `worker_method` name the hand-written functions answering `__clone` and the
///   methods only the worker runtime calls.
/// * `properties: { "name": path }` names the hand-written functions answering the property
///   forwarders (`__get`, `__set`, `__isset`, `__unset`) for an instance property the classes
///   declare; any other property is the "not available over RPC yet" error.
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
/// * `unimplemented: [...]` and `refused: { "member": "reason" }` list the forwarded members the
///   Rust side does not answer: the "not available over RPC yet" error, with the reason for a
///   refused one. An entry names a member of every class (`"getFoo"`), or of a class and its
///   subclasses (`r"Composer\Foo::getFoo"`; a constructor of that class alone).
///
/// For each class, the declaration generates the dispatch of the methods its stub forwards, its
/// `Bindings`, and the `ForwardedClass` impl answering its constructor and static methods. A
/// forwarded member neither routed nor listed, and an entry naming no forwarded member, fail the
/// build, as does a generated route whose adapter decodes other parameter types than the stub of
/// the class declares.
#[proc_macro]
pub fn php_entity(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    entity::expand(input.into()).into()
}

/// Generates the `ForwardedClass` impl of a class no entity crosses as, a guard whose static
/// methods are forwarded, routing each of them to the adapter of the type declaring it. It takes
/// `unimplemented` and `refused` as `php_entity!` does.
///
/// ```ignore
/// php_statics! {
///     class: r"Composer\Factory",
///     types: {
///         r"Composer\Factory": static crate::factory::__php_binding_factory,
///     },
/// }
/// ```
#[proc_macro]
pub fn php_statics(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    entity::expand_statics(input.into()).into()
}
