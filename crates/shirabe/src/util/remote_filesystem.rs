//! ref: composer/src/Composer/Util/RemoteFilesystem.php sha1:27ac0473e50d4f24137f3c4ac66fe7843aa34e9f
//!
//! The real `Composer\Util\RemoteFilesystem`, running in the PHP worker: every method forwards
//! to an instance there. `HttpDownloader` reaches for it last, for the requests neither
//! `NativeHttpDownloader` nor `LocalFilesystem` takes, which only PHP's stream layer can serve
//! (the stream wrappers of other schemes, including the ones plugins register). The instance is
//! built on first use, so a run that never needs it never spawns the worker.

use crate::config::Config;
use crate::event_dispatcher::EventDispatcher;
use crate::io::IOInterface;
use crate::php_worker::call::PhpCall;
use crate::php_worker::rpc::table::{config_handle_value, io_handle_value};
use crate::util::StreamContextOptions;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_rpc::{FromWire, PhpObjHandle, PhpThrow, PluginValue, ToWire, wire};
use shirabe_php_shim::{
    PhpMixed, path_from_bytes, path_to_bytes, php_regex, php_str, preg_match, utf8_text,
};

const CLASS: &BStr = php_str(b"Composer\\Util\\RemoteFilesystem");

/// Result of `RemoteFilesystem::get` — string content, `true` (for copy), or `false`.
#[derive(Debug, Clone)]
pub enum GetResult {
    False,
    True,
    Content(Vec<u8>),
}

#[derive(Debug)]
pub struct RemoteFilesystem {
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    options: StreamContextOptions,
    disable_tls: bool,
    entity: std::cell::OnceCell<PhpObjHandle>,
}

