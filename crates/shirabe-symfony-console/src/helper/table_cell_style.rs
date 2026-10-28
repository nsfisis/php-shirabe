//! ref: composer/vendor/symfony/console/Helper/TableCellStyle.php

use crate::exception::InvalidArgumentException;
use bstr::{B, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{PhpMixed, STR_PAD_BOTH, STR_PAD_LEFT, STR_PAD_RIGHT};

pub const DEFAULT_ALIGN: &str = "left";

const TAG_OPTIONS: [&str; 3] = ["fg", "bg", "options"];

/// Maps an alignment name to the corresponding `STR_PAD_*` value.
fn align_map(align: &str) -> Option<i64> {
    match align {
        "left" => Some(STR_PAD_RIGHT),
        "center" => Some(STR_PAD_BOTH),
        "right" => Some(STR_PAD_LEFT),
        _ => None,
    }
}

fn align_map_keys() -> Vec<&'static str> {
    vec!["left", "center", "right"]
}

#[derive(Debug)]
pub struct TableCellStyle {
    options: IndexMap<BString, PhpMixed>,
}

impl TableCellStyle {
    pub fn new(options: IndexMap<BString, PhpMixed>) -> Result<Self, InvalidArgumentException> {
        let mut this_options: IndexMap<BString, PhpMixed> = IndexMap::new();
        this_options.insert("fg".into(), PhpMixed::string("default".to_string()));
        this_options.insert("bg".into(), PhpMixed::string("default".to_string()));
        this_options.insert("options".into(), PhpMixed::Null);
        this_options.insert("align".into(), PhpMixed::string(DEFAULT_ALIGN.to_string()));
        this_options.insert("cellFormat".into(), PhpMixed::Null);

        let diff: Vec<String> = options
            .keys()
            .filter(|key| !this_options.contains_key(*key))
            .map(|key| key.to_str_lossy().into_owned())
            .collect();
        if !diff.is_empty() {
            return Err(InvalidArgumentException::new(format!(
                "The TableCellStyle does not support the following options: '{}'.",
                diff.join("', '"),
            )));
        }

        if let Some(align) = options.get(B("align")) {
            let align = match align {
                PhpMixed::String(align) => align.to_str_lossy().into_owned(),
                _ => String::new(),
            };
            if align_map(&align).is_none() {
                return Err(InvalidArgumentException::new(format!(
                    "Wrong align value. Value must be following: '{}'.",
                    align_map_keys().join("', '"),
                )));
            }
        }

        for (key, value) in options {
            this_options.insert(key, value);
        }

        Ok(Self {
            options: this_options,
        })
    }

    pub fn get_options(&self) -> IndexMap<BString, PhpMixed> {
        self.options.clone()
    }

    /// Gets options we need for tag for example fg, bg.
    pub fn get_tag_options(&self) -> IndexMap<BString, PhpMixed> {
        let mut result: IndexMap<BString, PhpMixed> = IndexMap::new();
        for (key, value) in self.get_options() {
            if TAG_OPTIONS.iter().any(|option| key == *option)
                && !matches!(self.options[&key], PhpMixed::Null)
            {
                result.insert(key, value);
            }
        }
        result
    }

    pub fn get_pad_by_align(&self) -> i64 {
        let align = match &self.get_options()[B("align")] {
            PhpMixed::String(align) => align.to_str_lossy().into_owned(),
            _ => String::new(),
        };
        align_map(&align).unwrap()
    }

    pub fn get_cell_format(&self) -> Option<String> {
        match &self.get_options()[B("cellFormat")] {
            PhpMixed::String(format) => Some(format.to_str_lossy().into_owned()),
            _ => None,
        }
    }
}
