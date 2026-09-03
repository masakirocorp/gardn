//! Canonical `GARDN_*` process identity.
//!
//! Tools written for upstream Herdr still read `HERDR_*`. Every Gardn identity
//! variable is therefore published under both names from this module. Callers
//! set `GARDN_*` only; the `HERDR_*` alias is derived.

use std::ffi::OsStr;

use portable_pty::CommandBuilder;

pub const GARDN_PREFIX: &str = "GARDN_";

pub fn herdr_alias(gardn_key: &str) -> Option<String> {
    gardn_key
        .strip_prefix(GARDN_PREFIX)
        .map(|rest| format!("HERDR_{rest}"))
}

pub fn apply(cmd: &mut CommandBuilder, gardn_key: &str, value: impl AsRef<OsStr>) {
    let value = value.as_ref();
    cmd.env(gardn_key, value);
    if let Some(alias) = herdr_alias(gardn_key) {
        cmd.env(alias, value);
    }
}

pub fn push(
    env: &mut Vec<(String, String)>,
    gardn_key: impl Into<String>,
    value: impl Into<String>,
) {
    let gardn_key = gardn_key.into();
    let value = value.into();
    if let Some(alias) = herdr_alias(&gardn_key) {
        env.push((alias, value.clone()));
    }
    env.push((gardn_key, value));
}

pub fn is_alias_of(gardn_keys: &[&str], key: &str) -> bool {
    gardn_keys
        .iter()
        .copied()
        .any(|gardn_key| key == gardn_key || herdr_alias(gardn_key).as_deref() == Some(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn herdr_alias_rewrites_the_gardn_prefix_only() {
        assert_eq!(
            herdr_alias("GARDN_PANE_ID").as_deref(),
            Some("HERDR_PANE_ID")
        );
        assert_eq!(
            herdr_alias("GARDN_SOCKET_PATH").as_deref(),
            Some("HERDR_SOCKET_PATH")
        );
        assert_eq!(herdr_alias("HERDR_PANE_ID"), None);
        assert_eq!(herdr_alias("PATH"), None);
    }

    #[test]
    fn push_emits_matching_gardn_and_herdr_values() {
        let mut env = Vec::new();
        push(&mut env, "GARDN_PANE_ID", "w1:p1");
        assert_eq!(
            env,
            vec![
                ("HERDR_PANE_ID".to_string(), "w1:p1".to_string()),
                ("GARDN_PANE_ID".to_string(), "w1:p1".to_string()),
            ]
        );
    }

    #[test]
    fn apply_sets_both_names_on_a_command() {
        let mut cmd = CommandBuilder::new("shell");
        apply(&mut cmd, "GARDN_PANE_ID", "w1:p1");
        assert_eq!(
            cmd.get_env("GARDN_PANE_ID")
                .map(|v| v.to_string_lossy().into_owned()),
            Some("w1:p1".to_string())
        );
        assert_eq!(
            cmd.get_env("HERDR_PANE_ID")
                .map(|v| v.to_string_lossy().into_owned()),
            Some("w1:p1".to_string())
        );
    }
}
