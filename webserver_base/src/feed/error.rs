//! Why a feed could not be produced.

/// Why a feed could not be produced.
///
/// Every variant except the serialization ones is a boot failure: the deploy
/// never cuts over and the previous container keeps serving, which is the same
/// signal as a broken feed without the outage.
#[derive(Debug, thiserror::Error)]
pub enum FeedError {
    /// An entry is missing something every reader renders.
    #[error("feed entry `{path}` has an empty {field}; readers render it as a blank row")]
    IncompleteEntry { path: String, field: &'static str },

    /// An entry's path is not site-relative, so it cannot become a stable id.
    #[error(
        "feed entry path `{path}` is not rooted; use a site-relative path such as `/blog/a-post`"
    )]
    UnrootedPath { path: String },

    /// Content carries a `srcset`, whose comma-separated URL+descriptor grammar
    /// this library deliberately does not rewrite.
    #[error(
        "feed entry `{path}` uses `srcset`, which is not rewritten; use a single `src` so the \
         feed cannot ship half-absolute image URLs"
    )]
    SrcsetUnsupported { path: String },

    /// The feed advertises a page the site does not serve.
    #[error(
        "the feed's page `{page_url}` is not a declared page; its `rel=\"alternate\"` would 404"
    )]
    PageNotDeclared { page_url: String },

    /// A declared entry image is absent from the asset manifest.
    #[error("feed entry `{path}` names image `{image}`, which is not in the asset manifest")]
    UnresolvedImage { path: String, image: String },

    /// The XML could not be written.
    #[error("failed to serialize the {format} feed")]
    Write {
        format: &'static str,
        #[source]
        source: std::io::Error,
    },

    /// Serialized XML was not valid UTF-8, which cannot happen for input this
    /// module produces.
    #[error("the serialized {format} feed was not valid UTF-8")]
    Encoding { format: &'static str },

    /// JSON Feed could not be serialized.
    #[error("failed to serialize the JSON feed")]
    Json {
        #[source]
        source: serde_json::Error,
    },
}
