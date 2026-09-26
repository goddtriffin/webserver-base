//! Turning post HTML into HTML that works outside this origin.
//!
//! A reader renders an entry on `feedly.com`, not on your site, so every
//! relative `src` and `href` in the content resolves against the wrong origin:
//! images 404 and internal links leave the site. Those same image paths are
//! also un-hashed, and `/static/*` only resolves at its content-hashed path.
//! Both are fixed here, in that order — bust, then absolutize.
//!
//! Rewriting is confined to real tags. `pulldown-cmark` escapes `<` inside a
//! code block but leaves the quotes alone, so a post *about* HTML contains a
//! literal `src="/a.png"` in its prose. Matching attributes anywhere in the
//! document would silently rewrite the code sample the post is teaching from.

use std::sync::LazyLock;

use regex::{Captures, Regex};

/// A real element, which is to say one whose `<` was not escaped.
static TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<[a-zA-Z][^>]*>").expect("a valid tag pattern"));

/// `src="…"` or `href='…'` inside such an element. The `regex` crate has no
/// backreferences, so the two quote styles are separate alternatives.
static ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(src|href)\s*=\s*(?:"([^"]*)"|'([^']*)')"#)
        .expect("a valid attribute pattern")
});

/// `srcset` on a real element — deliberately unsupported rather than
/// half-rewritten.
static SRCSET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<[a-zA-Z][^>]*\bsrcset\s*=").expect("a valid srcset pattern")
});

/// Anything already carrying a scheme, e.g. `https:`, `mailto:`, `data:`.
static SCHEME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z][a-zA-Z0-9+.\-]*:").expect("a valid scheme pattern"));

/// Content HTML, rewritten, plus what could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewritten {
    pub html: String,
    /// Declared `/static/*` references absent from the manifest. Reported by
    /// the caller rather than fatal: the page this entry mirrors already shows
    /// that broken image, so refusing to boot would hold the feed to a stricter
    /// standard than the site itself.
    pub unresolved: Vec<String>,
}

/// Whether content uses `srcset` on a real element.
///
/// A post whose prose merely *mentions* `srcset` is not a match: inside a code
/// block `pulldown-cmark` escapes the `<`, so there is no element to find.
#[must_use]
pub fn contains_srcset(html: &str) -> bool {
    SRCSET.is_match(html)
}

/// Rewrites every `src` and `href` on a real element so it resolves from
/// anywhere.
///
/// `resolve` maps a manifest path (`static/image/a.png`) to its hashed one, and
/// returns `None` when the asset is unknown.
pub fn rewrite_content<F>(html: &str, base_url: &str, entry_url: &str, resolve: &F) -> Rewritten
where
    F: Fn(&str) -> Option<String>,
{
    let mut unresolved: Vec<String> = Vec::new();

    let rewritten: String = TAG
        .replace_all(html, |tag: &Captures<'_>| {
            let tag: &str = tag.get(0).map_or("", |matched| matched.as_str());
            ATTRIBUTE
                .replace_all(tag, |attribute: &Captures<'_>| {
                    let name: &str = attribute.get(1).map_or("", |matched| matched.as_str());
                    let (value, quote): (&str, char) = attribute.get(2).map_or_else(
                        || {
                            (
                                attribute.get(3).map_or("", |matched| matched.as_str()),
                                '\'',
                            )
                        },
                        |matched| (matched.as_str(), '"'),
                    );

                    let resolved: String =
                        resolve_reference(value, base_url, entry_url, resolve, &mut unresolved);
                    format!("{name}={quote}{resolved}{quote}")
                })
                .into_owned()
        })
        .into_owned();

    Rewritten {
        html: rewritten,
        unresolved,
    }
}

/// Resolves one reference the way a browser on the post's own page would, then
/// makes it absolute.
fn resolve_reference<F>(
    reference: &str,
    base_url: &str,
    entry_url: &str,
    resolve: &F,
    unresolved: &mut Vec<String>,
) -> String
where
    F: Fn(&str) -> Option<String>,
{
    // Already absolute, protocol-relative, or a non-http scheme: leave it be.
    // Mirrors `sitemap::absolute` rather than inventing a second rule.
    if reference.is_empty() || reference.starts_with("//") || SCHEME.is_match(reference) {
        return String::from(reference);
    }

    // A bare fragment resolves against the entry, not against the reader's own
    // page — which is where it would otherwise land.
    if let Some(fragment) = reference.strip_prefix('#') {
        return format!("{entry_url}#{fragment}");
    }

    let rooted: String = if reference.starts_with('/') {
        String::from(reference)
    } else {
        // Relative to the post's directory, exactly as on the page itself.
        let directory: &str = entry_url
            .rfind('/')
            .map_or(entry_url, |index| &entry_url[..index]);
        return join(directory, reference);
    };

    let busted: String = bust(&rooted, resolve, unresolved);
    join(
        base_url.trim_end_matches('/'),
        busted.trim_start_matches('/'),
    )
}

/// Swaps a `/static/*` path for the hashed one it is actually served at.
fn bust<F>(rooted: &str, resolve: &F, unresolved: &mut Vec<String>) -> String
where
    F: Fn(&str) -> Option<String>,
{
    let key: &str = rooted.trim_start_matches('/');
    if !key.starts_with("static/") {
        return String::from(rooted);
    }

    if let Some(hashed) = resolve(key) {
        return format!("/{}", hashed.trim_start_matches('/'));
    }

    unresolved.push(String::from(rooted));
    String::from(rooted)
}

