//! ref: composer/src/Composer/DependencyResolver/Transaction.php

use std::any::Any;

use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, array_filter, array_intersect, array_keys, array_pop, array_unshift, spl_object_hash,
    strcmp, uasort,
};

use crate::dependency_resolver::operation::install_operation::InstallOperation;
use crate::dependency_resolver::operation::mark_alias_installed_operation::MarkAliasInstalledOperation;
use crate::dependency_resolver::operation::mark_alias_uninstalled_operation::MarkAliasUninstalledOperation;
use crate::dependency_resolver::operation::operation_interface::OperationInterface;
use crate::dependency_resolver::operation::uninstall_operation::UninstallOperation;
use crate::dependency_resolver::operation::update_operation::UpdateOperation;
use crate::package::alias_package::AliasPackage;
use crate::package::link::Link;
use crate::package::package_interface::PackageInterface;
use crate::repository::platform_repository::PlatformRepository;

/// @internal
#[derive(Debug)]
pub struct Transaction {
    /// @var OperationInterface[]
    pub(crate) operations: Vec<Box<dyn OperationInterface>>,

    /// Packages present at the beginning of the transaction
    /// @var PackageInterface[]
    pub(crate) present_packages: Vec<Box<dyn PackageInterface>>,

    /// Package set resulting from this transaction
    /// @var array<string, PackageInterface>
    pub(crate) result_package_map: IndexMap<String, Box<dyn PackageInterface>>,

    /// @var array<string, PackageInterface[]>
    pub(crate) result_packages_by_name: IndexMap<String, Vec<Box<dyn PackageInterface>>>,
}

impl Transaction {
    /// @param PackageInterface[] $presentPackages
    /// @param PackageInterface[] $resultPackages
    pub fn new(
        present_packages: Vec<Box<dyn PackageInterface>>,
        result_packages: Vec<Box<dyn PackageInterface>>,
    ) -> Self {
        let mut this = Self {
            operations: vec![],
            present_packages,
            result_package_map: IndexMap::new(),
            result_packages_by_name: IndexMap::new(),
        };
        this.set_result_package_maps(result_packages);
        this.operations = this.calculate_operations();
        this
    }

    /// @return OperationInterface[]
    pub fn get_operations(&self) -> &Vec<Box<dyn OperationInterface>> {
        &self.operations
    }

    /// @param PackageInterface[] $resultPackages
    fn set_result_package_maps(&mut self, result_packages: Vec<Box<dyn PackageInterface>>) {
        // PHP: static function (PackageInterface $a, PackageInterface $b): int { ... };
        // TODO(phase-b): bridge the closure to uasort's argument type
        let _package_sort = |a: &dyn PackageInterface, b: &dyn PackageInterface| -> i64 {
            // sort alias packages by the same name behind their non alias version
            if a.get_name() == b.get_name() {
                let a_is_alias = (a.as_any() as &dyn Any)
                    .downcast_ref::<AliasPackage>()
                    .is_some();
                let b_is_alias = (b.as_any() as &dyn Any)
                    .downcast_ref::<AliasPackage>()
                    .is_some();
                if a_is_alias != b_is_alias {
                    return if a_is_alias { -1 } else { 1 };
                }

                // if names are the same, compare version, e.g. to sort aliases reliably, actual order does not matter
                return strcmp(b.get_version(), a.get_version());
            }

            strcmp(b.get_name(), a.get_name())
        };

        self.result_package_map = IndexMap::new();
        for package in result_packages {
            for name in package.get_names(true) {
                self.result_packages_by_name
                    .entry(name)
                    .or_insert_with(Vec::new)
                    .push(package.clone_box());
            }
            self.result_package_map
                .insert(spl_object_hash(package.as_ref()), package);
        }

        // TODO(phase-b): uasort signature mismatch — needs to operate on the IndexMap with a PackageInterface comparator
        uasort(
            todo!("&mut self.result_package_map"),
            |_a: &str, _b: &str| -> i64 { todo!("package_sort") },
        );
        let names: Vec<String> = self.result_packages_by_name.keys().cloned().collect();
        for _name in &names {
            uasort(
                todo!("&mut self.result_packages_by_name[name]"),
                |_a: &str, _b: &str| -> i64 { todo!("package_sort") },
            );
        }
    }

