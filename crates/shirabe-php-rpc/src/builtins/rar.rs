//! The `RarArchive` and `RarEntry` classes of the PECL rar extension, answered by the PHP worker:
//! every method forwards to the extension's object there, so RAR archives can be read only when
//! the PHP the worker runs has the extension loaded, as under Composer.
//!
//! TODO(port): there is no native RAR decoder. Packages distributed as RAR archives are rare, and
//! the available decoders (the `unrar` crate and its bindings) build on the UnRAR library, whose
//! license is not an open source one. A native decoder could still replace the worker here.

use crate::PhpObjHandle;
use crate::builtins::or_false;
use crate::transport::{RefuseCallbacks, call_php_method, call_static_method};
use crate::wire::{ToWire, protocol_value};
use bstr::BStr;
use shirabe_php_shim::php_str;

const CLASS: &BStr = php_str(b"RarArchive");

#[derive(Debug)]
pub struct RarArchive {
    entity: PhpObjHandle,
}

impl RarArchive {
    /// PHP `RarArchive::open($filename)`, with PHP's `false` mapped to `None`.
    pub fn open(file: impl AsRef<std::path::Path>) -> anyhow::Result<Option<Self>> {
        let value = call_static_method(
            CLASS,
            "open",
            vec![file.as_ref().to_wire()],
            &mut RefuseCallbacks,
        )??;
        Ok(or_false(&value, "RarArchive::open()").map(|entity| RarArchive { entity }))
    }

    /// PHP `$archive->getEntries()`, with PHP's `false` mapped to `None`.
    pub fn get_entries(&self) -> anyhow::Result<Option<Vec<RarEntry>>> {
        let value = call_php_method(&self.entity, "getEntries", vec![], &mut RefuseCallbacks)??;
        Ok(
            or_false::<Vec<PhpObjHandle>>(&value, "RarArchive::getEntries()").map(|entities| {
                entities
                    .into_iter()
                    .map(|entity| RarEntry { entity })
                    .collect()
            }),
        )
    }

    /// PHP `$archive->close()`.
    pub fn close(&self) -> anyhow::Result<bool> {
        let value = call_php_method(&self.entity, "close", vec![], &mut RefuseCallbacks)??;
        Ok(protocol_value(&value, "RarArchive::close()"))
    }
}

#[derive(Debug)]
pub struct RarEntry {
    entity: PhpObjHandle,
}

impl RarEntry {
    /// PHP `$entry->extract($dir)`.
    pub fn extract(&self, dir: impl AsRef<std::path::Path>) -> anyhow::Result<bool> {
        let value = call_php_method(
            &self.entity,
            "extract",
            vec![dir.as_ref().to_wire()],
            &mut RefuseCallbacks,
        )??;
        Ok(protocol_value(&value, "RarEntry::extract()"))
    }
}
