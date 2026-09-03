use crate::api::schema::InstalledPluginInfo;

pub(super) fn plugin_path_env(plugin: &InstalledPluginInfo) -> Vec<(String, String)> {
    let component = crate::api::schema::plugin_managed_path_component(&plugin.plugin_id);
    let config_dir = crate::config::config_dir().join("plugins").join(&component);
    let state_dir = crate::config::state_dir().join("plugins").join(component);

    let mut env = Vec::new();
    crate::product_env::push(&mut env, "GARDN_PLUGIN_ROOT", plugin.plugin_root.clone());
    crate::product_env::push(
        &mut env,
        "GARDN_PLUGIN_CONFIG_DIR",
        config_dir.display().to_string(),
    );
    crate::product_env::push(
        &mut env,
        "GARDN_PLUGIN_STATE_DIR",
        state_dir.display().to_string(),
    );
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin() -> InstalledPluginInfo {
        InstalledPluginInfo {
            plugin_id: "example.plugin".to_string(),
            name: "Example plugin".to_string(),
            version: "0.1.0".to_string(),
            min_gardn_version: "0.2.0".to_string(),
            description: None,
            manifest_path: "/plugins/example/gardn-plugin.toml".to_string(),
            plugin_root: "/plugins/example".to_string(),
            enabled: true,
            platforms: None,
            build: Vec::new(),
            startup: Vec::new(),
            actions: Vec::new(),
            events: Vec::new(),
            panes: Vec::new(),
            link_handlers: Vec::new(),
            source: Default::default(),
            manifest_dialect: Default::default(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn plugin_paths_include_herdr_compatibility_aliases() {
        let env = plugin_path_env(&plugin());

        for (gardn, herdr) in [
            ("GARDN_PLUGIN_ROOT", "HERDR_PLUGIN_ROOT"),
            ("GARDN_PLUGIN_CONFIG_DIR", "HERDR_PLUGIN_CONFIG_DIR"),
            ("GARDN_PLUGIN_STATE_DIR", "HERDR_PLUGIN_STATE_DIR"),
        ] {
            let gardn_value = env
                .iter()
                .find(|(key, _)| key == gardn)
                .map(|(_, value)| value);
            let herdr_value = env
                .iter()
                .find(|(key, _)| key == herdr)
                .map(|(_, value)| value);
            assert_eq!(gardn_value, herdr_value, "{gardn} and {herdr}");
        }
    }
}
