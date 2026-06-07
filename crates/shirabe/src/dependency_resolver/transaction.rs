//! ref: composer/src/Composer/DependencyResolver/Transaction.php

use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, array_filter, array_intersect, array_keys, array_pop, array_unshift, strcmp, uasort,
};

use crate::dependency_resolver::operation::InstallOperation;
use crate::dependency_resolver::operation::MarkAliasInstalledOperation;
use crate::dependency_resolver::operation::MarkAliasUninstalledOperation;
use crate::dependency_resolver::operation::OperationInterface;
use crate::dependency_resolver::operation::UninstallOperation;
use crate::dependency_resolver::operation::UpdateOperation;
use crate::package::Link;
use crate::package::PackageInterfaceHandle;
use crate::repository::PlatformRepository;

/// @internal
#[derive(Debug)]
pub struct Transaction {
    /// @var OperationInterface[]
    pub(crate) operations: Vec<std::rc::Rc<dyn OperationInterface>>,

    /// Packages present at the beginning of the transaction
    /// @var PackageInterface[]
    pub(crate) present_packages: Vec<PackageInterfaceHandle>,

    /// Package set resulting from this transaction
    /// @var array<string, PackageInterface>
    pub(crate) result_package_map: IndexMap<String, PackageInterfaceHandle>,

    /// @var array<string, PackageInterface[]>
    pub(crate) result_packages_by_name: IndexMap<String, Vec<PackageInterfaceHandle>>,
}

impl Default for Transaction {
    fn default() -> Self {
        Self {
            operations: vec![],
            present_packages: vec![],
            result_package_map: IndexMap::new(),
            result_packages_by_name: IndexMap::new(),
        }
    }
}

