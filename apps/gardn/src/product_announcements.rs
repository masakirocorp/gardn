use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const PRODUCT_ANNOUNCEMENTS_PATH: &str = "product-announcements.json";
const BUNDLED_ANNOUNCEMENTS: &str = include_str!("../assets/product-announcements.json");
const FAKE_ANNOUNCEMENT_BODY_ENV: &str = "GARDN_FAKE_PRODUCT_ANNOUNCEMENT_BODY";
const FAKE_ANNOUNCEMENT_BODY_FILE_ENV: &str = "GARDN_FAKE_PRODUCT_ANNOUNCEMENT_BODY_FILE";
const FAKE_ANNOUNCEMENT_ID_ENV: &str = "GARDN_FAKE_PRODUCT_ANNOUNCEMENT_ID";
const FAKE_ANNOUNCEMENT_TITLE_ENV: &str = "GARDN_FAKE_PRODUCT_ANNOUNCEMENT_TITLE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductAnnouncement {
    pub version: String,
    pub id: String,
    pub title: String,
    pub body: String,
    pub preview: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct StoredProductAnnouncement {
    version: String,
    id: String,
    title: String,
    body: String,
}

#[derive(Debug, Deserialize)]
struct AnnouncementCatalog {
    announcements: Vec<StoredProductAnnouncement>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AnnouncementStore {
    #[serde(default)]
    seen: BTreeSet<String>,
}

impl StoredProductAnnouncement {
    fn seen_key(&self) -> String {
        seen_key(&self.version, &self.id)
    }

    fn into_product_announcement(self) -> ProductAnnouncement {
        ProductAnnouncement {
            version: self.version,
            id: self.id,
            title: self.title.trim().to_string(),
            body: normalize_body(&self.body),
            preview: false,
        }
    }
}

fn seen_key(version: &str, id: &str) -> String {
    format!("{version}/{id}")
}

pub fn store_path() -> PathBuf {
    crate::config::state_dir().join(PRODUCT_ANNOUNCEMENTS_PATH)
}

pub fn load_unseen_for_current_version() -> Option<ProductAnnouncement> {
    if let Some(announcement) = load_fake_for_current_version() {
        return Some(announcement);
    }

    #[cfg(test)]
    {
        None
    }
    #[cfg(not(test))]
    {
        load_unseen_from_catalog(
            BUNDLED_ANNOUNCEMENTS,
            &store_path(),
            env!("CARGO_PKG_VERSION"),
        )
    }
}

pub fn mark_seen(version: &str, id: &str) -> io::Result<()> {
    mark_seen_at(&store_path(), version, id)
}

fn load_fake_for_current_version() -> Option<ProductAnnouncement> {
    let body = std::env::var(FAKE_ANNOUNCEMENT_BODY_FILE_ENV)
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .or_else(|| std::env::var(FAKE_ANNOUNCEMENT_BODY_ENV).ok())?;
    let body = normalize_body(&body);
    if body.is_empty() {
        return None;
    }

    let id = std::env::var(FAKE_ANNOUNCEMENT_ID_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "local-preview".to_string());
    let title = std::env::var(FAKE_ANNOUNCEMENT_TITLE_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "product announcement preview".to_string());

    Some(ProductAnnouncement {
        version: env!("CARGO_PKG_VERSION").to_string(),
        id,
        title,
        body,
        preview: true,
    })
}

fn parse_catalog(content: &str) -> Result<AnnouncementCatalog, String> {
    let catalog: AnnouncementCatalog =
        serde_json::from_str(content).map_err(|err| format!("invalid JSON: {err}"))?;
    let mut versions = BTreeSet::new();
    for announcement in &catalog.announcements {
        if announcement.version.trim().is_empty()
            || announcement.id.trim().is_empty()
            || announcement.title.trim().is_empty()
            || normalize_body(&announcement.body).is_empty()
        {
            return Err("version, id, title, and body must be non-empty".to_string());
        }
        if !versions.insert(announcement.version.as_str()) {
            return Err(format!(
                "version {} has more than one announcement",
                announcement.version
            ));
        }
    }
    Ok(catalog)
}

fn load_unseen_from_catalog(
    catalog_content: &str,
    state_path: &Path,
    current_version: &str,
) -> Option<ProductAnnouncement> {
    let catalog = match parse_catalog(catalog_content) {
        Ok(catalog) => catalog,
        Err(err) => {
            tracing::warn!("failed to load bundled product announcements: {err}");
            return None;
        }
    };
    let announcement = catalog
        .announcements
        .into_iter()
        .find(|announcement| announcement.version == current_version)?;
    let store = load_store_from_path(state_path).unwrap_or_default();
    if store.seen.contains(&announcement.seen_key()) {
        return None;
    }
    Some(announcement.into_product_announcement())
}

fn mark_seen_at(path: &Path, version: &str, id: &str) -> io::Result<()> {
    let mut store = load_store_from_path(path).unwrap_or_default();
    store.seen.insert(seen_key(version, id));
    write_store_to_path(path, &store)
}

fn load_store_from_path(path: &Path) -> Option<AnnouncementStore> {
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_store_to_path(path: &Path, store: &AnnouncementStore) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(store).map_err(io::Error::other)?;
    let tmp_path = path.with_extension(format!("json.tmp.{}", std::process::id()));
    fs::write(&tmp_path, json)?;
    if let Err(err) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }
    Ok(())
}

