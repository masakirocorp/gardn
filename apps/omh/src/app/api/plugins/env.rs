use crate::api::schema::InstalledPluginInfo;

pub(super) fn plugin_path_env(plugin: &InstalledPluginInfo) -> Vec<(String, String)> {
    let component = crate::api::schema::plugin_managed_path_component(&plugin.plugin_id);
    let config_dir = crate::config::config_dir().join("plugins").join(&component);
    let state_dir = crate::config::state_dir().join("plugins").join(component);

    let config_dir = config_dir.display().to_string();
    let state_dir = state_dir.display().to_string();
    vec![
        ("OMH_PLUGIN_ROOT".to_string(), plugin.plugin_root.clone()),
        ("OMH_PLUGIN_CONFIG_DIR".to_string(), config_dir.clone()),
        ("OMH_PLUGIN_STATE_DIR".to_string(), state_dir.clone()),
        ("HERDR_PLUGIN_ROOT".to_string(), plugin.plugin_root.clone()),
        ("HERDR_PLUGIN_CONFIG_DIR".to_string(), config_dir),
        ("HERDR_PLUGIN_STATE_DIR".to_string(), state_dir),
    ]
}
