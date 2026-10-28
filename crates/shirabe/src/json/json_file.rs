//! ref: composer/src/Composer/Json/JsonFile.php

use crate::downloader::TransportException;
use crate::io::{IOInterface, IOInterfaceImmutable, io_interface};
use crate::json::JsonValidationException;
use crate::util::{Filesystem, HttpDownloader};
use shirabe_php_shim::{
    Catch as _, Error, InvalidArgumentException, JSON_PRETTY_PRINT, JSON_UNESCAPED_SLASHES,
    JSON_UNESCAPED_UNICODE, PhpMixed, PregMatches, RuntimeException, UnexpectedValueException,
    dirname, file_exists, file_get_contents, file_put_contents, is_dir, is_file, json_decode_assoc,
    json_decode_obj, json_encode_ex, legacy_lossy, mkdir, path_from_bytes, path_to_bytes,
    php_regex, preg_is_match, preg_match, preg_replace_callback, preg_replace2, realpath,
    str_repeat, strlen, strpos, usleep,
};
use shirabe_seld_json_lint::{ParsingException, ParsingExceptionDetails};

#[derive(Debug, Clone)]
pub struct JsonEncodeOptions {
    pub unescaped_slashes: bool,
    pub pretty_print: bool,
    pub unescaped_unicode: bool,
    pub indent: String,
}

impl Default for JsonEncodeOptions {
    fn default() -> Self {
        Self {
            unescaped_slashes: true,
            pretty_print: true,
            unescaped_unicode: true,
            indent: JsonFile::INDENT_DEFAULT.to_string(),
        }
    }
}

impl JsonEncodeOptions {
    pub fn none() -> Self {
        Self {
            unescaped_slashes: false,
            pretty_print: false,
            unescaped_unicode: false,
            indent: JsonFile::INDENT_DEFAULT.to_string(),
        }
    }

    fn to_flags(&self) -> i64 {
        let mut flags = 0;
        if self.unescaped_slashes {
            flags |= JSON_UNESCAPED_SLASHES;
        }
        if self.pretty_print {
            flags |= JSON_PRETTY_PRINT;
        }
        if self.unescaped_unicode {
            flags |= JSON_UNESCAPED_UNICODE;
        }
        flags
    }
}

/// Reads/writes json files.
#[derive(Debug)]
pub struct JsonFile {
    path: String,
    http_downloader: Option<std::rc::Rc<std::cell::RefCell<HttpDownloader>>>,
    io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
    indent: std::cell::RefCell<String>,
}

impl JsonFile {
    pub const LAX_SCHEMA: i64 = 1;
    pub const STRICT_SCHEMA: i64 = 2;
    pub const AUTH_SCHEMA: i64 = 3;
    pub const LOCK_SCHEMA: i64 = 4;

    pub const INDENT_DEFAULT: &'static str = "    ";

    /// PHP points these at Composer's res/ directory; here the schemas are embedded in the
    /// binary and SchemaRetriever resolves these URIs back to the constants below. The res/
    /// path segment matters: composer-lock-schema.json reaches the composer schema through the
    /// relative reference "./composer-schema.json", which must resolve to COMPOSER_SCHEMA_PATH.
    pub const COMPOSER_SCHEMA_PATH: &'static str = "shirabe:///res/composer-schema.json";
    pub const LOCK_SCHEMA_PATH: &'static str = "shirabe:///res/composer-lock-schema.json";

    pub const COMPOSER_SCHEMA_JSON: &'static str =
        include_str!("../../../../composer/res/composer-schema.json");
    const LOCK_SCHEMA_JSON: &'static str =
        include_str!("../../../../composer/res/composer-lock-schema.json");

    /// Initializes json file reader/parser.
    pub fn new(
        path: String,
        http_downloader: Option<std::rc::Rc<std::cell::RefCell<HttpDownloader>>>,
        io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
    ) -> anyhow::Result<Self> {
        if http_downloader.is_none() && preg_is_match(php_regex!(r"{^https?://}i"), &path) {
            return Err(InvalidArgumentException::new(
                "http urls require a HttpDownloader instance to be passed".to_string(),
            )
            .into());
        }
        Ok(Self {
            path,
            http_downloader,
            io,
            indent: std::cell::RefCell::new(Self::INDENT_DEFAULT.to_string()),
        })
    }

