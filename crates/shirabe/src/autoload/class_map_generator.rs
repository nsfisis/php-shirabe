//! ref: composer/src/Composer/Autoload/ClassMapGenerator.php

use indexmap::IndexMap;

use shirabe_class_map_generator::class_map_generator::ClassMapGenerator as ExternalClassMapGenerator;
use shirabe_php_shim::PhpMixed;

use crate::io::IOInterface;

#[derive(Debug)]
pub struct ClassMapGenerator;

impl ClassMapGenerator {
    pub fn dump(dirs: Vec<String>, file: &str) -> anyhow::Result<()> {
        let mut maps: IndexMap<String, String> = IndexMap::new();
        for dir in dirs {
            maps.extend(ClassMapGenerator::create_map(
                PhpMixed::String(dir),
                None,
                None,
                None,
                None,
                &mut IndexMap::new(),
            )?);
        }
        let maps_php = PhpMixed::Array(
            maps.into_iter()
                .map(|(k, v)| (k, Box::new(PhpMixed::String(v))))
                .collect(),
        );
        std::fs::write(
            file,
            format!(
                "<?php return {};",
                shirabe_php_shim::var_export(&maps_php, true)
            ),
        )?;
        Ok(())
    }

    pub fn create_map(
        path: PhpMixed,
        excluded: Option<String>,
        mut io: Option<Box<dyn IOInterface>>,
        namespace: Option<String>,
        autoload_type: Option<String>,
        scanned_files: &mut IndexMap<String, bool>,
    ) -> anyhow::Result<IndexMap<String, String>> {
        let _ = scanned_files;
        let mut generator = ExternalClassMapGenerator::new(vec![
            "php".to_string(),
            "inc".to_string(),
            "hh".to_string(),
        ]);
        // TODO(phase-b): scanned_files tracking via avoid_duplicate_scans not wired up
        generator.avoid_duplicate_scans(None);

        generator.scan_paths(
            path,
            excluded,
            autoload_type.as_deref().unwrap_or("classmap"),
            namespace,
            vec![],
        )?;

        let class_map = generator.get_class_map();

        if let Some(io) = io.as_mut() {
            for msg in class_map.get_psr_violations() {
                io.write_error(&format!("<warning>{}</warning>", msg));
            }

            for (class, paths) in class_map.get_ambiguous_classes(None)? {
                if paths.len() > 1 {
                    io.write_error(&format!(
                        "<warning>Warning: Ambiguous class resolution, \"{}\" was found {}x: in \"{}\" and \"{}\", the first will be used.</warning>",
                        class,
                        paths.len() + 1,
                        class_map.get_class_path(&class).unwrap_or(""),
                        paths.join("\", \""),
                    ));
                } else {
                    io.write_error(&format!(
                        "<warning>Warning: Ambiguous class resolution, \"{}\" was found in both \"{}\" and \"{}\", the first will be used.</warning>",
                        class,
                        class_map.get_class_path(&class).unwrap_or(""),
                        paths.join("\", \""),
                    ));
                }
            }
        }

        Ok(class_map.get_map().clone())
    }
}