impl RemoteFilesystem {
    /// TODO(plugin): PHP's trailing `?AuthHelper $authHelper` parameter is not offered: a Rust
    /// `AuthHelper` has no wire form, so the PHP instance always builds its own.
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        options: StreamContextOptions,
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
        origin_url: impl AsRef<[u8]>,
        file_url: impl AsRef<[u8]>,
        file_name: impl AsRef<std::path::Path>,
        progress: bool,
        options: StreamContextOptions,
    ) -> anyhow::Result<GetResult> {
        self.call(
            BStr::new("copy"),
            vec![
                PluginValue::string(origin_url.as_ref()),
                PluginValue::string(file_url.as_ref()),
                PluginValue::string(path_to_bytes(file_name.as_ref())),
                PluginValue::Bool(progress),
                options.to_wire(),
            ],
        )
    }

    pub fn get_contents(
        &self,
        origin_url: impl AsRef<[u8]>,
        file_url: impl AsRef<[u8]>,
        progress: bool,
        options: StreamContextOptions,
    ) -> anyhow::Result<GetResult> {
        self.call(
            BStr::new("getContents"),
            vec![
                PluginValue::string(origin_url.as_ref()),
                PluginValue::string(file_url.as_ref()),
                PluginValue::Bool(progress),
                options.to_wire(),
            ],
        )
    }

    pub fn get_options(&self) -> anyhow::Result<StreamContextOptions> {
        self.call(BStr::new("getOptions"), Vec::new())
    }

    pub fn set_options(&self, options: StreamContextOptions) -> anyhow::Result<()> {
        self.call::<PluginValue>(BStr::new("setOptions"), vec![options.to_wire()])?;
        Ok(())
    }

    pub fn is_tls_disabled(&self) -> anyhow::Result<bool> {
        self.call(BStr::new("isTlsDisabled"), Vec::new())
    }

    pub fn get_last_headers(&self) -> anyhow::Result<Vec<BString>> {
        self.call(BStr::new("getLastHeaders"), Vec::new())
    }

    /// Computed on this side: a static method has no instance to forward to, and the function is
    /// pure.
    pub fn find_status_code(headers: &[BString]) -> Option<i64> {
        let mut value: Option<i64> = None;
        for header in headers {
            if let Some(m) = preg_match(php_regex!("{^HTTP/\\S+ (\\d+)}i"), header) {
                value = m
                    .get(1)
                    .and_then(|s| utf8_text(s)?.parse().ok())
                    .or(Some(0));
            }
        }

        value
    }

    pub fn find_status_message(&self, headers: &[BString]) -> anyhow::Result<Option<BString>> {
        self.call(
            BStr::new("findStatusMessage"),
            vec![headers.to_vec().to_wire()],
        )
    }

    /// The worker-side instance, built on the first call.
    fn entity(&self) -> anyhow::Result<&PhpObjHandle> {
        if let Some(entity) = self.entity.get() {
            return Ok(entity);
        }

        EventDispatcher::ensure_composer_php_runtime(|| {
            let cache_dir = self.config.borrow().get_path("cache-dir")?;
            Ok(path_from_bytes(&cache_dir).to_path_buf())
        })?;
        let handle: PhpObjHandle = PhpCall::New(CLASS).run(vec![
            io_handle_value(&self.io)?,
            config_handle_value(&self.config),
            self.options.clone().to_wire(),
            PluginValue::Bool(self.disable_tls),
        ])?;

        Ok(self.entity.get_or_init(|| handle))
    }

    /// Forwards the call to the PHP instance. An exception it throws comes back as the ported
    /// class, so a caller catching `TransportException` catches it as it would under Composer.
    fn call<R: FromWire>(&self, method: &BStr, args: Vec<PluginValue>) -> anyhow::Result<R> {
        PhpCall::Method(self.entity()?, method).run(args)
    }

    /// For testing only. Invokes the protected `getOptionsForUrl` of the PHP instance, mirroring
    /// the `ReflectionMethod` access in `RemoteFilesystemTest::callGetOptionsForUrl`.
    pub fn __get_options_for_url(
        &self,
        origin_url: impl AsRef<[u8]>,
        additional_options: StreamContextOptions,
    ) -> anyhow::Result<IndexMap<BString, PhpMixed>> {
        self.reflection_invoke(
            BStr::new("getOptionsForUrl"),
            vec![
                PluginValue::string(origin_url.as_ref()),
                additional_options.to_wire(),
            ],
        )
    }

    /// For testing only. Sets the private `fileUrl` of the PHP instance, mirroring the
    /// `ReflectionProperty` access in `RemoteFilesystemTest::callGetOptionsForUrl`.
    pub fn __set_file_url(&self, file_url: impl AsRef<[u8]>) -> anyhow::Result<()> {
        self.reflection_set_property(BStr::new("fileUrl"), PluginValue::string(file_url.as_ref()))
    }

    /// For testing only. Invokes the protected `callbackGet` of the PHP instance, mirroring the
    /// `ReflectionMethod` access in `RemoteFilesystemTest::callCallbackGet`.
    pub fn __callback_get(
        &self,
        notification_code: i64,
        severity: i64,
        message: Option<BString>,
        message_code: i64,
        bytes_transferred: i64,
        bytes_max: i64,
    ) -> anyhow::Result<()> {
        self.reflection_invoke::<PluginValue>(
            BStr::new("callbackGet"),
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
        self.reflection_get_property(BStr::new("bytesMax"))
    }

    /// For testing only. Sets the private `bytesMax` of the PHP instance.
    pub fn __set_bytes_max(&self, bytes_max: i64) -> anyhow::Result<()> {
        self.reflection_set_property(BStr::new("bytesMax"), PluginValue::Int(bytes_max))
    }

    /// For testing only. Sets the private `progress` of the PHP instance.
    pub fn __set_progress(&self, progress: bool) -> anyhow::Result<()> {
        self.reflection_set_property(BStr::new("progress"), PluginValue::Bool(progress))
    }

    /// For testing only. Reads the private `lastProgress` of the PHP instance.
    pub fn __last_progress(&self) -> anyhow::Result<Option<i64>> {
        self.reflection_get_property(BStr::new("lastProgress"))
    }

    fn reflection_invoke<R: FromWire>(
        &self,
        method: &BStr,
        args: Vec<PluginValue>,
    ) -> anyhow::Result<R> {
        let entity = PluginValue::PhpHandle(self.entity()?.clone());
        PhpCall::Function(BStr::new("__shirabe_reflection_invoke")).run(vec![
            entity,
            PluginValue::string(method),
            PluginValue::List(args),
        ])
    }

    fn reflection_get_property<R: FromWire>(&self, name: &BStr) -> anyhow::Result<R> {
        let entity = PluginValue::PhpHandle(self.entity()?.clone());
        PhpCall::Function(BStr::new("__shirabe_reflection_get_property"))
            .run(vec![entity, PluginValue::string(name)])
    }

    fn reflection_set_property(&self, name: &BStr, value: PluginValue) -> anyhow::Result<()> {
        let entity = PluginValue::PhpHandle(self.entity()?.clone());
        PhpCall::Function(BStr::new("__shirabe_reflection_set_property"))
            .run::<PluginValue>(vec![entity, PluginValue::string(name), value])?;
        Ok(())
    }
}

/// `string|bool` as `get()` answers it: the content, or whether a copy succeeded.
impl FromWire for GetResult {
    fn from_wire(value: &PluginValue) -> Result<Self, PhpThrow> {
        match value {
            PluginValue::Bool(true) => Ok(GetResult::True),
            PluginValue::Bool(false) => Ok(GetResult::False),
            PluginValue::String(content) => Ok(GetResult::Content(content.to_vec())),
            other => Err(wire::unexpected("a string or a bool", other)),
        }
    }
}
