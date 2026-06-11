use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use indexmap::IndexMap;

pub const DEFAULT_ALIGN: &str = "left";

const TAG_OPTIONS: [&str; 3] = ["fg", "bg", "options"];

/// Maps an alignment name to the corresponding `STR_PAD_*` value.
fn align_map(align: &str) -> Option<i64> {
    match align {
        "left" => Some(shirabe_php_shim::STR_PAD_RIGHT),
        "center" => Some(shirabe_php_shim::STR_PAD_BOTH),
        "right" => Some(shirabe_php_shim::STR_PAD_LEFT),
        _ => None,
    }
}

fn align_map_keys() -> Vec<&'static str> {
    vec!["left", "center", "right"]
}

#[derive(Debug)]
pub struct TableCellStyle {
    options: IndexMap<String, shirabe_php_shim::PhpMixed>,
}

impl TableCellStyle {
    pub fn new(
        options: IndexMap<String, shirabe_php_shim::PhpMixed>,
    ) -> Result<Self, InvalidArgumentException> {
        let mut this_options: IndexMap<String, shirabe_php_shim::PhpMixed> = IndexMap::new();
        this_options.insert(
            "fg".to_string(),
            shirabe_php_shim::PhpMixed::String("default".to_string()),
        );
        this_options.insert(
            "bg".to_string(),
            shirabe_php_shim::PhpMixed::String("default".to_string()),
        );
        this_options.insert("options".to_string(), shirabe_php_shim::PhpMixed::Null);
        this_options.insert(
            "align".to_string(),
            shirabe_php_shim::PhpMixed::String(DEFAULT_ALIGN.to_string()),
        );
        this_options.insert("cellFormat".to_string(), shirabe_php_shim::PhpMixed::Null);

        let diff: Vec<String> = options
            .keys()
            .filter(|key| !this_options.contains_key(*key))
            .cloned()
            .collect();
        if !diff.is_empty() {
            return Err(InvalidArgumentException(
                shirabe_php_shim::InvalidArgumentException {
                    message: shirabe_php_shim::sprintf(
                        "The TableCellStyle does not support the following options: '%s'.",
                        &[shirabe_php_shim::PhpMixed::String(diff.join("', '"))],
                    ),
                    code: 0,
                },
            ));
        }

        if let Some(align) = options.get("align") {
            let align = match align {
                shirabe_php_shim::PhpMixed::String(align) => align.clone(),
                _ => String::new(),
            };
            if align_map(&align).is_none() {
                return Err(InvalidArgumentException(
                    shirabe_php_shim::InvalidArgumentException {
                        message: shirabe_php_shim::sprintf(
                            "Wrong align value. Value must be following: '%s'.",
                            &[shirabe_php_shim::PhpMixed::String(
                                align_map_keys().join("', '"),
                            )],
                        ),
                        code: 0,
                    },
                ));
            }
        }

        for (key, value) in options {
            this_options.insert(key, value);
        }

        Ok(Self {
            options: this_options,
        })
    }

    pub fn get_options(&self) -> IndexMap<String, shirabe_php_shim::PhpMixed> {
        self.options.clone()
    }

    /// Gets options we need for tag for example fg, bg.
    ///
    /// @return string[]
    pub fn get_tag_options(&self) -> IndexMap<String, shirabe_php_shim::PhpMixed> {
        let mut result: IndexMap<String, shirabe_php_shim::PhpMixed> = IndexMap::new();
        for (key, value) in self.get_options() {
            if TAG_OPTIONS.contains(&key.as_str())
                && !matches!(self.options[&key], shirabe_php_shim::PhpMixed::Null)
            {
                result.insert(key, value);
            }
        }
        result
    }

    pub fn get_pad_by_align(&self) -> i64 {
        let align = match &self.get_options()["align"] {
            shirabe_php_shim::PhpMixed::String(align) => align.clone(),
            _ => String::new(),
        };
        align_map(&align).unwrap()
    }

    pub fn get_cell_format(&self) -> Option<String> {
        match &self.get_options()["cellFormat"] {
            shirabe_php_shim::PhpMixed::String(format) => Some(format.clone()),
            _ => None,
        }
    }
}
