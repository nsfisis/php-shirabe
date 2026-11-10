//! The methods each proxied Composer class answers over RPC, one module per class.

pub(crate) mod autoload_generator;
pub(crate) mod composer;
pub(crate) mod config;
pub(crate) mod download_manager;
pub(crate) mod event_dispatcher;
pub(crate) mod events;
pub(crate) mod factory;
pub(crate) mod filesystem;
pub(crate) mod http_downloader;
pub(crate) mod installation_manager;
pub(crate) mod io;
pub(crate) mod locker;
pub(crate) mod operation;
pub(crate) mod package;
pub(crate) mod plugin;
pub(crate) mod process;
pub(crate) mod process_executor;
pub(crate) mod repository;
pub(crate) mod repository_manager;
pub(crate) mod transaction;
