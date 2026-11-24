//! Legacy identifiers used to read saved session reports.
use anyhow::{bail, Result};
const MARKERS: [&str; 3] = [
    "BLUE_BASE_MARKER",
    "BLUE_VARIANT_MARKER",
    "BLUE_BASE_MARKER",
];
pub(crate) fn probes(scenario: &str) -> Result<[String; 3]> {
    match scenario {
        "literal_marker" => Ok(MARKERS.map(str::to_owned)),
        "inert_html" => Ok(MARKERS.map(|marker| format!("<blue-probe>{marker}</blue-probe>"))),
        _ => bail!("Use literal_marker or inert_html; arbitrary payloads are not accepted"),
    }
}
pub(crate) fn target(raw: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(raw)?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(
            url.path(),
            "/preview/reflected-xss/vulnerable" | "/preview/reflected-xss/fixed"
        )
    {
        bail!(
            "Use the exact local Between Bytes preview URL without query, credentials or fragment"
        );
    }
    Ok(url)
}
