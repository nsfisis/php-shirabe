//! ref: composer/vendor/symfony/console/Helper/TableSeparator.php

use crate::exception::InvalidArgumentException;
use crate::helper::{TableCell, TableCellOption};
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

    fn new1(options: IndexMap<String, TableCellOption>) -> Result<Self, InvalidArgumentException> {
        Ok(Self {
            inner: TableCell::new("", options)?,
        })
    }

    // PHP `TableSeparator extends TableCell`, so these inherited accessors remain available.
    pub fn get_colspan(&self) -> i64 {
        self.inner.get_colspan()
    }

    pub fn get_rowspan(&self) -> i64 {
        self.inner.get_rowspan()
    }

    pub fn get_style(&self) -> Option<std::rc::Rc<crate::helper::TableCellStyle>> {
        self.inner.get_style()
    }
}

impl Default for TableSeparator {
    fn default() -> Self {
        Self::new()
    }
}
