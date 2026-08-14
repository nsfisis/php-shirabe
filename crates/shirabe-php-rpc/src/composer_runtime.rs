//! The Composer PHP runtime the worker loads.
//!
//! See `docs/dev/composer-runtime-bundle.md`.

use crate::PluginValue;
use crate::call_function;

const BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/composer-runtime-bundle.phar"));

/// Identifies the bundle by its contents; the sentinel entry holds the same string.
const BUNDLE_ID: &str = env!("SHIRABE_COMPOSER_RUNTIME_BUNDLE_ID");

/// The entry the worker reads back to tell a bundle it can use from one it cannot.
const SENTINEL_PATH: &str = "shirabe/bundle-id";

/// The alias `Phar::loadPhar` maps the bundle to in the worker. The bundle stores no alias of
/// its own, so this name is the only one its stream paths answer to.
const ALIAS: &str = "shirabe-composer-runtime.phar";

/// The path the Composer PHP runtime's files sit under in the worker, either inside this
/// executable or in the directory the bundle was extracted to.
pub fn base_path() -> anyhow::Result<String> {
    static BASE: std::sync::OnceLock<Result<String, String>> = std::sync::OnceLock::new();
    BASE.get_or_init(|| resolve().map_err(|e| format!("{e:#}")))
        .clone()
        .map_err(|e| anyhow::anyhow!(e))
}

fn resolve() -> anyhow::Result<String> {
    if worker_opens_bundle()? {
        return Ok(format!("phar://{ALIAS}"));
    }
    let directory = extract()?;
    directory.into_os_string().into_string().map_err(|path| {
        anyhow::anyhow!("the extracted Composer PHP runtime path {path:?} is not valid UTF-8")
    })
}

/// Whether the worker can read the bundle straight out of this executable. It cannot when its
/// PHP has no phar extension, no zlib to inflate the entries, or a restriction on the phar stream
/// wrapper.
fn worker_opens_bundle() -> anyhow::Result<bool> {
    let executable = std::env::current_exe()?;
    let executable = executable.to_str().ok_or_else(|| {
        anyhow::anyhow!("the path of this executable, {executable:?}, is not valid UTF-8")
    })?;
    let opened = call_function(
        "__shirabe_open_runtime_bundle",
        vec![
            PluginValue::string(executable),
            PluginValue::string(ALIAS),
            PluginValue::string(SENTINEL_PATH),
            PluginValue::string(BUNDLE_ID),
        ],
    )?
    .map_err(|throw| anyhow::anyhow!("{throw}"))?;
    match opened {
        PluginValue::Bool(opened) => Ok(opened),
        other => Err(anyhow::anyhow!(
            "opening the Composer PHP runtime bundle did not answer with a bool: {other:?}"
        )),
    }
}

/// Unpacks the bundle into a content-addressed directory, so that a worker that cannot read the
/// bundle in place gets the same files from the filesystem.
fn extract() -> anyhow::Result<std::path::PathBuf> {
    extract_into(&cache_directory()?.join("shirabe").join("runtime"))
}

fn extract_into(root: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
    let destination = root.join(BUNDLE_ID);
    if destination.is_dir() {
        return Ok(destination);
    }

    std::fs::create_dir_all(root)?;
    let staging = tempfile::tempdir_in(root)?;
    let archive = staging.path().join("bundle.phar");
    std::fs::write(&archive, BUNDLE)?;
    let unpacked = staging.path().join("unpacked");
    shirabe_php_shim::Phar::new(&archive)?.extract_to(&unpacked, None, true)?;
    std::fs::remove_file(&archive)?;

    if let Err(error) = std::fs::rename(&unpacked, &destination) {
        // Losing the race against another process that unpacked the same bundle is not a
        // failure: the directory is named after the contents that went into it.
        if !destination.is_dir() {
            return Err(error.into());
        }
    }
    Ok(destination)
}

fn cache_directory() -> anyhow::Result<std::path::PathBuf> {
    if let Some(directory) = std::env::var_os("XDG_CACHE_HOME")
        && !directory.is_empty()
    {
        return Ok(std::path::PathBuf::from(directory));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| anyhow::anyhow!("neither XDG_CACHE_HOME nor HOME is set"))?;
    Ok(std::path::Path::new(&home).join(".cache"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shirabe_symfony_process::PhpExecutableFinder;

    fn sentinel_of(directory: &std::path::Path) -> String {
        std::fs::read_to_string(directory.join(SENTINEL_PATH)).expect("no sentinel in the bundle")
    }

    /// PHP takes the first occurrence of the stub token in the file it opens, so no other copy of
    /// it may precede the bundle in the executable.
    #[test]
    fn the_first_phar_in_this_executable_is_the_bundle() {
        let executable = std::env::current_exe().expect("no path for this executable");
        let unpacked = tempfile::tempdir().unwrap();
        shirabe_php_shim::Phar::new(&executable)
            .expect("this executable does not read back as a phar")
            .extract_to(unpacked.path(), None, true)
            .unwrap();

        assert_eq!(sentinel_of(unpacked.path()), BUNDLE_ID);
    }

    #[test]
    fn extracting_the_bundle_names_the_directory_after_it() {
        let root = tempfile::tempdir().unwrap();
        let directory = extract_into(root.path()).unwrap();

        assert_eq!(directory, root.path().join(BUNDLE_ID));
        assert_eq!(sentinel_of(&directory), BUNDLE_ID);
        assert!(directory.join("vendor/autoload.php").is_file());
        // A second call finds the unpacked bundle and leaves it alone.
        assert_eq!(extract_into(root.path()).unwrap(), directory);
    }

    #[test]
    fn the_worker_reads_the_bundle_out_of_this_executable() {
        if PhpExecutableFinder::new().find(false).is_none() {
            // No PHP in this environment; the worker cannot start.
            return;
        }

        assert_eq!(base_path().unwrap(), format!("phar://{ALIAS}"));
    }

    /// The other half of `base_path`: a worker whose PHP cannot open the bundle is handed the
    /// unpacked tree, and has to reach the same classes through it.
    #[test]
    fn the_worker_loads_the_composer_runtime_from_an_unpacked_bundle() {
        if PhpExecutableFinder::new().find(false).is_none() {
            // No PHP in this environment; the worker cannot start.
            return;
        }

        let root = tempfile::tempdir().unwrap();
        let autoload = extract_into(root.path())
            .unwrap()
            .join("vendor/autoload.php");
        call_function(
            "__shirabe_require",
            vec![PluginValue::string(autoload.to_str().unwrap())],
        )
        .unwrap()
        .unwrap();

        // A class with no proxy stub, so that the answer is about the runtime and not about the
        // stub autoloader.
        let exists = call_function(
            "class_exists",
            vec![PluginValue::string(r"Composer\Util\Filesystem")],
        )
        .unwrap()
        .unwrap();
        assert_eq!(exists, PluginValue::Bool(true));
    }
}
