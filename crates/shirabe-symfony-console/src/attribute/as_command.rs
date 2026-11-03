//! ref: composer/vendor/symfony/console/Attribute/AsCommand.php

use bstr::BString;
use shirabe_php_shim::{explode, implode};

/// Service tag to autoconfigure commands.
///
/// PHP attribute: #[\Attribute(\Attribute::TARGET_CLASS)]
#[derive(Debug)]
pub struct AsCommand {
    pub name: BString,
    pub description: Option<BString>,
}

impl AsCommand {
    pub fn new(
        name: BString,
        description: Option<BString>,
        aliases: Vec<BString>,
        hidden: bool,
    ) -> Self {
        let mut this = Self { name, description };

        if !hidden && aliases.is_empty() {
            return this;
        }

        let mut name = explode("|", &this.name);
        name.extend(aliases);

        if hidden && !name[0].is_empty() {
            name.insert(0, BString::default());
        }

        this.name = implode("|", &name);

        this
    }
}
