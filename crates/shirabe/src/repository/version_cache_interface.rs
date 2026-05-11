//! ref: composer/src/Composer/Repository/VersionCacheInterface.php

pub trait VersionCacheInterface {
    // No class implementing this interface exists in Composer's codebase; a plugin may provide
    // one, but plugin support is not yet decided. Using () as a placeholder until then.
    fn get_version_package(&self, version: &str, identifier: &str) -> ();
}
