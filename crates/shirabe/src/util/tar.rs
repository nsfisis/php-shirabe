//! ref: composer/src/Composer/Util/Tar.php

use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::{PharData, RuntimeException, bconcat, implode};

pub struct Tar;

impl Tar {
    pub fn get_composer_json(path_to_archive: &std::path::Path) -> anyhow::Result<Option<BString>> {
        let phar = PharData::new(path_to_archive)?;

        if !phar.valid() {
            return Ok(None);
        }

        Ok(Some(Self::extract_composer_json_from_folder(&phar)?))
    }

    fn extract_composer_json_from_folder(phar: &PharData) -> anyhow::Result<BString> {
        if let Some(file) = phar.get("composer.json") {
            return Ok(file.get_content().into());
        }

        let mut top_level_paths: IndexMap<BString, bool> = IndexMap::new();
        for folder_file in phar.iter() {
            let name = folder_file.get_basename();
            if folder_file.is_dir() {
                top_level_paths.insert(name, true);
                if top_level_paths.len() > 1 {
                    return Err(RuntimeException::new(format!(
                        "Archive has more than one top level directories, and no composer.json was found on the top level, so it's an invalid archive. Top level paths found were: {}",
                        implode(",", &top_level_paths.keys().collect::<Vec<_>>()).display_lossy()
                    )).into());
                }
            }
        }

        let composer_json_path = bconcat!(
            top_level_paths.keys().next().cloned().unwrap_or_default(),
            "/composer.json"
        );
        if !top_level_paths.is_empty()
            && let Some(file) = phar.get(&composer_json_path)
        {
            return Ok(file.get_content().into());
        }

        Err(RuntimeException::new(
            "No composer.json found either at the top level or within the topmost directory"
                .to_string(),
        )
        .into())
    }
}