    pub fn get_path(&self) -> &str {
        &self.path
    }

    /// Checks whether json file exists.
    pub fn exists(&self) -> bool {
        is_file(&self.path)
    }

    /// Reads json file.
    pub fn read(&self) -> anyhow::Result<PhpMixed> {
        let json: Option<Vec<u8>> = match (|| -> anyhow::Result<Option<Vec<u8>>> {
            if let Some(http_downloader) = &self.http_downloader {
                Ok(http_downloader
                    .borrow_mut()
                    .get(&self.path, indexmap::IndexMap::new())?
                    .get_body()
                    .map(|s| s.as_bytes().to_vec()))
            } else {
                if !Filesystem::is_readable(&self.path) {
                    return Err(RuntimeException::new(format!(
                        "The file \"{}\" is not readable.",
                        self.path
                    ))
                    .into());
                }
                if let Some(io) = &self.io
                    && io.is_debug()
                {
                    let mut realpath_info = String::new();
                    if let Some(realpath) =
                        realpath(&self.path).map(|p| legacy_lossy(path_to_bytes(&p)).into_owned())
                        && realpath != self.path
                    {
                        realpath_info = format!(" ({})", realpath);
                    }
                    io.write_error3(
                        &format!("Reading {}{}", self.path, realpath_info),
                        true,
                        io_interface::NORMAL,
                    );
                }
                Ok(file_get_contents(&self.path).ok())
            }
        })() {
            Ok(j) => j,
            Err(e) => {
                if let Some(te) = e.catch::<TransportException>() {
                    let message = te.get_message().to_string();
                    return Err(RuntimeException::with_code_and_previous(
                        message,
                        0,
                        Some(std::sync::Arc::new(e)),
                    )
                    .into());
                }
                // `catch (\Exception)` leaves an \Error to propagate.
                if e.is_instanceof::<Error>() {
                    return Err(e);
                }
                return Err(RuntimeException::new(format!(
                    "Could not read {}\n\n{}",
                    self.path, e
                ))
                .into());
            }
        };

        let json = match json {
            Some(j) => j,
            None => {
                return Err(RuntimeException::new(format!("Could not read {}", self.path)).into());
            }
        };

        // The indent is a run of spaces and tabs at a line start, which a lossy decode leaves intact.
        *self.indent.borrow_mut() = Self::detect_indenting(Some(&String::from_utf8_lossy(&json)));

        Self::parse_json(Some(&json), Some(&self.path))
    }

    pub fn write(&self, hash: PhpMixed) -> anyhow::Result<()> {
        self.write_with_options(hash, JsonEncodeOptions::default())
    }

