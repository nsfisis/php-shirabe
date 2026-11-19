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
///     type_error: "a Locker",
///     classes: {
///         r"Composer\Package\Locker": { constructor: unimplemented },
///     },
///     methods: {
///         r"Composer\Package\Locker": {
///             instance: {
///                 adapters: crate::package::__php_binding_locker_interface,
///                 target: dyn LockerInterface,
///                 borrow: |locker| locker.borrow(),
///                 borrow_mut: |locker| locker.borrow_mut(),
///             },
///             static: { adapters: crate::package::__php_binding_locker },
///             refused: { getJsonFile: "JsonFile cannot cross the boundary" },
///         },
///     },
/// }
/// ```
///
/// Every part is `key: value`, each key given at most once. A string is a PHP class name or a
/// message, an identifier a PHP member name, a path a Rust item, and a closure an expression on
/// its one argument.
///
/// * `handle` is the handle type, and `type_error` names what an argument of another type was
///   expected to be. `identity` computes the entity's identity from the handle, the `Rc` itself by
///   default.
/// * `classes` lists, by class, the classes the entities cross as and their forwarding ancestors
///   (`ancestor: true`), which the entities never cross as. `constructor` answers a forwarded
///   constructor, and only a forwarded one: `{ adapters: module, into: |value| ... }` calls the
///   adapter of the inherent impl and makes a handle of the value it builds, `{ handwritten: path }`
///   calls a hand-written function, and `unimplemented` and `refused("reason")` are the "not
///   available over RPC yet" error, with the reason for a refused one.
/// * `class_of`, required when the entities cross as several classes, chooses the class of an
///   entity from the handle: a byte string in a `Result` whose error is the answer for an entity
///   no class stands for.
/// * `methods` lists, by the PHP type declaring them, what answers the forwarded methods:
///     * `instance`: the adapters of a trait or inherent impl, the type they call (`target`), and
///       how a handle reaches it: `borrow` (a `Ref`) or `deref` (a reference) for `&self` methods,
///       `borrow_mut` (a `RefMut`) for `&mut self` methods;
///     * `static`: the adapters of static methods;
///     * `handwritten`: a module of hand-written functions taking the handle, for the instance
///       methods it lists in `members`, or for every one;
///     * `unimplemented: [...]` and `refused: { member: "reason" }`: the "not available over RPC
///       yet" error, with the reason for a refused member.
///
///   A member listed in `unimplemented`, `refused` or `members` takes that answer whatever else the
///   entry gives.
/// * `handlers` names hand-written functions: `clone` answers `__clone`, `worker_method` the
///   methods only the worker runtime calls, `job` the job an `async fn` method's call starts, and
///   `properties: { name: path }` the property forwarders (`__get`, `__set`, `__isset`, `__unset`)
///   of an instance property the classes declare; any other property is the "not available over
///   RPC yet" error.
///
/// For each class, the declaration generates the dispatch of the methods its stub forwards, its
/// `Bindings`, and the `ForwardedClass` impl answering its constructor and static methods. A
/// forwarded member nothing answers, and anything given that no forwarded member reaches, fail
/// the build, as does a generated route whose adapter decodes other parameter types than the stub
/// of the class declares.
#[proc_macro]
pub fn php_entity(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    entity::expand(input.into()).into()
}

/// Generates the `ForwardedClass` impl of a class no entity crosses as, a guard whose static
/// methods are forwarded, routing each of them as `methods` of `php_entity!` does, with `static`,
/// `unimplemented` and `refused` entries.
///
/// ```ignore
/// php_statics! {
///     class: r"Composer\Factory",
///     methods: {
///         r"Composer\Factory": {
///             static: { adapters: crate::factory::__php_binding_factory },
///         },
///     },
/// }
/// ```
#[proc_macro]
pub fn php_statics(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    entity::expand_statics(input.into()).into()
}
