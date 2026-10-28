//! ref: composer/src/Composer/Util/RemoteFilesystem.php
//!
//! The real `Composer\Util\RemoteFilesystem`, running in the PHP worker: every method forwards
//! to an instance there. `HttpDownloader` reaches for it last, for the requests neither
//! `NativeHttpDownloader` nor `LocalFilesystem` takes, which only PHP's stream layer can serve
//! (the stream wrappers of other schemes, including the ones plugins register). The instance is
//! built on first use, so a run that never needs it never spawns the worker.

use crate::config::Config;
use crate::io::IOInterface;
use crate::plugin::php_plugin_proxy::{PluginRpcDispatcher, config_handle_value, io_handle_value};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpObjHandle, PhpThrow, PluginValue};
use shirabe_php_shim::{PhpMixed, RuntimeException, legacy_lossy, php_regex, preg_match};

const CLASS: &str = "Composer\\Util\\RemoteFilesystem";

/// Result of `RemoteFilesystem::get` — string content, `true` (for copy), or `false`.
#[derive(Debug, Clone)]
pub enum GetResult {
    False,
    True,
    Content(String),
}

#[derive(Debug)]
pub struct RemoteFilesystem {
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    options: IndexMap<BString, PhpMixed>,
    disable_tls: bool,
    entity: std::cell::OnceCell<PhpObjHandle>,
}