impl Transaction {
    /// @param PackageInterface[] $presentPackages
    /// @param PackageInterface[] $resultPackages
    pub fn new(
        present_packages: Vec<PackageInterfaceHandle>,
        result_packages: Vec<PackageInterfaceHandle>,
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

    pub fn get_operations(&self) -> &Vec<std::rc::Rc<dyn OperationInterface>> {
        &self.operations
    }

    /// @param PackageInterface[] $resultPackages
    fn set_result_package_maps(&mut self, result_packages: Vec<PackageInterfaceHandle>) {
        // PHP: static function (PackageInterface $a, PackageInterface $b): int { ... };
        // TODO(phase-b): bridge the closure to uasort's argument type
        let _package_sort = |a: &PackageInterfaceHandle, b: &PackageInterfaceHandle| -> i64 {
            // sort alias packages by the same name behind their non alias version
            if a.get_name() == b.get_name() {
                let a_is_alias = a.as_alias().is_some();
                let b_is_alias = b.as_alias().is_some();
                if a_is_alias != b_is_alias {
                    return if a_is_alias { -1 } else { 1 };
                }

                // if names are the same, compare version, e.g. to sort aliases reliably, actual order does not matter
                return strcmp(&b.get_version(), &a.get_version());
            }

            strcmp(&b.get_name(), &a.get_name())
        };

        self.result_package_map = IndexMap::new();
        for package in result_packages {
            for name in package.get_names(true) {
                self.result_packages_by_name
                    .entry(name)
                    .or_insert_with(Vec::new)
                    .push(package.clone());
            }
            self.result_package_map
                .insert(package.ptr_id().to_string(), package);
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
    pub(crate) fn calculate_operations(&mut self) -> Vec<std::rc::Rc<dyn OperationInterface>> {
        let mut operations: Vec<std::rc::Rc<dyn OperationInterface>> = vec![];

        let mut present_package_map: IndexMap<String, PackageInterfaceHandle> = IndexMap::new();
        let mut remove_map: IndexMap<String, PackageInterfaceHandle> = IndexMap::new();
        let mut present_alias_map: IndexMap<String, PackageInterfaceHandle> = IndexMap::new();
        let mut remove_alias_map: IndexMap<String, PackageInterfaceHandle> = IndexMap::new();
        for package in &self.present_packages {
            if package.as_alias().is_some() {
                let key = format!("{}::{}", package.get_name(), package.get_version());
                present_alias_map.insert(key.clone(), package.clone());
                remove_alias_map.insert(key, package.clone());
            } else {
                present_package_map.insert(package.get_name().to_string(), package.clone());
                remove_map.insert(package.get_name().to_string(), package.clone());
            }
        }

        // PHP: $stack = $this->getRootPackages();
        let mut stack: Vec<PackageInterfaceHandle> =
            self.get_root_packages().into_values().collect();

        let mut visited: IndexMap<String, bool> = IndexMap::new();
        let mut processed: IndexMap<String, bool> = IndexMap::new();

        while !stack.is_empty() {
            let package = array_pop(&mut stack).unwrap();

            if processed.contains_key(&package.ptr_id().to_string()) {
                continue;
            }

            if !visited.contains_key(&package.ptr_id().to_string()) {
                visited.insert(package.ptr_id().to_string(), true);

                stack.push(package.clone());
                if let Some(alias) = package.as_alias() {
                    stack.push(alias.get_alias_of().into());
                } else {
                    for link in package.get_requires().values() {
                        let possible_requires = self.get_providers_in_result(link);

                        for require in possible_requires {
                            stack.push(require);
                        }
                    }
                }
            } else if !processed.contains_key(&package.ptr_id().to_string()) {
                processed.insert(package.ptr_id().to_string(), true);

                if package.as_alias().is_some() {
                    let alias_key = format!("{}::{}", package.get_name(), package.get_version());
                    if present_alias_map.contains_key(&alias_key) {
                        remove_alias_map.shift_remove(&alias_key);
                    } else {
                        // TODO(phase-b): MarkAliasInstalledOperation::new expects AliasPackage by value
                        operations.push(std::rc::Rc::new(MarkAliasInstalledOperation::new(todo!(
                            "package as AliasPackage by value"
                        ))));
                    }
                } else if let Some(source) = present_package_map.get(&package.get_name()).cloned() {
                    // do we need to update?
                    // TODO different for lock?
                    let present = present_package_map.get(&package.get_name()).unwrap();
                    // PHP: $package instanceof CompletePackageInterface
                    //      && $presentPackageMap[$package->getName()] instanceof CompletePackageInterface
                    //      && ($package->isAbandoned() !== $presentPackageMap[...]->isAbandoned()
                    //          || $package->getReplacementPackage() !== $presentPackageMap[...]->getReplacementPackage())
                    let abandoned_or_replacement_changed =
                        match (package.as_complete(), present.as_complete()) {
                            (Some(package), Some(present)) => {
                                package.is_abandoned() != present.is_abandoned()
                                    || package.get_replacement_package()
                                        != present.get_replacement_package()
                            }
                            _ => false,
                        };
                    if package.get_version() != present.get_version()
                        || package.get_dist_reference() != present.get_dist_reference()
                        || package.get_source_reference() != present.get_source_reference()
                        || abandoned_or_replacement_changed
                    {
                        operations.push(std::rc::Rc::new(UpdateOperation::new(
                            source.clone(),
                            package.clone(),
                        )));
                    }
                    remove_map.shift_remove(&package.get_name());
                } else {
                    operations.push(std::rc::Rc::new(InstallOperation::new(package.clone())));
                    remove_map.shift_remove(&package.get_name());
                }
            }
        }

        for (_name, package) in remove_map {
            // PHP: array_unshift($operations, new Operation\UninstallOperation($package));
            array_unshift(
                &mut operations,
                std::rc::Rc::new(UninstallOperation::new(package))
                    as std::rc::Rc<dyn OperationInterface>,
            );
        }
        for (_name_version, _package) in remove_alias_map {
            // TODO(phase-b): MarkAliasUninstalledOperation::new expects AliasPackage by value
            operations.push(std::rc::Rc::new(MarkAliasUninstalledOperation::new(todo!(
                "package as AliasPackage by value"
            ))));
        }

        let operations = self.move_plugins_to_front(operations);
        // TODO fix this:
        // we have to do this again here even though the above stack code did it because moving plugins moves them before uninstalls
        let operations = self.move_uninstalls_to_front(operations);

        // TODO skip updates which don't update? is this needed? we shouldn't schedule this update in the first place?
        // if ('update' === $opType) { ... }

        self.operations = operations.clone();
        operations
    }

    /// Determine which packages in the result are not required by any other packages in it.
    ///
    /// These serve as a starting point to enumerate packages in a topological order despite potential cycles.
    /// If there are packages with a cycle on the top level the package with the lowest name gets picked
    ///
    /// @return array<string, PackageInterface>
    pub(crate) fn get_root_packages(&self) -> IndexMap<String, PackageInterfaceHandle> {
        let mut roots: IndexMap<String, PackageInterfaceHandle> = self
            .result_package_map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        for (package_hash, package) in &self.result_package_map {
            if !roots.contains_key(package_hash) {
                continue;
            }

            for link in package.get_requires().values() {
                let possible_requires = self.get_providers_in_result(link);

                for require in possible_requires {
                    // PHP: if ($require !== $package) — strict reference inequality
                    if require.ptr_id().to_string() != package.ptr_id().to_string() {
                        roots.shift_remove(&require.ptr_id().to_string());
                    }
                }
            }
        }

        roots
    }

    /// @return PackageInterface[]
    pub(crate) fn get_providers_in_result(&self, link: &Link) -> Vec<PackageInterfaceHandle> {
        let Some(packages) = self.result_packages_by_name.get(link.get_target()) else {
            return vec![];
        };

        packages.iter().cloned().collect()
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
        mut operations: Vec<std::rc::Rc<dyn OperationInterface>>,
    ) -> Vec<std::rc::Rc<dyn OperationInterface>> {
        let mut dl_modifying_plugins_no_deps: Vec<std::rc::Rc<dyn OperationInterface>> = vec![];
        let mut dl_modifying_plugins_with_deps: Vec<std::rc::Rc<dyn OperationInterface>> = vec![];
        let mut dl_modifying_plugin_requires: Vec<String> = vec![];
        let mut plugins_no_deps: Vec<std::rc::Rc<dyn OperationInterface>> = vec![];
        let mut plugins_with_deps: Vec<std::rc::Rc<dyn OperationInterface>> = vec![];
        let mut plugin_requires: Vec<String> = vec![];

        // PHP: foreach (array_reverse($operations, true) as $idx => $op)
        // TODO(phase-b): array_reverse preserves keys (true); iterate indices in reverse to mimic
        let mut to_remove: Vec<usize> = vec![];
        for idx in (0..operations.len()).rev() {
            let op = &operations[idx];

            let package: PackageInterfaceHandle = if let Some(install_op) =
                op.as_ref().as_any().downcast_ref::<InstallOperation>()
            {
                install_op.get_package().clone()
            } else if let Some(update_op) = op.as_ref().as_any().downcast_ref::<UpdateOperation>() {
                update_op.get_target_package().clone()
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
                    array_unshift(&mut dl_modifying_plugins_no_deps, operations[idx].clone());
                } else {
                    // capture the requirements for this package so those packages will be moved up as well
                    dl_modifying_plugin_requires.extend(requires);
                    // move the operation to the front
                    array_unshift(&mut dl_modifying_plugins_with_deps, operations[idx].clone());
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
                    array_unshift(&mut plugins_no_deps, operations[idx].clone());
                } else {
                    // capture the requirements for this package so those packages will be moved up as well
                    plugin_requires.extend(requires);
                    // move the operation to the front
                    array_unshift(&mut plugins_with_deps, operations[idx].clone());
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
        let mut result: Vec<std::rc::Rc<dyn OperationInterface>> = vec![];
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
        mut operations: Vec<std::rc::Rc<dyn OperationInterface>>,
    ) -> Vec<std::rc::Rc<dyn OperationInterface>> {
        let mut uninst_ops: Vec<std::rc::Rc<dyn OperationInterface>> = vec![];
        let mut to_remove: Vec<usize> = vec![];
        for (idx, op) in operations.iter().enumerate() {
            let is_uninstall = op
                .as_ref()
                .as_any()
                .downcast_ref::<UninstallOperation>()
                .is_some()
                || op
                    .as_ref()
                    .as_any()
                    .downcast_ref::<MarkAliasUninstalledOperation>()
                    .is_some();
            if is_uninstall {
                uninst_ops.push(op.clone());
                to_remove.push(idx);
            }
        }

        to_remove.sort_by(|a, b| b.cmp(a));
        for idx in to_remove {
            operations.remove(idx);
        }

        let mut result: Vec<std::rc::Rc<dyn OperationInterface>> = vec![];
        result.extend(uninst_ops);
        result.extend(operations);
        result
    }
}
