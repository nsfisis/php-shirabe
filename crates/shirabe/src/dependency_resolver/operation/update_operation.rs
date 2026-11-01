//! ref: composer/src/Composer/DependencyResolver/Operation/UpdateOperation.php

use crate::dependency_resolver::operation::SolverOperation;
use crate::package::PackageInterfaceHandle;
use crate::package::version::VersionParser;
use shirabe_php_shim::bconcat;

#[derive(Debug, Clone)]
pub struct UpdateOperation {
    initial_package: PackageInterfaceHandle,
    target_package: PackageInterfaceHandle,
}

impl UpdateOperation {
    pub fn new(initial: PackageInterfaceHandle, target: PackageInterfaceHandle) -> Self {
        Self {
            initial_package: initial,
            target_package: target,
        }
    }

    pub fn get_initial_package(&self) -> PackageInterfaceHandle {
        self.initial_package.clone()
    }

    pub fn get_target_package(&self) -> PackageInterfaceHandle {
        self.target_package.clone()
    }

    pub fn show(&self, lock: bool) -> bstr::BString {
        Self::format(
            self.initial_package.clone(),
            self.target_package.clone(),
            lock,
        )
    }

    pub fn format(
        initial_package: PackageInterfaceHandle,
        target_package: PackageInterfaceHandle,
        lock: bool,
    ) -> bstr::BString {
        let mut from_version = initial_package
            .get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev);
        let mut to_version = target_package
            .get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev);

        if from_version == to_version
            && initial_package.get_source_reference() != target_package.get_source_reference()
        {
            from_version = initial_package
                .get_full_pretty_version(true, crate::package::DisplayMode::SourceRef);
            to_version = target_package
                .get_full_pretty_version(true, crate::package::DisplayMode::SourceRef);
        } else if from_version == to_version
            && initial_package.get_dist_reference() != target_package.get_dist_reference()
        {
            from_version =
                initial_package.get_full_pretty_version(true, crate::package::DisplayMode::DistRef);
            to_version =
                target_package.get_full_pretty_version(true, crate::package::DisplayMode::DistRef);
        }

        let action_name = if VersionParser::is_upgrade(
            initial_package.get_version(),
            target_package.get_version(),
        )
        .unwrap_or(false)
        {
            "Upgrading"
        } else {
            "Downgrading"
        };

        bconcat!(
            action_name,
            " <info>",
            initial_package.get_pretty_name(),
            "</info> (<comment>",
            from_version,
            "</comment> => <comment>",
            to_version,
            "</comment>)"
        )
    }
}

impl SolverOperation for UpdateOperation {
    const TYPE: &'static str = "update";
}

impl std::fmt::Display for UpdateOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.show(false).display_lossy())
    }
}