    /// @return OperationInterface[]
    pub(crate) fn calculate_operations(&mut self) -> Vec<Box<dyn OperationInterface>> {
        let mut operations: Vec<Box<dyn OperationInterface>> = vec![];

        let mut present_package_map: IndexMap<String, Box<dyn PackageInterface>> = IndexMap::new();
        let mut remove_map: IndexMap<String, Box<dyn PackageInterface>> = IndexMap::new();
        let mut present_alias_map: IndexMap<String, Box<dyn PackageInterface>> = IndexMap::new();
        let mut remove_alias_map: IndexMap<String, Box<dyn PackageInterface>> = IndexMap::new();
        for package in &self.present_packages {
            if (package.as_any() as &dyn Any)
                .downcast_ref::<AliasPackage>()
                .is_some()
            {
                let key = format!("{}::{}", package.get_name(), package.get_version());
                present_alias_map.insert(key.clone(), package.clone_box());
                remove_alias_map.insert(key, package.clone_box());
            } else {
                present_package_map.insert(package.get_name().to_string(), package.clone_box());
                remove_map.insert(package.get_name().to_string(), package.clone_box());
            }
        }

        // PHP: $stack = $this->getRootPackages();
        let mut stack: Vec<Box<dyn PackageInterface>> =
            self.get_root_packages().into_values().collect();

        let mut visited: IndexMap<String, bool> = IndexMap::new();
        let mut processed: IndexMap<String, bool> = IndexMap::new();

        while !stack.is_empty() {
            let package = array_pop(&mut stack).unwrap();

            if processed.contains_key(&spl_object_hash(package.as_ref())) {
                continue;
            }

            if !visited.contains_key(&spl_object_hash(package.as_ref())) {
                visited.insert(spl_object_hash(package.as_ref()), true);

                stack.push(package.clone_box());
                if let Some(alias) = (package.as_any() as &dyn Any).downcast_ref::<AliasPackage>() {
                    stack.push(alias.get_alias_of().clone_box());
                } else {
                    for link in package.get_requires().values() {
                        let possible_requires = self.get_providers_in_result(link);

                        for require in possible_requires {
                            stack.push(require);
                        }
                    }
                }
            } else if !processed.contains_key(&spl_object_hash(package.as_ref())) {
                processed.insert(spl_object_hash(package.as_ref()), true);

                if (package.as_any() as &dyn Any)
                    .downcast_ref::<AliasPackage>()
                    .is_some()
                {
                    let alias_key = format!("{}::{}", package.get_name(), package.get_version());
                    if present_alias_map.contains_key(&alias_key) {
                        remove_alias_map.shift_remove(&alias_key);
                    } else {
                        // TODO(phase-b): MarkAliasInstalledOperation::new expects AliasPackage by value
                        operations.push(Box::new(MarkAliasInstalledOperation::new(todo!(
                            "package as AliasPackage by value"
                        ))));
                    }
                } else if let Some(source) = present_package_map.get(package.get_name()) {
                    // do we need to update?
                    // TODO different for lock?
                    let present = present_package_map.get(package.get_name()).unwrap();
                    // TODO(phase-b): downcast to CompletePackageInterface trait object
                    let package_is_complete = false;
                    let present_is_complete = false;
                    let abandoned_or_replacement_changed =
                        package_is_complete && present_is_complete && {
                            // PHP: $package->isAbandoned() !== $presentPackageMap[$package->getName()]->isAbandoned()
                            //      || $package->getReplacementPackage() !== $presentPackageMap[$package->getName()]->getReplacementPackage()
                            todo!("compare abandoned/replacement across CompletePackageInterface")
                        };
                    if package.get_version() != present.get_version()
                        || package.get_dist_reference() != present.get_dist_reference()
                        || package.get_source_reference() != present.get_source_reference()
                        || abandoned_or_replacement_changed
                    {
                        operations.push(Box::new(UpdateOperation::new(
                            source.clone_box(),
                            package.clone_box(),
                        )));
                    }
                    remove_map.shift_remove(package.get_name());
                } else {
                    operations.push(Box::new(InstallOperation::new(package.clone_box())));
                    remove_map.shift_remove(package.get_name());
                }
            }
        }

        for (_name, package) in remove_map {
            // PHP: array_unshift($operations, new Operation\UninstallOperation($package));
            array_unshift(
                &mut operations,
                Box::new(UninstallOperation::new(package)) as Box<dyn OperationInterface>,
            );
        }
        for (_name_version, _package) in remove_alias_map {
            // TODO(phase-b): MarkAliasUninstalledOperation::new expects AliasPackage by value
            operations.push(Box::new(MarkAliasUninstalledOperation::new(todo!(
                "package as AliasPackage by value"
            ))));
        }

        let operations = self.move_plugins_to_front(operations);
        // TODO fix this:
        // we have to do this again here even though the above stack code did it because moving plugins moves them before uninstalls
        let operations = self.move_uninstalls_to_front(operations);

        // TODO skip updates which don't update? is this needed? we shouldn't schedule this update in the first place?
        // if ('update' === $opType) { ... }

        // PHP: return $this->operations = $operations;
        // TODO(phase-b): self.operations assignment plus return — caller needs owned Vec
        self.operations = todo!("operations cloned for both assignment and return");
        todo!("return cloned operations")
    }

