//! ref: composer/tests/Composer/Test/ComposerTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::composer::Composer;
use shirabe::config::Config;
use shirabe::downloader::DownloadManager;
use shirabe::downloader::DownloadManagerInterface;
use shirabe::installer::InstallationManager;
use shirabe::installer::InstallationManagerInterface;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::json::JsonFile;
use shirabe::package::{Locker, LockerInterface, RootPackageHandle, RootPackageInterfaceHandle};
use shirabe::repository::RepositoryManager;
use shirabe::repository::RepositoryManagerInterface;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe::util::r#loop::Loop;
use shirabe::util::process_executor::ProcessExecutor;

fn null_io() -> Rc<RefCell<dyn IOInterface>> {
    Rc::new(RefCell::new(NullIO::new()))
}

fn http_downloader(io: &Rc<RefCell<dyn IOInterface>>) -> Rc<RefCell<HttpDownloader>> {
    let config = Rc::new(RefCell::new(Config::new(false, None)));
    Rc::new(RefCell::new(HttpDownloader::new(
        io.clone(),
        config,
        IndexMap::new(),
        true,
    )))
}

fn installation_manager(io: &Rc<RefCell<dyn IOInterface>>) -> Rc<RefCell<InstallationManager>> {
    let r#loop = Rc::new(RefCell::new(Loop::new(http_downloader(io), None)));
    Rc::new(RefCell::new(InstallationManager::new(
        r#loop,
        io.clone(),
        None,
    )))
}

#[test]
fn test_set_get_package() {
    let mut composer = Composer::new();
    let package: RootPackageInterfaceHandle = RootPackageHandle::new(
        "foo".to_string(),
        "1.0.0.0".to_string(),
        "1.0.0".to_string(),
    )
    .into();
    composer.set_package(package.clone());

    assert!(composer.get_package().ptr_eq(&package));
}

#[test]
fn test_set_get_locker() {
    let mut composer = Composer::new();
    let io = null_io();
    let json_file = JsonFile::new("composer.lock".to_string(), None, None).unwrap();
    let process = Rc::new(RefCell::new(ProcessExecutor::new(Some(io.clone()))));
    let locker: Rc<RefCell<dyn LockerInterface>> = Rc::new(RefCell::new(Locker::new(
        io.clone(),
        json_file,
        installation_manager(&io),
        "{}",
        process,
    )));
    composer.set_locker(locker.clone());

    assert!(Rc::ptr_eq(&composer.get_locker(), &locker));
}

#[test]
fn test_set_get_repository_manager() {
    let mut composer = Composer::new();
    let io = null_io();
    let config = Rc::new(RefCell::new(Config::new(false, None)));
    let manager: Rc<RefCell<dyn RepositoryManagerInterface>> = Rc::new(RefCell::new(
        RepositoryManager::new(io.clone(), config, http_downloader(&io), None, None),
    ));
    composer.set_repository_manager(manager.clone());

    assert!(Rc::ptr_eq(&composer.get_repository_manager(), &manager));
}

#[test]
fn test_set_get_download_manager() {
    let mut composer = Composer::new();
    let manager: Rc<RefCell<dyn DownloadManagerInterface>> =
        Rc::new(RefCell::new(DownloadManager::new(null_io(), false, None)));
    composer.set_download_manager(manager.clone());

    assert!(Rc::ptr_eq(&composer.get_download_manager(), &manager));
}

#[test]
fn test_set_get_installation_manager() {
    let mut composer = Composer::new();
    let io = null_io();
    let manager: Rc<RefCell<dyn InstallationManagerInterface>> = installation_manager(&io);
    composer.set_installation_manager(manager.clone());

    assert!(Rc::ptr_eq(&composer.get_installation_manager(), &manager));
}
