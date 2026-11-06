//! Reads `file://` URLs and plain local paths for `HttpDownloader`, which Composer hands to
//! `RemoteFilesystem` along with every other request curl does not take. It reproduces what
//! `RemoteFilesystem::get()` does for a local read without the PHP worker. The parts of that
//! method only an HTTP response reaches (status codes, redirects, authentication retries,
//! content decoding) have nothing to act on here: a local read yields no response headers.

use crate::config::Config;
use crate::downloader::{MaxFileSizeExceededException, TransportException};
use crate::io::{IOInterface, IOInterfaceWrite};
use crate::util::http::ProxyManager;
use crate::util::{GetResult, StreamContextOptions, Url};
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    RuntimeException, bconcat, file_get_contents, file_get_contents_with_max_length,
    file_put_contents, parse_url, path_from_bytes, php_regex, php_to_string, preg_is_match, strpos,
    strtr,
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
    pub fn handles(url: impl AsRef<[u8]>) -> bool {
        let scheme = parse_url(strtr(url, "\\", "/"))
            .and_then(|parsed| parsed.scheme)
            .unwrap_or_default();
        if scheme.is_empty() || scheme.eq_ignore_ascii_case(b"file") {
            return true;
        }

        // `parse_url` reads the drive letter of an absolute Windows path as a scheme.
        crate::util::Platform::is_windows() && scheme.len() == 1
    }

    pub fn copy(
        &self,
        origin_url: impl AsRef<[u8]>,
        file_url: impl AsRef<[u8]>,
        file_name: impl AsRef<std::path::Path>,
        progress: bool,
        options: StreamContextOptions,
    ) -> anyhow::Result<GetResult> {
        self.get(
            origin_url.as_ref().as_bstr(),
            file_url.as_ref().as_bstr(),
            options,
            Some(file_name.as_ref()),
            progress,
        )
    }

    pub fn get_contents(
        &self,
        origin_url: impl AsRef<[u8]>,
        file_url: impl AsRef<[u8]>,
        progress: bool,
        options: StreamContextOptions,
    ) -> anyhow::Result<GetResult> {
        self.get(
            origin_url.as_ref().as_bstr(),
            file_url.as_ref().as_bstr(),
            options,
            None,
            progress,
        )
    }

    fn get(
        &self,
        origin_url: &BStr,
        file_url: &BStr,
        mut options: StreamContextOptions,
        file_name: Option<&std::path::Path>,
        progress: bool,
    ) -> anyhow::Result<GetResult> {
        if options.isset("retry-auth-failure") {
            options.remove("retry-auth-failure");
        }
        let is_redirect = options.isset("redirects");
        if is_redirect {
            options.remove("redirects");
        }

        let orig_file_url = file_url.to_owned();
        let mut file_url = file_url.to_owned();

        if options.isset("prevent_ip_access_callable") {
            return Err(RuntimeException::new(BString::from(
                "RemoteFilesystem doesn't support the 'prevent_ip_access_callable' config.",
            ))
            .into());
        }

        if options.isset("gitlab-token")
            && let Some(token) = options.remove("gitlab-token")
        {
            let separator = if strpos(&file_url, "?").is_none() {
                "?"
            } else {
                "&"
            };
            file_url = bconcat!(file_url, separator, "access_token=", php_to_string(&token));
        }

        let max_file_size = options.max_file_size()?;

        let using_proxy = {
            let proxy_manager_guard = ProxyManager::get_instance();
            let proxy = proxy_manager_guard
                .as_ref()
                .expect("ProxyManager instance")
                .get_proxy_for_request(&file_url)?;
            proxy
                .get_status(Some(BStr::new(" using proxy (%s)")))
                .unwrap_or_default()
        };
        self.io.write_error3(
            bconcat!(
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
                file_url.as_ref(),
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
        let error_message = BString::default();
        let result = match max_file_size {
            Some(max) => {
                file_get_contents_with_max_length(path_from_bytes(&file_url), max as usize)
            }
            None => file_get_contents(path_from_bytes(&file_url)),
        }
        .ok();

        if let Some(ref r) = result
            && let Some(max) = max_file_size
            && r.len() as i64 >= max
        {
            return Err(MaxFileSizeExceededException::new(bconcat!(
                "Maximum allowed download size reached. Downloaded ",
                r.len(),
                " of allowed ",
                max,
                " bytes"
            ))
            .into());
        }

        if progress && !is_redirect {
            self.io.overwrite_error4(
                bconcat!(
                    "Downloading (",
                    if result.is_none() {
                        "<error>failed</error>"
                    } else {
                        "<comment>100%</comment>"
                    },
                    ")"
                ),
                false,
                None,
                crate::io::NORMAL,
            );
        }

        let Some(result) = result else {
            return Err(TransportException::new(
                bconcat!(
                    "The \"",
                    orig_file_url,
                    "\" file could not be downloaded: ",
                    error_message
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
                    bconcat!(
                        "\"",
                        orig_file_url,
                        "\" appears broken, and returned an empty 200 response"
                    ),
                    0,
                )
                .into());
            }

            // TODO(php-semantics): PHP captures the file_put_contents warning via set_error_handler
            // and appends it to the message below; the write failure's reason is not carried yet.
            if file_put_contents(file_name, &result).is_none() {
                return Err(TransportException::new(
                    bconcat!(
                        "The \"",
                        orig_file_url,
                        "\" file could not be written to ",
                        file_name,
                        ": "
                    ),
                    0,
                )
                .into());
            }

            return Ok(GetResult::True);
        }

        Ok(GetResult::Content(result))
    }
}