impl RemoteFilesystem {
    /// TODO(plugin): PHP's trailing `?AuthHelper $authHelper` parameter is not offered: a Rust
    /// `AuthHelper` has no wire form, so the PHP instance always builds its own.
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        options: IndexMap<BString, PhpMixed>,
        disable_tls: bool,
    ) -> Self {
        Self {
            io,
            config,
            options,
            disable_tls,
            entity: std::cell::OnceCell::new(),
        }
    }

    pub fn copy(
        &self,
        origin_url: &str,
        file_url: &str,
        file_name: &str,
        progress: bool,
        options: IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<GetResult> {
        let value = self.call(
            "copy",
            vec![
                PluginValue::string(origin_url),
                PluginValue::string(file_url),
                PluginValue::string(file_name),
                PluginValue::Bool(progress),
                PluginValue::from_php_mixed(&PhpMixed::Array(options)),
            ],
        )?;
        Self::get_result(value)
    }

    pub fn get_contents(
        &self,
        origin_url: &str,
        file_url: &str,
        progress: bool,
        options: IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<GetResult> {
        let value = self.call(
            "getContents",
            vec![
                PluginValue::string(origin_url),
                PluginValue::string(file_url),
                PluginValue::Bool(progress),
                PluginValue::from_php_mixed(&PhpMixed::Array(options)),
            ],
        )?;
        Self::get_result(value)
    }

    pub fn get_options(&self) -> anyhow::Result<IndexMap<BString, PhpMixed>> {
        match self.call("getOptions", Vec::new())?.to_php_mixed()? {
            PhpMixed::Array(options) => Ok(options),
            PhpMixed::List(list) if list.is_empty() => Ok(IndexMap::new()),
            other => Err(Self::shape_error("getOptions", &other)),
        }
    }

    pub fn set_options(&self, options: IndexMap<BString, PhpMixed>) -> anyhow::Result<()> {
        self.call(
            "setOptions",
            vec![PluginValue::from_php_mixed(&PhpMixed::Array(options))],
        )?;
        Ok(())
    }

    pub fn is_tls_disabled(&self) -> anyhow::Result<bool> {
        match self.call("isTlsDisabled", Vec::new())? {
            PluginValue::Bool(disabled) => Ok(disabled),
            other => Err(Self::shape_error("isTlsDisabled", &other)),
        }
    }

    pub fn get_last_headers(&self) -> anyhow::Result<Vec<String>> {
        let value = self.call("getLastHeaders", Vec::new())?;
        Self::string_list("getLastHeaders", value)
    }

    /// Computed on this side: a static method has no instance to forward to, and the function is
    /// pure.
    pub fn find_status_code(headers: &[String]) -> Option<i64> {
        let mut value: Option<i64> = None;
        for header in headers {
            if let Some(m) = preg_match(php_regex!("{^HTTP/\\S+ (\\d+)}i"), header) {
                value = m
                    .get(1)
                    .and_then(|s| legacy_lossy(s).parse().ok())
                    .or(Some(0));
            }
        }

        value
    }

    pub fn find_status_message(&self, headers: &[String]) -> anyhow::Result<Option<String>> {
        let value = self.call(
            "findStatusMessage",
            vec![PluginValue::List(
                headers
                    .iter()
                    .map(|h| PluginValue::string(h.as_str()))
                    .collect(),
            )],
        )?;
        match value {
            PluginValue::Null => Ok(None),
            PluginValue::String(message) => Ok(Some(String::from_utf8(message.into())?)),
            other => Err(Self::shape_error("findStatusMessage", &other)),
        }
    }

    /// The worker-side instance, built on the first call.
    fn entity(&self) -> anyhow::Result<&PhpObjHandle> {
        if let Some(entity) = self.entity.get() {
            return Ok(entity);
        }

        let cache_dir = self.config.borrow().get_str("cache-dir")?;
        crate::event_dispatcher::EventDispatcher::ensure_composer_php_runtime(
            std::path::Path::new(&cache_dir),
        )?;
        let value = Self::php_result(shirabe_php_rpc::new_object(
            CLASS,
            vec![
                io_handle_value(&self.io)?,
                config_handle_value(&self.config),
                PluginValue::from_php_mixed(&PhpMixed::Array(self.options.clone())),
                PluginValue::Bool(self.disable_tls),
            ],
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        let PluginValue::PhpHandle(handle) = value else {
            return Err(Self::shape_error("__construct", &value));
        };

        Ok(self.entity.get_or_init(|| handle))
    }

    fn call(&self, method: &str, args: Vec<PluginValue>) -> anyhow::Result<PluginValue> {
        let entity = self.entity()?;
        Self::php_result(shirabe_php_rpc::call_php_method(
            entity.phandle,
            method,
            args,
            Some(&mut PluginRpcDispatcher::default()),
        ))
    }

    /// Carries an exception the PHP instance throws over as the ported class, so a caller
    /// catching `TransportException` catches it as it would under Composer.
    fn php_result(
        outcome: anyhow::Result<Result<PluginValue, PhpThrow>>,
    ) -> anyhow::Result<PluginValue> {
        match outcome? {
            Ok(value) => Ok(value),
            Err(throw) => Err(crate::plugin::throwable_bridge::throw_into_error(throw)),
        }
    }

    fn get_result(value: PluginValue) -> anyhow::Result<GetResult> {
        match value {
            PluginValue::Bool(true) => Ok(GetResult::True),
            PluginValue::Bool(false) => Ok(GetResult::False),
            // TODO(bytes): GetResult carries the body as a String; from_utf8_lossy corrupts binary
            // payloads.
            PluginValue::String(content) => Ok(GetResult::Content(
                String::from_utf8_lossy(&content).into_owned(),
            )),
            other => Err(Self::shape_error("get", &other)),
        }
    }

    fn string_list(method: &str, value: PluginValue) -> anyhow::Result<Vec<String>> {
        let PluginValue::List(list) = value else {
            return match value {
                PluginValue::Array(ref map) if map.is_empty() => Ok(Vec::new()),
                other => Err(Self::shape_error(method, &other)),
            };
        };
        list.into_iter()
            .map(|item| match item {
                PluginValue::String(s) => Ok(String::from_utf8(s.into())?),
                other => Err(Self::shape_error(method, &other)),
            })
            .collect()
    }

    fn shape_error(method: &str, value: &dyn std::fmt::Debug) -> anyhow::Error {
        RuntimeException::new(format!(
            "{CLASS}::{method} answered with an unsupported shape over RPC: {value:?}"
        ))
        .into()
    }

    /// For testing only. Invokes the protected `getOptionsForUrl` of the PHP instance, mirroring
    /// the `ReflectionMethod` access in `RemoteFilesystemTest::callGetOptionsForUrl`.
    pub fn __get_options_for_url(
        &self,
        origin_url: &str,
        additional_options: IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<IndexMap<BString, PhpMixed>> {
        let value = self.reflection_invoke(
            "getOptionsForUrl",
            vec![
                PluginValue::string(origin_url),
                PluginValue::from_php_mixed(&PhpMixed::Array(additional_options)),
            ],
        )?;
        match value.to_php_mixed()? {
            PhpMixed::Array(options) => Ok(options),
            other => Err(Self::shape_error("getOptionsForUrl", &other)),
        }
    }

    /// For testing only. Sets the private `fileUrl` of the PHP instance, mirroring the
    /// `ReflectionProperty` access in `RemoteFilesystemTest::callGetOptionsForUrl`.
    pub fn __set_file_url(&self, file_url: &str) -> anyhow::Result<()> {
        self.reflection_set_property("fileUrl", PluginValue::string(file_url))
    }

    /// For testing only. Invokes the protected `callbackGet` of the PHP instance, mirroring the
    /// `ReflectionMethod` access in `RemoteFilesystemTest::callCallbackGet`.
    pub fn __callback_get(
        &self,
        notification_code: i64,
        severity: i64,
        message: Option<String>,
        message_code: i64,
        bytes_transferred: i64,
        bytes_max: i64,
    ) -> anyhow::Result<()> {
        self.reflection_invoke(
            "callbackGet",
            vec![
                PluginValue::Int(notification_code),
                PluginValue::Int(severity),
                match message {
                    Some(message) => PluginValue::string(message),
                    None => PluginValue::Null,
                },
                PluginValue::Int(message_code),
                PluginValue::Int(bytes_transferred),
                PluginValue::Int(bytes_max),
            ],
        )?;
        Ok(())
    }

    /// For testing only. Reads the private `bytesMax` of the PHP instance.
    pub fn __bytes_max(&self) -> anyhow::Result<i64> {
        match self.reflection_get_property("bytesMax")? {
            PluginValue::Int(bytes_max) => Ok(bytes_max),
            other => Err(Self::shape_error("bytesMax", &other)),
        }
    }

    /// For testing only. Sets the private `bytesMax` of the PHP instance.
    pub fn __set_bytes_max(&self, bytes_max: i64) -> anyhow::Result<()> {
        self.reflection_set_property("bytesMax", PluginValue::Int(bytes_max))
    }

    /// For testing only. Sets the private `progress` of the PHP instance.
    pub fn __set_progress(&self, progress: bool) -> anyhow::Result<()> {
        self.reflection_set_property("progress", PluginValue::Bool(progress))
    }

    /// For testing only. Reads the private `lastProgress` of the PHP instance.
    pub fn __last_progress(&self) -> anyhow::Result<Option<i64>> {
        match self.reflection_get_property("lastProgress")? {
            PluginValue::Null => Ok(None),
            PluginValue::Int(last_progress) => Ok(Some(last_progress)),
            other => Err(Self::shape_error("lastProgress", &other)),
        }
    }

    fn reflection_invoke(
        &self,
        method: &str,
        args: Vec<PluginValue>,
    ) -> anyhow::Result<PluginValue> {
        let entity = PluginValue::PhpHandle(self.entity()?.clone());
        Self::php_result(shirabe_php_rpc::call_function_with_dispatcher(
            "__shirabe_reflection_invoke",
            vec![entity, PluginValue::string(method), PluginValue::List(args)],
            Some(&mut PluginRpcDispatcher::default()),
        ))
    }

    fn reflection_get_property(&self, name: &str) -> anyhow::Result<PluginValue> {
        let entity = PluginValue::PhpHandle(self.entity()?.clone());
        Self::php_result(shirabe_php_rpc::call_function_with_dispatcher(
            "__shirabe_reflection_get_property",
            vec![entity, PluginValue::string(name)],
            Some(&mut PluginRpcDispatcher::default()),
        ))
    }

    fn reflection_set_property(&self, name: &str, value: PluginValue) -> anyhow::Result<()> {
        let entity = PluginValue::PhpHandle(self.entity()?.clone());
        Self::php_result(shirabe_php_rpc::call_function_with_dispatcher(
            "__shirabe_reflection_set_property",
            vec![entity, PluginValue::string(name), value],
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        Ok(())
    }
}

impl Drop for RemoteFilesystem {
    fn drop(&mut self) {
        if let Some(entity) = self.entity.get() {
            // A dead worker has nothing left to release.
            let _ = shirabe_php_rpc::release_php_handle(entity.phandle);
        }
    }
}
