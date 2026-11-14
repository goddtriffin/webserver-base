//! Post-sign-in redirects that can't leave your site.

/// `target` if it is a path on this site, else None. Refuses `//host`,
/// `/\host`, absolute URLs, control characters and anything over 1000 bytes:
/// the shapes open-redirect attacks use.
#[must_use]
pub fn safe(target: &str) -> Option<String> {
    let ok: bool = target.starts_with('/')
        && !target.starts_with("//")
        && !target.starts_with("/\\")
        && !target.chars().any(|c| c.is_control() || c == '\\')
        && target.len() < 1000;
    ok.then(|| target.to_owned())
}

#[cfg(test)]
mod tests {
    use super::safe;

    #[test]
    fn only_local_paths() {
        assert_eq!(safe("/play?level=2").as_deref(), Some("/play?level=2"));
        for bad in [
            "//evil.example",
            "https://evil.example",
            "/\\evil.example",
            "play",
            "/a\r\nx",
            "",
        ] {
            assert_eq!(safe(bad), None, "{bad:?}");
        }
    }
}
