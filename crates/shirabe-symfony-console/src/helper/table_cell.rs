//! ref: composer/vendor/symfony/console/Helper/TableCell.php sha1:93b1e2c2f3285db45b0ba6f7d9a8912eae4112b0

use crate::exception::InvalidArgumentException;
use crate::helper::TableCellStyle;
use bstr::{B, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{Stringable, bconcat, implode};

/// A `TableCell` option value: an integer span, a `TableCellStyle`, or null.
#[derive(Debug, Clone)]
pub enum TableCellOption {
    Int(i64),
    Style(std::rc::Rc<TableCellStyle>),
    Null,
}

#[derive(Debug, Clone)]
pub struct TableCell {
    value: BString,
    options: IndexMap<BString, TableCellOption>,
}

impl TableCell {
    pub fn new(
        value: impl AsRef<[u8]>,
        options: IndexMap<BString, TableCellOption>,
    ) -> Result<Self, InvalidArgumentException> {
        let mut this_options: IndexMap<BString, TableCellOption> = IndexMap::new();
        this_options.insert("rowspan".into(), TableCellOption::Int(1));
        this_options.insert("colspan".into(), TableCellOption::Int(1));
        this_options.insert("style".into(), TableCellOption::Null);

        // check option names
        let diff: Vec<&BString> = options
            .keys()
            .filter(|key| !this_options.contains_key(*key))
            .collect();
        if !diff.is_empty() {
            return Err(InvalidArgumentException::new(bconcat!(
                "The TableCell does not support the following options: '",
                implode("', '", &diff),
                "'."
            )));
        }

        if let Some(style) = options.get(B("style"))
            && !matches!(style, TableCellOption::Style(_))
            && !matches!(style, TableCellOption::Null)
        {
            return Err(InvalidArgumentException::new(
                "The style option must be an instance of \"TableCellStyle\".",
            ));
        }

        for (key, option) in options {
            this_options.insert(key, option);
        }

        Ok(Self {
            value: BString::from(value.as_ref()),
            options: this_options,
        })
    }

    /// Two-argument constructor (`__construct(string $value, array $options)`).
    ///
    /// The options used by the Table helper are internally controlled, so a malformed-option
    /// error here would be a programming bug rather than a recoverable condition.
    pub fn new2(value: impl AsRef<[u8]>, options: IndexMap<BString, TableCellOption>) -> Self {
        Self::new(value, options).expect("TableCell options built internally are always valid")
    }

    /// Gets number of colspan.
    pub fn get_colspan(&self) -> i64 {
        match self.options[B("colspan")] {
            TableCellOption::Int(colspan) => colspan,
            _ => 0,
        }
    }

    /// Gets number of rowspan.
    pub fn get_rowspan(&self) -> i64 {
        match self.options[B("rowspan")] {
            TableCellOption::Int(rowspan) => rowspan,
            _ => 0,
        }
    }

    pub fn get_style(&self) -> Option<std::rc::Rc<TableCellStyle>> {
        match &self.options[B("style")] {
            TableCellOption::Style(style) => Some(style.clone()),
            _ => None,
        }
    }
}

impl Stringable for TableCell {
    fn to_php_string(&self) -> BString {
        self.value.clone()
    }
}