fn normalize_body(body: &str) -> String {
    body.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TestEnvVar;

    fn env_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "gardn-product-announcements-{name}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn bundled_catalog_is_valid() {
        parse_catalog(BUNDLED_ANNOUNCEMENTS).expect("bundled product announcements");
    }

    #[test]
    fn current_unseen_bundled_announcement_is_delivered_once() {
        let path = temp_path("unseen");
        let catalog = r####"{
            "announcements": [{
                "version": "1.2.3",
                "id": "keymap-v2",
                "title": "Keymap changed",
                "body": "### Changed\n- One"
            }]
        }"####;

        let loaded = load_unseen_from_catalog(catalog, &path, "1.2.3").expect("announcement");
        assert_eq!(loaded.id, "keymap-v2");
        mark_seen_at(&path, &loaded.version, &loaded.id).unwrap();
        assert_eq!(load_unseen_from_catalog(catalog, &path, "1.2.3"), None);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn announcement_for_another_version_is_not_delivered() {
        let path = temp_path("other-version");
        let catalog = r####"{
            "announcements": [{
                "version": "1.2.3",
                "id": "keymap-v2",
                "title": "Keymap changed",
                "body": "### Changed\n- One"
            }]
        }"####;

        assert_eq!(load_unseen_from_catalog(catalog, &path, "1.2.4"), None);
    }

    #[test]
    fn catalog_rejects_multiple_announcements_for_one_version() {
        let catalog = r#"{
            "announcements": [
                {"version": "1.2.3", "id": "one", "title": "One", "body": "First"},
                {"version": "1.2.3", "id": "two", "title": "Two", "body": "Second"}
            ]
        }"#;

        assert_eq!(
            parse_catalog(catalog).unwrap_err(),
            "version 1.2.3 has more than one announcement"
        );
    }

    #[test]
    fn fake_announcement_body_env_creates_preview() {
        let _guard = env_lock().lock().unwrap();
        let _body_env = TestEnvVar::set(FAKE_ANNOUNCEMENT_BODY_ENV, "### Preview\n- Local body");
        let _title_env = TestEnvVar::set(FAKE_ANNOUNCEMENT_TITLE_ENV, "Local title");
        let _id_env = TestEnvVar::set(FAKE_ANNOUNCEMENT_ID_ENV, "local-id");

        let announcement = load_fake_for_current_version().expect("fake announcement");
        assert_eq!(announcement.id, "local-id");
        assert_eq!(announcement.title, "Local title");
        assert_eq!(announcement.body, "### Preview\n- Local body");
        assert!(announcement.preview);
    }
}
