//! Keeps `docs/api/openapi.yaml` and `docs/api/README.md` in step with the
//! server.
//!
//! The router cannot be introspected — axum's `Router` has no public route
//! list — so this reads `router.rs` as text and extracts every
//! `.route("...", method(...))` call. Crude, but every route in that file is
//! written exactly that way, and a change in shape that slips past the regex
//! would leave zero routes, which fails loudly rather than passing quietly.
//!
//! The YAML is read with a small line-based walk rather than a YAML crate:
//! the only maintained `serde_yaml` successors are forks or compatibility
//! shims, and the workspace rule is that a dependency must pay its way. The
//! `paths:` block is two-space-indented keys followed by four-space-indented
//! methods, Prettier keeps it that way, and anything else under `paths` that
//! this walk misreads shows up as a spurious route the assertions below will
//! name.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;

const ROUTER_SOURCE: &str = include_str!("../src/router.rs");
const PROMETHEUS_SOURCE: &str = include_str!("../src/prometheus.rs");
const OPENAPI: &str = include_str!("../../../docs/api/openapi.yaml");
const README: &str = include_str!("../../../docs/api/README.md");

const HTTP_METHODS: [&str; 8] = [
    "get", "post", "put", "patch", "delete", "head", "options", "trace",
];

/// `(method, path)` for every `.route("/path", method(handler))` in the
/// router source. Method names may be bare (`get(...)`) or path-qualified
/// (`axum::routing::post(...)`), and both forms appear in the file.
fn routes_in_code() -> BTreeSet<(String, String)> {
    let mut found = BTreeSet::new();
    let mut rest = ROUTER_SOURCE;
    while let Some(start) = rest.find(".route(\"") {
        rest = &rest[start + ".route(\"".len()..];
        let path_end = rest.find('"').expect("an unterminated route path");
        let path = &rest[..path_end];
        let after_path = rest[path_end + 1..].trim_start_matches(',').trim_start();
        // Take the identifier up to the opening paren, then the last `::`
        // segment so `axum::routing::post` and `post` compare equal.
        let ident_end = after_path.find('(').expect("a route without a method call");
        let method = after_path[..ident_end]
            .rsplit("::")
            .next()
            .expect("rsplit yields at least one item")
            .trim();
        assert!(
            HTTP_METHODS.contains(&method),
            "route {path} uses `{method}`, which this test does not know how to read"
        );
        found.insert((method.to_owned(), path.to_owned()));
        rest = after_path;
    }
    found
}

/// `(method, path)` for every operation under `paths:` in `openapi.yaml`.
fn routes_in_document() -> BTreeSet<(String, String)> {
    let mut found = BTreeSet::new();
    let mut in_paths = false;
    let mut current_path: Option<String> = None;

    for line in OPENAPI.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let content = line.trim();

        if indent == 0 {
            // A new top-level key: either `paths:` itself or the end of it.
            in_paths = content == "paths:";
            current_path = None;
            continue;
        }
        if !in_paths {
            continue;
        }
        match indent {
            2 => {
                let key = content.strip_suffix(':').unwrap_or(content);
                assert!(
                    key.starts_with('/'),
                    "unexpected key `{key}` directly under `paths:`"
                );
                current_path = Some(key.to_owned());
            }
            4 => {
                let key = content.strip_suffix(':').unwrap_or(content);
                if HTTP_METHODS.contains(&key) {
                    let path = current_path
                        .clone()
                        .expect("a method before any path under `paths:`");
                    found.insert((key.to_owned(), path));
                }
            }
            _ => {}
        }
    }
    found
}

fn metric_names_in_code() -> BTreeSet<String> {
    // Every metric name is passed to `gauge(...)` as a quoted literal, and
    // also appears inside `writeln!` format strings; the literal form is the
    // one that is always a bare `"vitals_…"` token.
    let mut names = BTreeSet::new();
    let mut rest = PROMETHEUS_SOURCE;
    while let Some(start) = rest.find("\"vitals_") {
        rest = &rest[start + 1..];
        let end = rest
            .find(|c: char| !(c.is_ascii_lowercase() || c == '_'))
            .expect("an unterminated metric literal");
        // A literal such as `"vitals_cpu_percent {}"` is a format string, not
        // a name, but its prefix up to the first non-identifier character is
        // still the metric name, so including it is harmless and correct.
        names.insert(rest[..end].to_owned());
        rest = &rest[end..];
    }
    names
}

#[test]
fn every_route_in_the_router_is_documented_and_nothing_else_is() {
    let code = routes_in_code();
    let doc = routes_in_document();

    assert!(
        !code.is_empty(),
        "no routes extracted from router.rs — the regex no longer matches its shape"
    );

    let undocumented: Vec<_> = code.difference(&doc).collect();
    let phantom: Vec<_> = doc.difference(&code).collect();
    assert!(
        undocumented.is_empty(),
        "routes in router.rs missing from docs/api/openapi.yaml: {undocumented:?}"
    );
    assert!(
        phantom.is_empty(),
        "routes in docs/api/openapi.yaml that router.rs does not register: {phantom:?}"
    );
}

#[test]
fn every_prometheus_metric_is_listed_in_the_readme() {
    let names = metric_names_in_code();
    assert!(
        names.len() >= 10,
        "only {} metric names found in prometheus.rs — extraction is broken: {names:?}",
        names.len()
    );
    let missing: Vec<_> = names
        .iter()
        .filter(|name| !README.contains(&format!("`{name}`")))
        .collect();
    assert!(
        missing.is_empty(),
        "metrics rendered by prometheus.rs but absent from docs/api/README.md: {missing:?}"
    );
}

#[test]
fn the_document_is_openapi_3_1_and_says_the_two_things_that_matter() {
    // The two sentences that stop a reader from making the two classic
    // mistakes: retrying a 401 with variations, and coercing null to zero.
    assert!(OPENAPI.starts_with("openapi: 3.1.0"));
    assert!(
        OPENAPI.contains("A wrong token and a missing token return identical 401s"),
        "the identical-401 rule must be stated in info.description"
    );
    assert!(
        OPENAPI.contains("A field that is null was not measured. It is never zero-when-unknown."),
        "the null-is-unmeasured rule must be stated in info.description"
    );
}
