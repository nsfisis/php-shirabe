//! ref: composer/src/Composer/Package/Loader/JsonLoader.php

use crate::json::JsonFile;
use crate::package::PackageInterfaceHandle;
use crate::package::loader::LoaderInterface;
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::{PhpMixed, TypeError, file_exists, file_get_contents, path_from_bytes};

pub enum JsonLoaderInput {
    File(JsonFile),
    String(BString),
}

pub struct JsonLoader {
    loader: Box<dyn LoaderInterface>,
}

impl JsonLoader {
    pub fn new(loader: Box<dyn LoaderInterface>) -> Self {
        Self { loader }
    }

    pub fn load(&self, json: JsonLoaderInput) -> anyhow::Result<PackageInterfaceHandle> {
        let config = match json {
            JsonLoaderInput::File(json_file) => json_file.read()?,
            JsonLoaderInput::String(ref s) if file_exists(path_from_bytes(s)) => {
                let contents = file_get_contents(path_from_bytes(s))?;
                JsonFile::parse_json(Some(&contents), Some(bstr::BStr::new(s)))?
            }
            JsonLoaderInput::String(ref s) => JsonFile::parse_json(Some(s), None)?,
        };

        let config: IndexMap<BString, PhpMixed> = match config {
            PhpMixed::Array(m) => m,
            _ => {
                return Err(TypeError::new(BString::from("Composer\\Package\\Loader\\LoaderInterface::load(): Argument #1 ($config) must be of type array"))
                .into());
            }
        };

        self.loader.load(config, None)
    }
}
