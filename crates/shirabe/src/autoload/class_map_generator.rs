//! ref: composer/src/Composer/Autoload/ClassMapGenerator.php

/// `Composer\Autoload\ClassMapGenerator`.
///
/// Deprecated since Composer 2.4.0 in favor of the composer/class-map-generator
/// package (`shirabe-class-map-generator`), which Composer itself now uses
/// directly. Composer's own code no longer references this class, so its
/// `dump` / `createMap` methods are intentionally left unported.
///
/// Even though it is deprecated, plugins may still use it, so this type will
/// eventually have to be implemented alongside plugin API support. It is left
/// here intentionally so that implementation is not forgotten.
///
/// TODO(plugin): implement `dump` / `createMap` for plugins still relying on
/// this deprecated class.
#[derive(Debug)]
#[deprecated(
    since = "Composer 2.4.0",
    note = "use the composer/class-map-generator package (shirabe-class-map-generator) instead"
)]
pub struct ClassMapGenerator;
