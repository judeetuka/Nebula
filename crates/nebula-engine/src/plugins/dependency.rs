use std::collections::HashMap;

use nebula_plugin_sdk::manifest::{PluginDependency, PluginManifest};

/// Resolves plugin dependencies and computes installation order.
pub struct DependencyResolver;

/// Errors that can occur during dependency resolution.
#[derive(Debug)]
pub enum DependencyError {
    /// A required dependency is not installed.
    Missing {
        plugin_id: String,
        required_by: String,
    },
    /// Installed version is too old.
    VersionMismatch {
        plugin_id: String,
        required: String,
        installed: String,
    },
    /// Circular dependency detected.
    CircularDependency { chain: Vec<String> },
}

impl std::fmt::Display for DependencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing {
                plugin_id,
                required_by,
            } => write!(
                f,
                "Missing dependency: {} required by {}",
                plugin_id, required_by
            ),
            Self::VersionMismatch {
                plugin_id,
                required,
                installed,
            } => write!(
                f,
                "Version mismatch for {}: requires >= {}, installed {}",
                plugin_id, required, installed
            ),
            Self::CircularDependency { chain } => {
                write!(f, "Circular dependency detected: {}", chain.join(" -> "))
            }
        }
    }
}

/// Simple semver comparison: returns true if `installed >= required`.
///
/// Compares major.minor.patch numerically. If either version string is
/// malformed, falls back to lexicographic comparison.
fn version_satisfies(installed: &str, required: &str) -> bool {
    let parse = |s: &str| -> Option<(u64, u64, u64)> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some((
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            parts[2].parse().ok()?,
        ))
    };

    match (parse(installed), parse(required)) {
        (Some(inst), Some(req)) => inst >= req,
        _ => installed >= required,
    }
}

impl DependencyResolver {
    /// Check if all dependencies for a plugin are satisfied.
    /// `installed` is the map of currently installed plugin IDs to their manifests.
    pub fn check_dependencies(
        manifest: &PluginManifest,
        installed: &HashMap<String, PluginManifest>,
    ) -> Vec<DependencyError> {
        let mut errors = Vec::new();

        for dep in &manifest.depends_on {
            match installed.get(&dep.id) {
                None => {
                    errors.push(DependencyError::Missing {
                        plugin_id: dep.id.clone(),
                        required_by: manifest.id.clone(),
                    });
                }
                Some(installed_manifest) => {
                    if !version_satisfies(&installed_manifest.version, &dep.min_version) {
                        errors.push(DependencyError::VersionMismatch {
                            plugin_id: dep.id.clone(),
                            required: dep.min_version.clone(),
                            installed: installed_manifest.version.clone(),
                        });
                    }
                }
            }
        }

        errors
    }

    /// Get the list of missing dependencies that need to be installed.
    pub fn missing_dependencies(
        manifest: &PluginManifest,
        installed: &HashMap<String, PluginManifest>,
    ) -> Vec<PluginDependency> {
        manifest
            .depends_on
            .iter()
            .filter(|dep| !installed.contains_key(&dep.id))
            .cloned()
            .collect()
    }

    /// Compute installation order for a set of plugins (topological sort).
    /// Returns an error if circular dependencies are detected.
    pub fn resolve_install_order(
        manifests: &[PluginManifest],
    ) -> Result<Vec<String>, DependencyError> {
        let manifest_map: HashMap<&str, &PluginManifest> =
            manifests.iter().map(|m| (m.id.as_str(), m)).collect();

        let mut visited: HashMap<String, bool> = HashMap::new(); // false = in-progress, true = done
        let mut order: Vec<String> = Vec::new();

        for manifest in manifests {
            if !visited.contains_key(manifest.id.as_str()) {
                let mut chain = Vec::new();
                Self::topo_visit(
                    &manifest.id,
                    &manifest_map,
                    &mut visited,
                    &mut order,
                    &mut chain,
                )?;
            }
        }

        Ok(order)
    }