    pub fn write_with_options(
        &self,
        hash: PhpMixed,
        options: JsonEncodeOptions,
    ) -> anyhow::Result<()> {
        let options = JsonEncodeOptions {
            indent: self.indent.borrow().clone(),
            ..options
        };

        if self.path == "php://memory" {
            file_put_contents(
                &self.path,
                Self::encode_with_options(&hash, options)?.as_bytes(),
            );

            return Ok(());
        }

        let dir = dirname(&self.path);
        if !is_dir(path_from_bytes(&dir)) {
            if file_exists(path_from_bytes(&dir)) {
                return Err(UnexpectedValueException::new(format!(
                    "{} exists and is not a directory.",
                    realpath(path_from_bytes(&dir))
                        .map(|p| legacy_lossy(path_to_bytes(&p)).into_owned())
                        .unwrap_or_default(),
                ))
                .into());
            }
            if mkdir(path_from_bytes(&dir), 0o777, true).is_err() {
                return Err(UnexpectedValueException::new(format!(
                    "{} does not exist and could not be created.",
                    dir
                ))
                .into());
            }
        }

        let mut retries = 3;
        while retries > 0 {
            retries -= 1;
            let attempt: anyhow::Result<()> = (|| -> anyhow::Result<()> {
                self.file_put_contents_if_modified(
                    &self.path,
                    &format!(
                        "{}{}",
                        Self::encode_with_options(&hash, options.clone())?,
                        if options.pretty_print { "\n" } else { "" },
                    ),
                )?;
                Ok(())
            })();
            match attempt {
                Ok(_) => break,
                Err(e) => {
                    if retries > 0 {
                        usleep(500_000);
                        continue;
                    }

                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Modify file properties only if content modified
    fn file_put_contents_if_modified(
        &self,
        path: &str,
        content: &str,
    ) -> anyhow::Result<Option<i64>> {
        let current_content = file_get_contents(path).ok();
        if current_content.is_none() || current_content.as_deref() != Some(content.as_bytes()) {
            return Ok(file_put_contents(path, content.as_bytes()));
        }

        Ok(Some(0))
    }

    /// Validates the schema of the current json file according to composer-schema.json rules
    pub fn validate_schema(&self, schema: i64, schema_file: Option<&str>) -> anyhow::Result<bool> {
        if !Filesystem::is_readable(&self.path) {
            return Err(RuntimeException::new(format!(
                "The file \"{}\" is not readable.",
                self.path
            ))
            .into());
        }
        let content = file_get_contents(&self.path).unwrap_or_default();
        let data = json_decode_obj(&content)?;

        if matches!(data, PhpMixed::Null) && content != b"null" {
            Self::validate_syntax(&content, Some(&self.path))?;
        }

        Self::validate_json_schema(&self.path, &data, schema, schema_file)
    }

    /// Validates the schema of the current json file according to composer-schema.json rules
    pub fn validate_json_schema(
        source: &str,
        data: &PhpMixed,
        schema: i64,
        schema_file: Option<&str>,
    ) -> anyhow::Result<bool> {
        let mut is_composer_schema_file = false;
        let mut schema_file = match schema_file {
            Some(f) => f.to_string(),
            None => {
                if schema == Self::LOCK_SCHEMA {
                    Self::LOCK_SCHEMA_PATH.to_string()
                } else {
                    is_composer_schema_file = true;
                    Self::COMPOSER_SCHEMA_PATH.to_string()
                }
            }
        };

        // Prepend with file:// only when not using a special schema already (e.g. in the phar)
        if strpos(&schema_file, "://").is_none() {
            schema_file = format!("file://{}", schema_file);
        }

        // PHP: $schemaData = (object) ['$ref' => $schemaFile, '$schema' => "https://json-schema.org/draft-04/schema#"];
        // A string-keyed `PhpMixed::Array` serializes as a JSON object, matching the (object) cast.
        let mut schema_data: PhpMixed = {
            let mut m = indexmap::IndexMap::new();
            m.insert("$ref".into(), PhpMixed::string(schema_file.clone()));
            m.insert(
                "$schema".into(),
                PhpMixed::string("https://json-schema.org/draft-04/schema#".to_string()),
            );
            PhpMixed::Array(m)
        };

        if schema == Self::STRICT_SCHEMA && is_composer_schema_file {
            schema_data = json_decode_obj(Self::COMPOSER_SCHEMA_JSON)?;
            if let PhpMixed::Object(map) = &mut schema_data {
                map.insert("additionalProperties".into(), PhpMixed::Bool(false));
                map.insert(
                    "required".into(),
                    PhpMixed::List(vec![
                        PhpMixed::string("name".to_string()),
                        PhpMixed::string("description".to_string()),
                    ]),
                );
            }
        } else if schema == Self::AUTH_SCHEMA && is_composer_schema_file {
            let mut m = indexmap::IndexMap::new();
            m.insert(
                "$ref".into(),
                PhpMixed::string(format!("{}#/properties/config", schema_file)),
            );
            m.insert(
                "$schema".into(),
                PhpMixed::string("https://json-schema.org/draft-04/schema#".to_string()),
            );
            schema_data = PhpMixed::Array(m);
        }

        // convert assoc arrays to objects
        let schema_value = serde_json::to_value(&schema_data)?;
        let data_value = serde_json::to_value(data)?;
        let validator = jsonschema::options()
            .with_retriever(SchemaRetriever)
            .build(&schema_value)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let errors: Vec<String> = validator
            .iter_errors(&data_value)
            .map(|error| {
                let mut property = error
                    .instance_path()
                    .as_str()
                    .trim_start_matches('/')
                    .replace('/', ".");
                // A missing required property is reported against its parent object, so the
                // instance path is the parent (empty at the root). Composer points the error at
                // the missing property itself, so append its name to match the `PROPERTY : MESSAGE`
                // shape.
                if let jsonschema::error::ValidationErrorKind::Required { property: missing } =
                    error.kind()
                    && let Some(name) = missing.as_str()
                {
                    if property.is_empty() {
                        property = name.to_string();
                    } else {
                        property = format!("{}.{}", property, name);
                    }
                }
                if property.is_empty() {
                    error.to_string()
                } else {
                    format!("{} : {}", property, error)
                }
            })
            .collect();

        if !errors.is_empty() {
            return Err(JsonValidationException::new(
                format!("\"{}\" does not match the expected JSON schema", source),
                errors,
            )
            .into());
        }

        Ok(true)
    }

    pub fn encode<T: serde::Serialize + ?Sized>(data: &T) -> anyhow::Result<String> {
        Self::encode_with_options(data, JsonEncodeOptions::default())
    }

    pub fn encode_with_options<T: serde::Serialize + ?Sized>(
        data: &T,
        options: JsonEncodeOptions,
    ) -> anyhow::Result<String> {
        let json = json_encode_ex(data, options.to_flags())
            .map_err(|err| RuntimeException::new(format!("JSON encoding failed: {}", err)))?;

        if options.pretty_print && options.indent != Self::INDENT_DEFAULT {
            // Pretty printing and not using default indentation
            let indent_owned = options.indent;
            return Ok(legacy_lossy(
                &preg_replace_callback(
                    php_regex!(r"#^ {4,}#m"),
                    move |m: &PregMatches| -> anyhow::Result<String> {
                        let whole = m.get(0).unwrap_or_default();
                        Ok(
                            legacy_lossy(&str_repeat(&indent_owned, (strlen(whole) / 4) as usize))
                                .into_owned(),
                        )
                    },
                    &json,
                )
                .expect("the replacement callback cannot fail"),
            )
            .into_owned());
        }

        Ok(json)
    }

    /// Parses json string and returns hash.
    pub fn parse_json(
        json: Option<impl AsRef<[u8]>>,
        file: Option<&str>,
    ) -> anyhow::Result<PhpMixed> {
        let json = match &json {
            None => return Ok(PhpMixed::Null),
            Some(j) => j.as_ref(),
        };
        let mut data = json_decode_assoc(json)?;
        // PHP: `null === $data && JSON_ERROR_NONE !== json_last_error()`, i.e. the decode produced
        // null because of an error rather than because the input was the literal `null`.
        // json_decode_assoc swallows the error into PhpMixed::Null, so detect the failure by
        // comparing the source against `null`, mirroring validateSchema's own
        // `'null' !== $content` check.
        if matches!(data, PhpMixed::Null) && json != b"null" {
            // attempt resolving simple conflicts in lock files so that one can run `composer update --lock` and get a valid lock file
            // TODO(bytes): preg_replace2 matches over a &str, so input that is not UTF-8 skips
            // this; PHP still resolves a conflict whose marker lines hold the only invalid bytes.
            if let Some(file) = file
                && file.ends_with(".lock")
                && let Ok(json) = std::str::from_utf8(json)
                && json.contains("\"content-hash\"")
            {
                let mut count: usize = 0;
                let replaced = preg_replace2(
                    php_regex!(
                        r#"{\r?\n<<<<<<< [^\r\n]+\r?\n\s+"content-hash": *"[0-9a-f]+", *\r?\n(?:\|{7} [^\r\n]+\r?\n\s+"content-hash": *"[0-9a-f]+", *\r?\n)?=======\r?\n\s+"content-hash": *"[0-9a-f]+", *\r?\n>>>>>>> [^\r\n]+(\r?\n)}"#
                    ),
                    "    \"content-hash\": \"VCS merge conflict detected. Please run `shirabe update --lock`.\",$1",
                    json,
                    -1,
                    Some(&mut count),
                );
                if count == 1 {
                    data = json_decode_assoc(&replaced)?;
                    if !matches!(data, PhpMixed::Null) {
                        return Ok(data);
                    }
                }
            }

            Self::validate_syntax(json, file)?;
        }

        Ok(data)
    }

    /// Validates the syntax of a JSON string
    fn validate_syntax(json: &[u8], file: Option<&str>) -> anyhow::Result<bool> {
        // PHP's linter checks the syntax byte-wise and leaves the encoding to the json_decode()
        // that ran before. A lossy decode keeps the syntax and the line numbers intact, so it
        // stands in for the former, and the UTF-8 check below for the latter.
        let lossy = String::from_utf8_lossy(json);
        // TODO(php-semantics): make json_decode() returns an error object with details.
        let error = match serde_json::from_str::<serde_json::Value>(&lossy) {
            Ok(_) => {
                if std::str::from_utf8(json).is_err() {
                    return Err(UnexpectedValueException::new(match file {
                        None => "The input is not UTF-8, could not parse as JSON".to_string(),
                        Some(file) => format!("\"{file}\" is not UTF-8, could not parse as JSON"),
                    })
                    .into());
                }

                return Ok(true);
            }
            Err(e) => e,
        };

        let details = ParsingExceptionDetails {
            line: Some(error.line() as i64),
            ..Default::default()
        };
        Err(match file {
            None => ParsingException::new(
                format!("The input does not contain valid JSON\n{error}"),
                details,
            ),
            Some(file) => ParsingException::new(
                format!("\"{file}\" does not contain valid JSON\n{error}"),
                details,
            ),
        }
        .into())
    }

    pub fn detect_indenting(json: Option<&str>) -> String {
        if let Some(m) = preg_match(php_regex!(r##"#^([ \t]+)"#m"##), json.unwrap_or_default()) {
            return m.get(1).unwrap_or_default().to_string();
        }

        Self::INDENT_DEFAULT.to_string()
    }
}

#[derive(Debug)]
struct SchemaRetriever;

impl jsonschema::Retrieve for SchemaRetriever {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> anyhow::Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        match uri.scheme().as_str() {
            "shirabe" => {
                let contents = match uri.path().as_str() {
                    "/res/composer-schema.json" => JsonFile::COMPOSER_SCHEMA_JSON,
                    "/res/composer-lock-schema.json" => JsonFile::LOCK_SCHEMA_JSON,
                    path => return Err(format!("Unknown embedded resource {path}").into()),
                };
                Ok(serde_json::from_str(contents)?)
            }
            "file" => {
                let file = std::fs::File::open(uri.path().as_str())?;
                Ok(serde_json::from_reader(std::io::BufReader::new(file))?)
            }
            scheme => Err(format!("Unknown scheme {scheme}").into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected values are the output of PHP 8.5.9 running Composer's JsonFile::parseJson().
    #[test]
    fn input_that_is_not_utf8_is_rejected_as_php_rejects_it() {
        let err = JsonFile::parse_json(Some(b"{\"a\": \"\xff\"}"), Some("x.json")).unwrap_err();
        assert!(err.is_instanceof::<UnexpectedValueException>());
        assert_eq!(
            err.to_string(),
            "\"x.json\" is not UTF-8, could not parse as JSON"
        );

        let err = JsonFile::parse_json(Some(b"{\"a\": 1\xff}"), Some("x.json")).unwrap_err();
        assert!(err.is_instanceof::<ParsingException>());

        let err = JsonFile::parse_json(Some(b"\xff"), Some("x.json")).unwrap_err();
        assert!(err.is_instanceof::<ParsingException>());
    }
}
