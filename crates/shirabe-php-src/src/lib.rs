//! Rust port from the original C implementation in php-src.
//! See the `LICENSE` file in this crate.
#![allow(special_module_name)]
//!
//! Rules for this crate:
//!
//! * Any code written while looking at php-src belongs here, and nowhere else. `shirabe-php-shim`
//!   must not contain php-src derived code; it depends on this crate instead.
//! * The dependency direction is `shirabe-php-shim` -> `shirabe-php-src` only. This crate must not
//!   depend on any other crate in the workspace, so that BSD-derived code cannot pull in
//!   MIT-licensed types.
//! * The module layout mirrors php-src's own tree (`src/standard/string.rs` holds code derived
//!   from `ext/standard/string.c`), so the origin of a function is visible from its path.
//! * Every item carries a fixed-format origin comment:
//!
//!   ```text
//!   /// php-src: ext/standard/strnatcmp.c `strnatcmp_ex` (PHP 8.5.2)
//!   ```

// The module mirrors php-src's `main/` directory; it is not a binary entry point (autobins is
// disabled in Cargo.toml). `special_module_name` is allowed crate-wide because the item-level
// attribute does not reach this early-pass lint.
pub mod main;
pub mod standard;
pub mod zend;
