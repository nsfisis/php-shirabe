//! ref: composer/vendor/symfony/console/Helper/TableRows.php

use crate::symfony::console::helper::table::Row;

/// @internal
///
/// In PHP this wraps a `\Closure` yielding a `\Traversable` of row groups. The generator
/// borrows the Table to lazily call `fillCells()`. For the Rust port we precompute the row
/// groups eagerly (see `Table::build_table_rows`) and store them here.
#[derive(Debug)]
pub struct TableRows {
    row_groups: Vec<Vec<Row>>,
}

impl TableRows {
    pub fn from_row_groups(row_groups: Vec<Vec<Row>>) -> Self {
        Self { row_groups }
    }

    pub fn get_iterator(&self) -> std::slice::Iter<'_, Vec<Row>> {
        self.row_groups.iter()
    }

    pub fn into_row_groups(self) -> Vec<Vec<Row>> {
        self.row_groups
    }
}

impl<'a> IntoIterator for &'a TableRows {
    type Item = &'a Vec<Row>;
    type IntoIter = std::slice::Iter<'a, Vec<Row>>;

    fn into_iter(self) -> Self::IntoIter {
        self.row_groups.iter()
    }
}

impl IntoIterator for TableRows {
    type Item = Vec<Row>;
    type IntoIter = std::vec::IntoIter<Vec<Row>>;

    fn into_iter(self) -> Self::IntoIter {
        self.row_groups.into_iter()
    }
}
