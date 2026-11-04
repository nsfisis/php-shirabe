//! ref: composer/vendor/symfony/process/Pipes/PipesInterface.php sha1:cfec64166c31d850525a82ff16e66c8debe95000

use indexmap::IndexMap;
use shirabe_php_shim::{Descriptor, PhpResource};

pub const CHUNK_SIZE: i64 = 16384;

/// PipesInterface manages descriptors and pipes for the use of proc_open.
pub trait PipesInterface: std::fmt::Debug {
    /// Returns an array of descriptors for the use of proc_open.
    fn get_descriptors(&mut self) -> Vec<Descriptor>;

    /// Returns an array of filenames indexed by their related stream in case these pipes use temporary files.
    fn get_files(&self) -> IndexMap<i64, bstr::BString>;

    /// Reads data in file handles and pipes.
    fn read_and_write(&mut self, blocking: bool, close: bool) -> IndexMap<i64, bstr::BString>;

    /// Returns if the current state has open file handles or pipes.
    fn are_open(&self) -> bool;

    /// Closes file handles and pipes.
    fn close(&mut self);

    /// Accessor for the `pipes` property populated by proc_open, keyed by fd index.
    fn pipes(&self) -> &IndexMap<i64, PhpResource>;
    fn pipes_mut(&mut self) -> &mut IndexMap<i64, PhpResource>;
}
