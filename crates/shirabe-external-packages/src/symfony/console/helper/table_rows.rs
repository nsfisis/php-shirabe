//! ref: composer/vendor/symfony/console/Helper/TableRows.php

use shirabe_php_shim::PhpMixed;

/// @internal
///
/// In PHP this wraps a `\Closure` yielding a `\Traversable` of row groups. The generator
/// borrows the Table to lazily call `fillCells()`. For the Rust port we precompute the row
/// groups eagerly (see `Table::build_table_rows`) and store them here.
#[derive(Debug)]
pub struct TableRows {
    row_groups: Vec<Vec<PhpMixed>>,
}

impl TableRows {
    pub fn from_row_groups(row_groups: Vec<Vec<PhpMixed>>) -> Self {
        Self { row_groups }
    }

    pub fn get_iterator(&self) -> std::slice::Iter<'_, Vec<PhpMixed>> {
        self.row_groups.iter()
    }

    pub fn into_row_groups(self) -> Vec<Vec<PhpMixed>> {
        self.row_groups
    }
}

impl<'a> IntoIterator for &'a TableRows {
    type Item = &'a Vec<PhpMixed>;
    type IntoIter = std::slice::Iter<'a, Vec<PhpMixed>>;

    fn into_iter(self) -> Self::IntoIter {
        self.row_groups.iter()
    }
}

impl IntoIterator for TableRows {
    type Item = Vec<PhpMixed>;
    type IntoIter = std::vec::IntoIter<Vec<PhpMixed>>;

    fn into_iter(self) -> Self::IntoIter {
        self.row_groups.into_iter()
    }
}
