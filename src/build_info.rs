//! Build identity helpers.

pub const BASE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn version() -> String {
    BASE_VERSION.to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_cargo_version() {
        assert_eq!(super::version(), super::BASE_VERSION);
    }
}
