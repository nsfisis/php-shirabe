//! ref: composer/vendor/symfony/process/Pipes/PipesInterface.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

pub const CHUNK_SIZE: i64 = 16384;

/// PipesInterface manages descriptors and pipes for the use of proc_open.
pub trait PipesInterface: std::fmt::Debug {
    /// Returns an array of descriptors for the use of proc_open.
    fn get_descriptors(&mut self) -> Vec<PhpMixed>;

    /// Returns an array of filenames indexed by their related stream in case these pipes use temporary files.
    fn get_files(&self) -> IndexMap<i64, String>;

    /// Reads data in file handles and pipes.
    fn read_and_write(&mut self, blocking: bool, close: bool) -> IndexMap<i64, String>;

    /// Returns if the current state has open file handles or pipes.
    fn are_open(&self) -> bool;

    /// Returns if pipes are able to read output.
    fn have_read_support(&self) -> bool;

    /// Closes file handles and pipes.
    fn close(&mut self);

    /// Accessor for the `pipes` property populated by proc_open.
    fn pipes(&self) -> &PhpMixed;
    fn pipes_mut(&mut self) -> &mut PhpMixed;
}
