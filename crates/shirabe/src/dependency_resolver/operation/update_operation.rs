//! ref: composer/src/Composer/DependencyResolver/Operation/UpdateOperation.php

use crate::dependency_resolver::operation::OperationInterface;
use crate::dependency_resolver::operation::SolverOperation;
use crate::package::PackageInterface;
use crate::package::PackageInterfaceHandle;
use crate::package::version::VersionParser;

#[derive(Debug)]
pub struct UpdateOperation {
    pub(crate) initial_package: PackageInterfaceHandle,
    pub(crate) target_package: PackageInterfaceHandle,
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

    pub fn format(
        initial_package: PackageInterfaceHandle,
        target_package: PackageInterfaceHandle,
        lock: bool,
    ) -> String {
        let mut from_version = initial_package
            .get_full_pretty_version(false, <dyn PackageInterface>::DISPLAY_SOURCE_REF);
        let mut to_version = target_package
            .get_full_pretty_version(false, <dyn PackageInterface>::DISPLAY_SOURCE_REF);

        if from_version == to_version
            && initial_package.get_source_reference() != target_package.get_source_reference()
        {
            from_version = initial_package
                .get_full_pretty_version(true, <dyn PackageInterface>::DISPLAY_SOURCE_REF);
            to_version = target_package
                .get_full_pretty_version(true, <dyn PackageInterface>::DISPLAY_SOURCE_REF);
        } else if from_version == to_version
            && initial_package.get_dist_reference() != target_package.get_dist_reference()
        {
            from_version = initial_package
                .get_full_pretty_version(true, <dyn PackageInterface>::DISPLAY_DIST_REF);
            to_version = target_package
                .get_full_pretty_version(true, <dyn PackageInterface>::DISPLAY_DIST_REF);
        }

        let action_name = if VersionParser::is_upgrade(
            &initial_package.get_version(),
            &target_package.get_version(),
        )
        .unwrap_or(false)
        {
            "Upgrading"
        } else {
            "Downgrading"
        };

        format!(
            "{} <info>{}</info> (<comment>{}</comment> => <comment>{}</comment>)",
            action_name,
            initial_package.get_pretty_name(),
            from_version,
            to_version,
        )
    }
}

impl SolverOperation for UpdateOperation {
    const TYPE: &'static str = "update";
}

impl OperationInterface for UpdateOperation {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_operation_type(&self) -> String {
        Self::TYPE.to_string()
    }

    fn show(&self, lock: bool) -> String {
        Self::format(
            self.initial_package.clone(),
            self.target_package.clone(),
            lock,
        )
    }

    fn to_string(&self) -> String {
        self.show(true)
    }

    fn as_update_operation(&self) -> Option<&UpdateOperation> {
        Some(self)
    }
}