    /// Recursive DFS for topological sort.
    fn topo_visit(
        node: &str,
        manifests: &HashMap<&str, &PluginManifest>,
        visited: &mut HashMap<String, bool>,
        order: &mut Vec<String>,
        chain: &mut Vec<String>,
    ) -> Result<(), DependencyError> {
        if let Some(&done) = visited.get(node) {
            if done {
                return Ok(()); // Already fully processed
            }
            // In-progress means we have a cycle
            chain.push(node.to_string());
            return Err(DependencyError::CircularDependency {
                chain: chain.clone(),
            });
        }

        visited.insert(node.to_string(), false); // Mark as in-progress
        chain.push(node.to_string());

        if let Some(manifest) = manifests.get(node) {
            for dep in &manifest.depends_on {
                Self::topo_visit(&dep.id, manifests, visited, order, chain)?;
            }
        }

        chain.pop();
        visited.insert(node.to_string(), true); // Mark as done
        order.push(node.to_string());
        Ok(())
    }

    /// Check if uninstalling a plugin would break any dependents.
    /// Returns list of plugins that depend on this one.
    pub fn check_removal(
        plugin_id: &str,
        installed: &HashMap<String, PluginManifest>,
    ) -> Vec<String> {
        installed
            .values()
            .filter(|m| m.depends_on.iter().any(|dep| dep.id == plugin_id))
            .map(|m| m.id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_plugin_sdk::capabilities::PluginCapability;

    fn make_manifest(id: &str, version: &str, deps: Vec<PluginDependency>) -> PluginManifest {
        PluginManifest {
            id: id.to_string(),
            name: format!("Plugin {}", id),
            version: version.to_string(),
            description: "Test".to_string(),
            author: "Test".to_string(),
            abi: "x86_64".to_string(),
            entry_symbol: "nebula_plugin_init".to_string(),
            capabilities: vec![PluginCapability::Network],
            depends_on: deps,
        }
    }

    fn make_dep(id: &str, min_version: &str) -> PluginDependency {
        PluginDependency {
            id: id.to_string(),
            min_version: min_version.to_string(),
        }
    }

    // -------------------------------------------------------------------
    // version_satisfies
    // -------------------------------------------------------------------

    #[test]
    fn test_version_satisfies_equal() {
        assert!(version_satisfies("1.0.0", "1.0.0"));
    }

    #[test]
    fn test_version_satisfies_newer_patch() {
        assert!(version_satisfies("1.0.1", "1.0.0"));
    }

    #[test]
    fn test_version_satisfies_newer_minor() {
        assert!(version_satisfies("1.2.0", "1.0.0"));
    }

    #[test]
    fn test_version_satisfies_newer_major() {
        assert!(version_satisfies("2.0.0", "1.0.0"));
    }

    #[test]
    fn test_version_not_satisfies_older() {
        assert!(!version_satisfies("0.9.0", "1.0.0"));
    }

    #[test]
    fn test_version_not_satisfies_older_patch() {
        assert!(!version_satisfies("1.0.0", "1.0.1"));
    }

    // -------------------------------------------------------------------
    // check_dependencies
    // -------------------------------------------------------------------

    #[test]
    fn test_check_dependencies_all_satisfied() {
        let manifest = make_manifest("app", "1.0.0", vec![make_dep("lib-a", "1.0.0")]);

        let mut installed = HashMap::new();
        installed.insert(
            "lib-a".to_string(),
            make_manifest("lib-a", "1.0.0", vec![]),
        );

        let errors = DependencyResolver::check_dependencies(&manifest, &installed);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_check_dependencies_missing() {
        let manifest = make_manifest("app", "1.0.0", vec![make_dep("lib-a", "1.0.0")]);
        let installed = HashMap::new();

        let errors = DependencyResolver::check_dependencies(&manifest, &installed);
        assert_eq!(errors.len(), 1);
        assert!(matches!(&errors[0], DependencyError::Missing { plugin_id, .. } if plugin_id == "lib-a"));
    }

    #[test]
    fn test_check_dependencies_version_mismatch() {
        let manifest = make_manifest("app", "1.0.0", vec![make_dep("lib-a", "2.0.0")]);

        let mut installed = HashMap::new();
        installed.insert(
            "lib-a".to_string(),
            make_manifest("lib-a", "1.5.0", vec![]),
        );

        let errors = DependencyResolver::check_dependencies(&manifest, &installed);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            DependencyError::VersionMismatch { plugin_id, required, installed }
            if plugin_id == "lib-a" && required == "2.0.0" && installed == "1.5.0"
        ));
    }

    #[test]
    fn test_check_dependencies_newer_version_ok() {
        let manifest = make_manifest("app", "1.0.0", vec![make_dep("lib-a", "1.0.0")]);

        let mut installed = HashMap::new();
        installed.insert(
            "lib-a".to_string(),
            make_manifest("lib-a", "2.3.0", vec![]),
        );

        let errors = DependencyResolver::check_dependencies(&manifest, &installed);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_check_dependencies_no_deps() {
        let manifest = make_manifest("app", "1.0.0", vec![]);
        let installed = HashMap::new();

        let errors = DependencyResolver::check_dependencies(&manifest, &installed);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_check_dependencies_multiple_missing() {
        let manifest = make_manifest(
            "app",
            "1.0.0",
            vec![make_dep("lib-a", "1.0.0"), make_dep("lib-b", "1.0.0")],
        );
        let installed = HashMap::new();

        let errors = DependencyResolver::check_dependencies(&manifest, &installed);
        assert_eq!(errors.len(), 2);
    }

    // -------------------------------------------------------------------
    // missing_dependencies
    // -------------------------------------------------------------------

    #[test]
    fn test_missing_dependencies_returns_missing() {
        let manifest = make_manifest(
            "app",
            "1.0.0",
            vec![make_dep("lib-a", "1.0.0"), make_dep("lib-b", "1.0.0")],
        );

        let mut installed = HashMap::new();
        installed.insert(
            "lib-a".to_string(),
            make_manifest("lib-a", "1.0.0", vec![]),
        );

        let missing = DependencyResolver::missing_dependencies(&manifest, &installed);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].id, "lib-b");
    }

    #[test]
    fn test_missing_dependencies_none_missing() {
        let manifest = make_manifest("app", "1.0.0", vec![make_dep("lib-a", "1.0.0")]);

        let mut installed = HashMap::new();
        installed.insert(
            "lib-a".to_string(),
            make_manifest("lib-a", "1.0.0", vec![]),
        );

        let missing = DependencyResolver::missing_dependencies(&manifest, &installed);
        assert!(missing.is_empty());
    }

    // -------------------------------------------------------------------
    // resolve_install_order
    // -------------------------------------------------------------------

    #[test]
    fn test_resolve_install_order_linear() {
        // C depends on B, B depends on A
        let manifests = vec![
            make_manifest("C", "1.0.0", vec![make_dep("B", "1.0.0")]),
            make_manifest("B", "1.0.0", vec![make_dep("A", "1.0.0")]),
            make_manifest("A", "1.0.0", vec![]),
        ];

        let order = DependencyResolver::resolve_install_order(&manifests).unwrap();
        assert_eq!(order.len(), 3);

        let pos_a = order.iter().position(|x| x == "A").unwrap();
        let pos_b = order.iter().position(|x| x == "B").unwrap();
        let pos_c = order.iter().position(|x| x == "C").unwrap();

        // A must come before B, B before C
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn test_resolve_install_order_diamond() {
        // D depends on B and C, both depend on A
        let manifests = vec![
            make_manifest(
                "D",
                "1.0.0",
                vec![make_dep("B", "1.0.0"), make_dep("C", "1.0.0")],
            ),
            make_manifest("B", "1.0.0", vec![make_dep("A", "1.0.0")]),
            make_manifest("C", "1.0.0", vec![make_dep("A", "1.0.0")]),
            make_manifest("A", "1.0.0", vec![]),
        ];

        let order = DependencyResolver::resolve_install_order(&manifests).unwrap();
        assert_eq!(order.len(), 4);

        let pos_a = order.iter().position(|x| x == "A").unwrap();
        let pos_b = order.iter().position(|x| x == "B").unwrap();
        let pos_c = order.iter().position(|x| x == "C").unwrap();
        let pos_d = order.iter().position(|x| x == "D").unwrap();

        // A must come before B and C, both before D
        assert!(pos_a < pos_b);
        assert!(pos_a < pos_c);
        assert!(pos_b < pos_d);
        assert!(pos_c < pos_d);
    }

    #[test]
    fn test_resolve_install_order_circular_detection() {
        // A depends on B, B depends on A
        let manifests = vec![
            make_manifest("A", "1.0.0", vec![make_dep("B", "1.0.0")]),
            make_manifest("B", "1.0.0", vec![make_dep("A", "1.0.0")]),
        ];

        let result = DependencyResolver::resolve_install_order(&manifests);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, DependencyError::CircularDependency { .. }));
    }