    /// Determine which packages in the result are not required by any other packages in it.
    ///
    /// These serve as a starting point to enumerate packages in a topological order despite potential cycles.
    /// If there are packages with a cycle on the top level the package with the lowest name gets picked
    ///
    /// @return array<string, PackageInterface>
    pub(crate) fn get_root_packages(&self) -> IndexMap<String, Box<dyn PackageInterface>> {
        let mut roots: IndexMap<String, Box<dyn PackageInterface>> = self
            .result_package_map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone_box()))
            .collect();

        for (package_hash, package) in &self.result_package_map {
            if !roots.contains_key(package_hash) {
                continue;
            }

            for link in package.get_requires().values() {
                let possible_requires = self.get_providers_in_result(link);

                for require in possible_requires {
                    // PHP: if ($require !== $package) — strict reference inequality
                    if spl_object_hash(require.as_ref()) != spl_object_hash(package.as_ref()) {
                        roots.shift_remove(&spl_object_hash(require.as_ref()));
                    }
                }
            }
        }

        roots
    }

    /// @return PackageInterface[]
    pub(crate) fn get_providers_in_result(&self, link: &Link) -> Vec<Box<dyn PackageInterface>> {
        let Some(packages) = self.result_packages_by_name.get(link.get_target()) else {
            return vec![];
        };

        packages.iter().map(|p| p.clone_box()).collect()
    }

    /// Workaround: if your packages depend on plugins, we must be sure
    /// that those are installed / updated first; else it would lead to packages
    /// being installed multiple times in different folders, when running Composer
    /// twice.
    ///
    /// While this does not fix the root-causes of https://github.com/composer/composer/issues/1147,
    /// it at least fixes the symptoms and makes usage of composer possible (again)
    /// in such scenarios.
    ///
    /// @param  OperationInterface[] $operations
    /// @return OperationInterface[] reordered operation list
    fn move_plugins_to_front(
        &self,
        mut operations: Vec<Box<dyn OperationInterface>>,
    ) -> Vec<Box<dyn OperationInterface>> {
        let mut dl_modifying_plugins_no_deps: Vec<Box<dyn OperationInterface>> = vec![];
        let mut dl_modifying_plugins_with_deps: Vec<Box<dyn OperationInterface>> = vec![];
        let mut dl_modifying_plugin_requires: Vec<String> = vec![];
        let mut plugins_no_deps: Vec<Box<dyn OperationInterface>> = vec![];
        let mut plugins_with_deps: Vec<Box<dyn OperationInterface>> = vec![];
        let mut plugin_requires: Vec<String> = vec![];

        // PHP: foreach (array_reverse($operations, true) as $idx => $op)
        // TODO(phase-b): array_reverse preserves keys (true); iterate indices in reverse to mimic
        let mut to_remove: Vec<usize> = vec![];
        for idx in (0..operations.len()).rev() {
            let op = &operations[idx];

            let package: Box<dyn PackageInterface> = if let Some(install_op) =
                (op.as_ref() as &dyn Any).downcast_ref::<InstallOperation>()
            {
                install_op.get_package().clone_box()
            } else if let Some(update_op) =
                (op.as_ref() as &dyn Any).downcast_ref::<UpdateOperation>()
            {
                update_op.get_target_package().clone_box()
            } else {
                continue;
            };

            let extra = package.get_extra();
            let is_downloads_modifying_plugin = package.get_type() == "composer-plugin"
                && extra.contains_key("plugin-modifies-downloads")
                && matches!(
                    extra.get("plugin-modifies-downloads"),
                    Some(PhpMixed::Bool(true))
                );

            // is this a downloads modifying plugin or a dependency of one?
            if is_downloads_modifying_plugin
                || array_intersect(&package.get_names(true), &dl_modifying_plugin_requires).len()
                    > 0
            {
                // get the package's requires, but filter out any platform requirements
                let requires: Vec<String> = array_filter(
                    &array_keys(&package.get_requires()),
                    |req: &String| -> bool { !PlatformRepository::is_platform_package(req) },
                );

                // is this a plugin with no meaningful dependencies?
                if is_downloads_modifying_plugin && requires.is_empty() {
                    // plugins with no dependencies go to the very front
                    // TODO(phase-b): move ownership of operations[idx] into the new vec
                    array_unshift(
                        &mut dl_modifying_plugins_no_deps,
                        todo!("operations[idx] moved out"),
                    );
                } else {
                    // capture the requirements for this package so those packages will be moved up as well
                    dl_modifying_plugin_requires.extend(requires);
                    // move the operation to the front
                    array_unshift(
                        &mut dl_modifying_plugins_with_deps,
                        todo!("operations[idx] moved out"),
                    );
                }

                to_remove.push(idx);
                continue;
            }

            // is this package a plugin?
            let is_plugin = package.get_type() == "composer-plugin"
                || package.get_type() == "composer-installer";

            // is this a plugin or a dependency of a plugin?
            if is_plugin || array_intersect(&package.get_names(true), &plugin_requires).len() > 0 {
                // get the package's requires, but filter out any platform requirements
                let requires: Vec<String> = array_filter(
                    &array_keys(&package.get_requires()),
                    |req: &String| -> bool { !PlatformRepository::is_platform_package(req) },
                );

                // is this a plugin with no meaningful dependencies?
                if is_plugin && requires.is_empty() {
                    // plugins with no dependencies go to the very front
                    array_unshift(&mut plugins_no_deps, todo!("operations[idx] moved out"));
                } else {
                    // capture the requirements for this package so those packages will be moved up as well
                    plugin_requires.extend(requires);
                    // move the operation to the front
                    array_unshift(&mut plugins_with_deps, todo!("operations[idx] moved out"));
                }

                to_remove.push(idx);
            }
        }

        // PHP: unset($operations[$idx]) removes by index — perform in descending order
        to_remove.sort_by(|a, b| b.cmp(a));
        for idx in to_remove {
            operations.remove(idx);
        }

        // PHP: array_merge($dlModifyingPluginsNoDeps, $dlModifyingPluginsWithDeps, $pluginsNoDeps, $pluginsWithDeps, $operations)
        let mut result: Vec<Box<dyn OperationInterface>> = vec![];
        result.extend(dl_modifying_plugins_no_deps);
        result.extend(dl_modifying_plugins_with_deps);
        result.extend(plugins_no_deps);
        result.extend(plugins_with_deps);
        result.extend(operations);
        result
    }

    /// Removals of packages should be executed before installations in
    /// case two packages resolve to the same path (due to custom installers)
    ///
    /// @param  OperationInterface[] $operations
    /// @return OperationInterface[] reordered operation list
    fn move_uninstalls_to_front(
        &self,
        mut operations: Vec<Box<dyn OperationInterface>>,
    ) -> Vec<Box<dyn OperationInterface>> {
        let mut uninst_ops: Vec<Box<dyn OperationInterface>> = vec![];
        let mut to_remove: Vec<usize> = vec![];
        for (idx, op) in operations.iter().enumerate() {
            let is_uninstall = (op.as_ref() as &dyn Any)
                .downcast_ref::<UninstallOperation>()
                .is_some()
                || (op.as_ref() as &dyn Any)
                    .downcast_ref::<MarkAliasUninstalledOperation>()
                    .is_some();
            if is_uninstall {
                // TODO(phase-b): move ownership out of operations[idx]
                uninst_ops.push(todo!("operations[idx] moved out"));
                to_remove.push(idx);
            }
        }

        to_remove.sort_by(|a, b| b.cmp(a));
        for idx in to_remove {
            operations.remove(idx);
        }

        let mut result: Vec<Box<dyn OperationInterface>> = vec![];
        result.extend(uninst_ops);
        result.extend(operations);
        result
    }
}
