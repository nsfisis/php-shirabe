//! Reads `file://` URLs and plain local paths for `HttpDownloader`, which Composer hands to
//! `RemoteFilesystem` along with every other request curl does not take. It reproduces what
//! `RemoteFilesystem::get()` does for a local read without the PHP worker. The parts of that
//! method only an HTTP response reaches (status codes, redirects, authentication retries,
//! content decoding) have nothing to act on here: a local read yields no response headers.

use crate::config::Config;
use crate::downloader::{MaxFileSizeExceededException, TransportException};
use crate::io::{IOInterface, IOInterfaceImmutable};
use crate::util::http::ProxyManager;
use crate::util::{GetResult, Url};
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, file_get_contents, file_get_contents_with_max_length,
    file_put_contents, parse_url, php_regex, preg_is_match, strpos, strtr,
};

#[derive(Debug)]
pub struct LocalFilesystem {
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
}

impl LocalFilesystem {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
    ) -> Self {
        Self { io, config }
    }

    /// Whether `url` names a local file: a `file://` URL, or a path with no scheme.
    pub fn handles(url: &str) -> bool {
        let scheme = parse_url(&strtr(url, "\\", "/"))
            .and_then(|parsed| parsed.scheme)
            .unwrap_or_default();
        if scheme.is_empty() || scheme.eq_ignore_ascii_case("file") {
            return true;
        }

        // `parse_url` reads the drive letter of an absolute Windows path as a scheme.
        crate::util::Platform::is_windows() && scheme.len() == 1
    }

    pub fn copy(
        &self,
        origin_url: &str,
        file_url: &str,
        file_name: &str,
        progress: bool,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<GetResult> {
        self.get(origin_url, file_url, options, Some(file_name), progress)
    }

    pub fn get_contents(
        &self,
        origin_url: &str,
        file_url: &str,
        progress: bool,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<GetResult> {
        self.get(origin_url, file_url, options, None, progress)
    }

    fn get(
        &self,
        origin_url: &str,
        file_url: &str,
        mut options: IndexMap<String, PhpMixed>,
        file_name: Option<&str>,
        progress: bool,
    ) -> anyhow::Result<GetResult> {
        options.shift_remove("retry-auth-failure");
        let is_redirect = options.shift_remove("redirects").is_some();

        let orig_file_url = file_url.to_string();
        let mut file_url = file_url.to_string();

        if options.contains_key("prevent_ip_access_callable") {
            return Err(RuntimeException::new(
                "RemoteFilesystem doesn't support the 'prevent_ip_access_callable' config."
                    .to_string(),
            )
            .into());
        }

        if let Some(token) = options.shift_remove("gitlab-token") {
            let separator = if strpos(&file_url, "?").is_none() {
                "?"
            } else {
                "&"
            };
            file_url = format!(
                "{}{}access_token={}",
                file_url,
                separator,
                token.as_string().unwrap_or_default()
            );
        }

        let max_file_size = options
            .shift_remove("max_file_size")
            .and_then(|v| v.as_int());

        let using_proxy = {
            let proxy_manager_guard = ProxyManager::get_instance();
            let proxy = proxy_manager_guard
                .as_ref()
                .expect("ProxyManager instance")
                .get_proxy_for_request(&file_url)
                .map_err(|e| anyhow::anyhow!(e))?;
            proxy
                .get_status(Some(" using proxy (%s)"))
                .unwrap_or_default()
        };
        self.io.write_error3(
            &format!(
                "{}{}{}",
                if strpos(&orig_file_url, "http") == Some(0) {
                    "Downloading "
                } else {
                    "Reading "
                },
                Url::sanitize(orig_file_url.clone()),
                using_proxy
            ),
            true,
            crate::io::DEBUG,
        );

        if !preg_is_match(
            php_regex!("{^http://(repo\\.)?packagist\\.org/p/}"),
            &file_url,
        ) || (strpos(&file_url, "$").is_none() && strpos(&file_url, "%24").is_none())
        {
            self.config.borrow_mut().prohibit_url_by_config(
                &file_url,
                Some(self.io.clone()),
                &IndexMap::new(),
            )?;
        }

        if progress && !is_redirect {
            self.io.write_error3(
                "Downloading (<comment>connecting...</comment>)",
                false,
                crate::io::NORMAL,
            );
        }

        // TODO(php-semantics): PHP captures the file_get_contents warning via set_error_handler and
        // appends it to the "could not be downloaded" message below; the read failure's reason is
        // not carried into error_message yet.
        let error_message = String::new();
        let result = match max_file_size {
            Some(max) => file_get_contents_with_max_length(&file_url, max as usize),
            None => file_get_contents(&file_url),
        }
        .ok();

        if let Some(ref r) = result
            && let Some(max) = max_file_size
            && r.len() as i64 >= max
        {
            return Err(MaxFileSizeExceededException::new(format!(
                "Maximum allowed download size reached. Downloaded {} of allowed {} bytes",
                r.len(),
                max
            ))
            .into());
        }

        if progress && !is_redirect {
            self.io.overwrite_error4(
                &format!(
                    "Downloading ({})",
                    if result.is_none() {
                        "<error>failed</error>"
                    } else {
                        "<comment>100%</comment>"
                    }
                ),
                false,
                None,
                crate::io::NORMAL,
            );
        }

        let Some(result) = result else {
            return Err(TransportException::new(
                format!(
                    "The \"{}\" file could not be downloaded: {}",
                    orig_file_url, error_message
                ),
                0,
            )
            .into());
        };

        if let Some(file_name) = file_name
            && !is_redirect
        {
            if result.is_empty() {
                return Err(TransportException::new(
                    format!(
                        "\"{}\" appears broken, and returned an empty 200 response",
                        orig_file_url
                    ),
                    0,
                )
                .into());
            }

            // TODO(php-semantics): PHP captures the file_put_contents warning via set_error_handler
            // and appends it to the message below; the write failure's reason is not carried yet.
            if file_put_contents(file_name, &result).is_none() {
                return Err(TransportException::new(
                    format!(
                        "The \"{}\" file could not be written to {}: ",
                        orig_file_url, file_name
                    ),
                    0,
                )
                .into());
            }

            return Ok(GetResult::True);
        }

        // TODO(bytes): GetResult carries the body as a String; from_utf8_lossy corrupts binary
        // payloads.
        Ok(GetResult::Content(
            String::from_utf8_lossy(&result).into_owned(),
        ))
    }
}