/// Joins two halves of a URL without doubling or dropping the slash.
fn join(left: &str, right: &str) -> String {
    format!(
        "{}/{}",
        left.trim_end_matches('/'),
        right.trim_start_matches('/')
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{Rewritten, contains_srcset, rewrite_content};

    const BASE: &str = "https://www.example.com";
    const ENTRY: &str = "https://www.example.com/blog/a-post";

    /// A manifest holding one hashed asset.
    fn resolver() -> impl Fn(&str) -> Option<String> {
        let mut manifest: BTreeMap<String, String> = BTreeMap::new();
        manifest.insert(
            String::from("static/image/blog/a.png"),
            String::from("static/image/blog/a.abc123.png"),
        );
        move |path: &str| manifest.get(path).cloned()
    }

    fn rewrite(html: &str) -> Rewritten {
        rewrite_content(html, BASE, ENTRY, &resolver())
    }

    #[test]
    fn a_rooted_static_image_is_hashed_and_then_made_absolute() {
        let expected: String =
            String::from("<img src=\"https://www.example.com/static/image/blog/a.abc123.png\">");
        let actual: Rewritten = rewrite("<img src=\"/static/image/blog/a.png\">");
        assert_eq!(expected, actual.html);
        assert_eq!(Vec::<String>::new(), actual.unresolved);
    }

    #[test]
    fn an_internal_link_is_made_absolute_without_a_manifest_lookup() {
        let expected: String =
            String::from("<a href=\"https://www.example.com/blog/other\">other</a>");
        let actual: Rewritten = rewrite("<a href=\"/blog/other\">other</a>");
        assert_eq!(expected, actual.html);
        assert_eq!(Vec::<String>::new(), actual.unresolved);
    }

    #[test]
    fn an_already_absolute_url_is_left_exactly_as_written() {
        let html: &str = "<a href=\"https://crates.io/crates/serde\">serde</a>";
        let expected: String = String::from(html);
        let actual: Rewritten = rewrite(html);
        assert_eq!(expected, actual.html);
    }

    #[test]
    fn non_http_schemes_and_protocol_relative_urls_are_left_alone() {
        let html: &str = "<a href=\"mailto:a@b.com\">m</a><img src=\"data:image/gif;base64,AA\"><a href=\"//cdn.example.com/x\">c</a>";
        let expected: String = String::from(html);
        let actual: Rewritten = rewrite(html);
        assert_eq!(expected, actual.html);
    }

    #[test]
    fn a_bare_fragment_resolves_against_the_post_not_the_readers_own_page() {
        let expected: String =
            String::from("<a href=\"https://www.example.com/blog/a-post#setup\">jump</a>");
        let actual: Rewritten = rewrite("<a href=\"#setup\">jump</a>");
        assert_eq!(expected, actual.html);
    }

    #[test]
    fn a_directory_relative_reference_resolves_the_way_the_page_itself_would() {
        let expected: String =
            String::from("<a href=\"https://www.example.com/blog/sibling\">s</a>");
        let actual: Rewritten = rewrite("<a href=\"sibling\">s</a>");
        assert_eq!(expected, actual.html);
    }

    #[test]
    fn single_quoted_attributes_are_rewritten_and_keep_their_quoting() {
        let expected: String = String::from("<a href='https://www.example.com/blog/other'>o</a>");
        let actual: Rewritten = rewrite("<a href='/blog/other'>o</a>");
        assert_eq!(expected, actual.html);
    }

    #[test]
    fn a_missing_static_asset_is_reported_and_left_untouched_rather_than_guessed_at() {
        let actual: Rewritten = rewrite("<img src=\"/static/image/blog/gone.png\">");

        let expected_html: String =
            String::from("<img src=\"https://www.example.com/static/image/blog/gone.png\">");
        assert_eq!(expected_html, actual.html);

        let expected_unresolved: Vec<String> = vec![String::from("/static/image/blog/gone.png")];
        assert_eq!(expected_unresolved, actual.unresolved);
    }

    #[test]
    fn a_code_sample_that_merely_shows_markup_is_never_rewritten() {
        // `pulldown-cmark` escapes `<` inside a fence but leaves the quotes, so
        // matching attributes document-wide would corrupt the lesson the post
        // is teaching.
        let html: &str = "<pre><code>&lt;img src=\"/static/image/blog/a.png\"&gt;</code></pre>";
        let expected: String = String::from(html);
        let actual: Rewritten = rewrite(html);
        assert_eq!(expected, actual.html);
    }

    #[test]
    fn srcset_on_a_real_element_is_detected() {
        assert!(contains_srcset("<img srcset=\"/a.png 1x, /b.png 2x\">"));
        assert!(contains_srcset("<img\n  srcset='/a.png 1x'>"));
    }

    #[test]
    fn a_post_that_only_writes_about_srcset_is_not_a_false_positive() {
        assert!(!contains_srcset(
            "<p>Use <code>srcset</code> for responsive images.</p>"
        ));
        assert!(!contains_srcset(
            "<pre><code>&lt;img srcset=\"a.png 1x\"&gt;</code></pre>"
        ));
    }

    #[test]
    fn several_attributes_in_one_document_are_all_rewritten() {
        let actual: Rewritten = rewrite(
            "<p><a href=\"/blog/one\">1</a> <img src=\"/static/image/blog/a.png\"> <a href=\"/blog/two\">2</a></p>",
        );

        let expected: String = String::from(
            "<p><a href=\"https://www.example.com/blog/one\">1</a> \
             <img src=\"https://www.example.com/static/image/blog/a.abc123.png\"> \
             <a href=\"https://www.example.com/blog/two\">2</a></p>",
        );
        assert_eq!(expected, actual.html);
    }
}