    #[test]
    fn test_resolve_install_order_no_deps() {
        let manifests = vec![
            make_manifest("A", "1.0.0", vec![]),
            make_manifest("B", "1.0.0", vec![]),
        ];

        let order = DependencyResolver::resolve_install_order(&manifests).unwrap();
        assert_eq!(order.len(), 2);
    }

    #[test]
    fn test_resolve_install_order_empty() {
        let order = DependencyResolver::resolve_install_order(&[]).unwrap();
        assert!(order.is_empty());
    }

    #[test]
    fn test_resolve_install_order_single() {
        let manifests = vec![make_manifest("A", "1.0.0", vec![])];
        let order = DependencyResolver::resolve_install_order(&manifests).unwrap();
        assert_eq!(order, vec!["A"]);
    }

    // -------------------------------------------------------------------
    // check_removal
    // -------------------------------------------------------------------

    #[test]
    fn test_check_removal_safe() {
        let mut installed = HashMap::new();
        installed.insert("A".to_string(), make_manifest("A", "1.0.0", vec![]));
        installed.insert("B".to_string(), make_manifest("B", "1.0.0", vec![]));

        let dependents = DependencyResolver::check_removal("A", &installed);
        assert!(dependents.is_empty());
    }

    #[test]
    fn test_check_removal_would_break() {
        let mut installed = HashMap::new();
        installed.insert("A".to_string(), make_manifest("A", "1.0.0", vec![]));
        installed.insert(
            "B".to_string(),
            make_manifest("B", "1.0.0", vec![make_dep("A", "1.0.0")]),
        );
        installed.insert(
            "C".to_string(),
            make_manifest("C", "1.0.0", vec![make_dep("A", "1.0.0")]),
        );

        let mut dependents = DependencyResolver::check_removal("A", &installed);
        dependents.sort();
        assert_eq!(dependents, vec!["B", "C"]);
    }

    #[test]
    fn test_check_removal_nonexistent_plugin() {
        let installed = HashMap::new();
        let dependents = DependencyResolver::check_removal("ghost", &installed);
        assert!(dependents.is_empty());
    }

    // -------------------------------------------------------------------
    // DependencyError Display
    // -------------------------------------------------------------------

    #[test]
    fn test_dependency_error_display_missing() {
        let err = DependencyError::Missing {
            plugin_id: "lib-a".to_string(),
            required_by: "app".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("lib-a"));
        assert!(msg.contains("app"));
    }

    #[test]
    fn test_dependency_error_display_version() {
        let err = DependencyError::VersionMismatch {
            plugin_id: "lib-a".to_string(),
            required: "2.0.0".to_string(),
            installed: "1.0.0".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("2.0.0"));
        assert!(msg.contains("1.0.0"));
    }

    #[test]
    fn test_dependency_error_display_circular() {
        let err = DependencyError::CircularDependency {
            chain: vec!["A".to_string(), "B".to_string(), "A".to_string()],
        };
        let msg = format!("{}", err);
        assert!(msg.contains("A -> B -> A"));
    }
}
