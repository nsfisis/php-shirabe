//! ref: composer/vendor/symfony/console/Helper/TableCell.php

use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::helper::table_cell_style::TableCellStyle;
use indexmap::IndexMap;
use std::rc::Rc;

/// A `TableCell` option value: an integer span, a `TableCellStyle`, or null.
#[derive(Debug, Clone)]
pub enum TableCellOption {
    Int(i64),
    Style(Rc<TableCellStyle>),
    Null,
}

#[derive(Debug, Clone)]
pub struct TableCell {
    pub(crate) value: String,
    options: IndexMap<String, TableCellOption>,
}

impl TableCell {
    pub fn new(
        value: &str,
        options: IndexMap<String, TableCellOption>,
    ) -> Result<Self, InvalidArgumentException> {
        let mut this_options: IndexMap<String, TableCellOption> = IndexMap::new();
        this_options.insert("rowspan".to_string(), TableCellOption::Int(1));
        this_options.insert("colspan".to_string(), TableCellOption::Int(1));
        this_options.insert("style".to_string(), TableCellOption::Null);

        // check option names
        let diff: Vec<String> = options
            .keys()
            .filter(|key| !this_options.contains_key(*key))
            .cloned()
            .collect();
        if !diff.is_empty() {
            return Err(InvalidArgumentException(
                shirabe_php_shim::InvalidArgumentException {
                    message: format!(
                        "The TableCell does not support the following options: '{}'.",
                        shirabe_php_shim::PhpMixed::String(diff.join("', '")),
                    ),
                    code: 0,
                },
            ));
        }

        if let Some(style) = options.get("style")
            && !matches!(style, TableCellOption::Style(_))
            && !matches!(style, TableCellOption::Null)
        {
            return Err(InvalidArgumentException(
                shirabe_php_shim::InvalidArgumentException {
                    message: "The style option must be an instance of \"TableCellStyle\"."
                        .to_string(),
                    code: 0,
                },
            ));
        }

        for (key, option) in options {
            this_options.insert(key, option);
        }

        Ok(Self {
            value: value.to_string(),
            options: this_options,
        })
    }

    /// Two-argument constructor (`__construct(string $value, array $options)`).
    ///
    /// The options used by the Table helper are internally controlled, so a malformed-option
    /// error here would be a programming bug rather than a recoverable condition.
    pub fn new2(value: &str, options: IndexMap<String, TableCellOption>) -> Self {
        Self::new(value, options).expect("TableCell options built internally are always valid")
    }

    /// Returns the cell value.
    pub fn to_string(&self) -> String {
        self.value.clone()
    }

    /// Gets number of colspan.
    pub fn get_colspan(&self) -> i64 {
        match self.options["colspan"] {
            TableCellOption::Int(colspan) => colspan,
            _ => 0,
        }
    }

    /// Gets number of rowspan.
    pub fn get_rowspan(&self) -> i64 {
        match self.options["rowspan"] {
            TableCellOption::Int(rowspan) => rowspan,
            _ => 0,
        }
    }

    pub fn get_style(&self) -> Option<Rc<TableCellStyle>> {
        match &self.options["style"] {
            TableCellOption::Style(style) => Some(style.clone()),
            _ => None,
        }
    }
}

impl std::fmt::Display for TableCell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}
