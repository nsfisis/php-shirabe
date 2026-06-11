use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::helper::table_cell::{TableCell, TableCellOption};
use indexmap::IndexMap;

/// Marks a row as being a separator.
#[derive(Debug, Clone)]
pub struct TableSeparator {
    inner: TableCell,
}

impl TableSeparator {
    pub fn new() -> Self {
        Self::new1(IndexMap::new()).expect("TableSeparator default options are always valid")
    }

    pub fn new1(
        options: IndexMap<String, TableCellOption>,
    ) -> Result<Self, InvalidArgumentException> {
        Ok(Self {
            inner: TableCell::new("", options)?,
        })
    }
}

impl Default for TableSeparator {
    fn default() -> Self {
        Self::new()
    }
}
