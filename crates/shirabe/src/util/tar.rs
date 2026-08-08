//! ref: composer/src/Composer/Util/Tar.php

use indexmap::IndexMap;
use shirabe_php_shim::{PharData, RuntimeException};

pub struct Tar;

impl Tar {
    pub fn get_composer_json(path_to_archive: &str) -> anyhow::Result<Option<String>> {
        let phar = PharData::new(path_to_archive)?;

        if !phar.valid() {
            return Ok(None);
        }

        Ok(Some(Self::extract_composer_json_from_folder(&phar)?))
    }

    /// The content bytes are decoded strictly: a composer.json that is not valid
    /// UTF-8 could never survive the JSON parsing that follows in PHP either.
    fn content_to_string(content: Vec<u8>) -> anyhow::Result<String> {
        String::from_utf8(content).map_err(|_| {
            RuntimeException::new("composer.json in the archive is not valid UTF-8".to_string())
                .into()
        })
    }

    fn extract_composer_json_from_folder(phar: &PharData) -> anyhow::Result<String> {
        if let Some(file) = phar.get("composer.json") {
            return Self::content_to_string(file.get_content());
        }

        let mut top_level_paths: IndexMap<String, bool> = IndexMap::new();
        for folder_file in phar.iter() {
            let name = folder_file.get_basename();
            if folder_file.is_dir() {
                top_level_paths.insert(name, true);
                if top_level_paths.len() > 1 {
                    return Err(RuntimeException::new(format!(
                        "Archive has more than one top level directories, and no composer.json was found on the top level, so it's an invalid archive. Top level paths found were: {}",
                        top_level_paths
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(",")
                    )).into());
                }
            }
        }

        let composer_json_path = format!(
            "{}/composer.json",
            top_level_paths.keys().next().cloned().unwrap_or_default()
        );
        if !top_level_paths.is_empty()
            && let Some(file) = phar.get(&composer_json_path)
        {
            return Self::content_to_string(file.get_content());
        }

        Err(RuntimeException::new(
            "No composer.json found either at the top level or within the topmost directory"
                .to_string(),
        )
        .into())
    }
}
