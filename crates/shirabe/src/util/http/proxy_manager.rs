//! ref: composer/src/Composer/Util/Http/ProxyManager.php

use crate::downloader::TransportException;
use crate::util::NoProxyPattern;
use crate::util::http::ProxyItem;
use crate::util::http::RequestProxy;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

static INSTANCE: OnceLock<Mutex<Option<ProxyManager>>> = OnceLock::new();

// Distinguishes ProxyManager instances so tests can mirror PHP `===` identity of the singleton,
// which the Rust value-based singleton does not otherwise expose.
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct ProxyManager {
    error: Option<String>,
    http_proxy: Option<ProxyItem>,
    https_proxy: Option<ProxyItem>,
    no_proxy_handler: std::cell::RefCell<Option<NoProxyPattern>>,
    generation: u64,
}

impl ProxyManager {
    fn new() -> Self {
        let mut instance = Self {
            error: None,
            http_proxy: None,
            https_proxy: None,
            no_proxy_handler: std::cell::RefCell::new(None),
            generation: NEXT_GENERATION.fetch_add(1, Ordering::Relaxed),
        };
        if let Err(e) = instance.get_proxy_data() {
            instance.error = Some(e.to_string());
        }
        instance
    }

    pub fn get_instance() -> &'static Mutex<Option<ProxyManager>> {
        INSTANCE.get_or_init(|| Mutex::new(Some(ProxyManager::new())))
    }

    pub fn reset() {
        if let Some(mutex) = INSTANCE.get() {
            *mutex.lock().unwrap() = Some(ProxyManager::new());
        }
    }

    /// For testing only: a unique id per constructed instance, used to mirror PHP `===` identity
    /// comparison of the ProxyManager singleton across `get_instance`/`reset`.
    pub fn __generation(&self) -> u64 {
        self.generation
    }

    pub fn has_proxy(&self) -> bool {
        self.http_proxy.is_some() || self.https_proxy.is_some()
    }

    pub fn get_proxy_for_request(
        &self,
        request_url: &str,
    ) -> Result<RequestProxy, TransportException> {
        if let Some(ref error) = self.error {
            return Err(TransportException::new(
                format!("Unable to use a proxy: {}", error),
                0,
            ));
        }

        let scheme = request_url.split("://").next().unwrap_or("").to_string();
        let proxy = self.get_proxy_for_scheme(&scheme);

        if proxy.is_none() {
            return Ok(RequestProxy::none());
        }

        if self.no_proxy(request_url) {
            return Ok(RequestProxy::no_proxy());
        }

        Ok(proxy.unwrap().to_request_proxy(scheme))
    }

    fn get_proxy_for_scheme(&self, scheme: &str) -> Option<&ProxyItem> {
        if scheme == "http" {
            return self.http_proxy.as_ref();
        }
        if scheme == "https" {
            return self.https_proxy.as_ref();
        }
        None
    }

    fn get_proxy_data(&mut self) -> anyhow::Result<()> {
        // handle http_proxy/HTTP_PROXY on CLI only for security reasons
        // PHP_SAPI is always 'cli' for this application
        let (env, name) = Self::get_proxy_env("http_proxy");
        if let Some(env) = env {
            self.http_proxy = Some(ProxyItem::new(env, name)?);
        }

        if self.http_proxy.is_none() {
            let (env, name) = Self::get_proxy_env("cgi_http_proxy");
            if let Some(env) = env {
                self.http_proxy = Some(ProxyItem::new(env, name)?);
            }
        }

        let (env, name) = Self::get_proxy_env("https_proxy");
        if let Some(env) = env {
            self.https_proxy = Some(ProxyItem::new(env, name)?);
        }

        let (env, _name) = Self::get_proxy_env("no_proxy");
        if let Some(env) = env {
            *self.no_proxy_handler.borrow_mut() = Some(NoProxyPattern::new(&env));
        }

        Ok(())
    }

    fn get_proxy_env(env_name: &str) -> (Option<String>, String) {
        for name in [env_name.to_lowercase(), env_name.to_uppercase()] {
            if let Ok(val) = std::env::var(&name)
                && !val.is_empty()
            {
                return (Some(val), name);
            }
        }
        (None, String::new())
    }

    fn no_proxy(&self, request_url: &str) -> bool {
        match self.no_proxy_handler.borrow_mut().as_mut() {
            None => false,
            Some(handler) => handler.test(request_url).unwrap_or(false),
        }
    }
}
