use super::{SettingsAction, SettingsInput};
use crate::app::state::{
    SettingsIntegrationsTab, SettingsSection, SettingsState, SpriteSettingsDraft,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(super) fn active(settings: &SettingsState) -> bool {
    settings.section == SettingsSection::Integrations
        && settings.integrations_tab == SettingsIntegrationsTab::Sprites
}

pub(super) fn accepts_text(settings: &SettingsState, index: usize) -> bool {
    active(settings) && !settings.sprites.confirm_disable && (1..=7).contains(&index)
}

fn focused_text<'a>(state: &'a mut SettingsInput<'_>) -> Option<&'a mut String> {
    let index = state.client.settings.focused_input?;
    if !accepts_text(&state.client.settings, index) {
        return None;
    }
    state.client.settings.list.select(index);
    let draft = state
        .client
        .settings
        .sprites
        .draft
        .get_or_insert_with(|| SpriteSettingsDraft::from(&state.shared.sprites_config));
    match index {
        1 => Some(&mut draft.org),
        2 => Some(&mut draft.sprite_bin),
        3 => Some(&mut draft.node_bin),
        4 => Some(&mut draft.name_prefix),
        5 => Some(&mut draft.max_sprites),
        6 => Some(&mut draft.max_concurrent_operations),
        7 => Some(&mut draft.max_transfer_mib),
        _ => None,
    }
}

pub(super) fn edit_text(state: &mut SettingsInput<'_>, key: KeyEvent) -> bool {
    let edit = matches!(key.code, KeyCode::Backspace)
        || matches!(key.code, KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL))
        || matches!(key.code, KeyCode::Char(_) if key.modifiers.difference(KeyModifiers::SHIFT).is_empty());
    if !edit {
        return false;
    }
    let Some(value) = focused_text(state) else {
        return false;
    };
    match key.code {
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => value.clear(),
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::SUPER) => value.clear(),
        KeyCode::Backspace => {
            value.pop();
        }
        KeyCode::Char(character) => value.push(character),
        _ => {}
    }
    state.client.settings.sprites.message = None;
    true
}

pub(super) fn paste_text(state: &mut SettingsInput<'_>, text: &str) -> bool {
    let Some(value) = focused_text(state) else {
        return false;
    };
    value.extend(text.chars().filter(|character| !character.is_control()));
    state.client.settings.sprites.message = None;
    true
}

fn draft_config(state: &SettingsInput<'_>) -> Result<crate::api::schema::SpritesConfig, String> {
    let Some(draft) = state.client.settings.sprites.draft.as_ref() else {
        return Ok(state.sprites_config.clone());
    };
    let parse = |value: &str, label: &str| {
        value
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("{label} must be a positive integer."))
    };
    let config = crate::api::schema::SpritesConfig {
        enabled: state.sprites_config.enabled,
        org: draft.org.trim().to_string(),
        sprite_bin: draft.sprite_bin.trim().to_string(),
        node_bin: draft.node_bin.trim().to_string(),
        name_prefix: draft.name_prefix.trim().to_string(),
        max_sprites: parse(&draft.max_sprites, "Maximum Sprites")?,
        max_concurrent_operations: parse(
            &draft.max_concurrent_operations,
            "Concurrent operations",
        )?,
        max_transfer_mib: parse(&draft.max_transfer_mib, "Transfer limit")?,
    };
    crate::config::validate_sprites_config(&config)?;
    Ok(config)
}

pub(super) fn selected_action(state: &mut SettingsInput<'_>) -> Option<SettingsAction> {
    let selected = state.client.settings.list.selected;
    if state.client.settings.sprites.confirm_disable {
        if selected == 0 {
            state.client.settings.sprites.confirm_disable = false;
            state.client.settings.sprites.message = None;
            state.client.settings.scroll = 0;
            return None;
        }
        if selected == 1 {
            return Some(SettingsAction::SetSpritesEnabled(false));
        }
        return None;
    }
    if selected == 0 {
        if state.sprites_config.enabled
            && (!state.sprites_snapshot.resources.is_empty()
                || state.sprites_snapshot.operations.iter().any(|operation| {
                    matches!(
                        operation.status,
                        crate::api::schema::SpriteOperationStatus::Queued
                            | crate::api::schema::SpriteOperationStatus::Running
                    )
                }))
        {
            state.client.settings.sprites.confirm_disable = true;
            state.client.settings.sprites.message = None;
            state.client.settings.scroll = 0;
            state.client.settings.list.select(0);
            state.client.settings.focused_input = None;
            return None;
        }
        return Some(SettingsAction::SetSpritesEnabled(
            !state.sprites_config.enabled,
        ));
    }
    match draft_config(state) {
        Ok(config) => Some(SettingsAction::SaveSpritesConfig(config)),
        Err(message) => {
            state.client.settings.sprites.message = Some(message);
            state.client.settings.scroll = 0;
            None
        }
    }
}

impl crate::app::App {
    pub(crate) fn save_sprite_settings_for_view(
        &mut self,
        view: &mut crate::app::ClientViewState,
        config: crate::api::schema::SpritesConfig,
        clear_draft: bool,
    ) {
        let disabling = self.state.sprites_config.enabled && !config.enabled;
        match self.save_sprites_config(config) {
            Ok(()) => {
                if clear_draft {
                    view.settings.sprites.draft = None;
                }
                view.settings.sprites.confirm_disable = false;
                view.settings.sprites.message = Some(if disabling {
                    "Disabled locally. Remote resources and sessions are unchanged.".into()
                } else {
                    "Sprite settings saved.".into()
                });
            }
            Err(message) => view.settings.sprites.message = Some(message),
        }
        view.settings.scroll = 0;
    }
}
