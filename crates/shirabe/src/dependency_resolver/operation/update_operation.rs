//! ref: composer/src/Composer/DependencyResolver/Operation/UpdateOperation.php

use crate::dependency_resolver::operation::operation_interface::OperationInterface;
use crate::dependency_resolver::operation::solver_operation::SolverOperation;
use crate::package::package_interface::PackageInterface;
use crate::package::version::version_parser::VersionParser;

#[derive(Debug)]
pub struct UpdateOperation {
    pub(crate) initial_package: Box<dyn PackageInterface>,
    pub(crate) target_package: Box<dyn PackageInterface>,
}

impl UpdateOperation {
    pub fn new(initial: Box<dyn PackageInterface>, target: Box<dyn PackageInterface>) -> Self {
        Self { initial_package: initial, target_package: target }
    }

    pub fn get_initial_package(&self) -> &dyn PackageInterface {
        self.initial_package.as_ref()
    }

    pub fn get_target_package(&self) -> &dyn PackageInterface {
        self.target_package.as_ref()
    }

    pub fn format(initial_package: &dyn PackageInterface, target_package: &dyn PackageInterface, lock: bool) -> String {
        let mut from_version = initial_package.get_full_pretty_version(false, PackageInterface::DISPLAY_SOURCE_REF);
        let mut to_version = target_package.get_full_pretty_version(false, PackageInterface::DISPLAY_SOURCE_REF);

        if from_version == to_version && initial_package.get_source_reference() != target_package.get_source_reference() {
            from_version = initial_package.get_full_pretty_version(true, PackageInterface::DISPLAY_SOURCE_REF);
            to_version = target_package.get_full_pretty_version(true, PackageInterface::DISPLAY_SOURCE_REF);
        } else if from_version == to_version && initial_package.get_dist_reference() != target_package.get_dist_reference() {
            from_version = initial_package.get_full_pretty_version(true, PackageInterface::DISPLAY_DIST_REF);
            to_version = target_package.get_full_pretty_version(true, PackageInterface::DISPLAY_DIST_REF);
        }

        let action_name = if VersionParser::is_upgrade(&initial_package.get_version(), &target_package.get_version()) {
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
    fn get_operation_type(&self) -> String {
        Self::TYPE.to_string()
    }

    fn show(&self, lock: bool) -> String {
        Self::format(self.initial_package.as_ref(), self.target_package.as_ref(), lock)
    }

    fn to_string(&self) -> String {
        self.show(true)
    }
}
