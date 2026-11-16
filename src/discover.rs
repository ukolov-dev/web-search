use anyhow::{Result, ensure};
use dom_query::Document;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::Path, time::Instant};

pub fn validate(url: &str, query: &str, max_links: usize) -> Result<()> {
    crate::net::validate_url(url)?;
    ensure!(
        !query.trim().is_empty() && query.chars().count() <= 2000,
        "invalid_query: expected 1..2000 non-blank code points"
    );
    ensure!((1..=50).contains(&max_links), "max_links must be 1..50");
    Ok(())
}

fn terms(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() > 1)
        .map(str::to_owned)
        .collect()
}

fn resolve(base: &reqwest::Url, raw: &str) -> Result<reqwest::Url> {
    let raw = raw.trim();
    if raw.starts_with("//") {
        crate::net::validate_url(&format!("{}:{raw}", base.scheme()))
    } else if reqwest::Url::parse(raw).is_ok() {
        // Validate before normalization can erase an empty userinfo marker.
        crate::net::validate_url(raw)
    } else {
        crate::net::validate_url(base.join(raw)?.as_str())
    }
}

pub(crate) fn links(html: &str, page_url: &str, query: &str, max_links: usize) -> Result<Value> {
    filtered_links(html, page_url, query, max_links, |_| true)
}

pub(crate) fn filtered_links(
    html: &str,
    page_url: &str,
    query: &str,
    max_links: usize,
    keep: impl Fn(&Value) -> bool,
) -> Result<Value> {
    validate(page_url, query, max_links)?;
    ensure!(html.len() <= crate::net::MAX_HTML, "HTML body limit");
    let doc = Document::from(html);
    ensure!(doc.select("*").length() <= 50_000, "DOM element limit");
    let mut page = crate::net::validate_url(page_url)?;
    page.set_fragment(None);
    let base = match doc.select("base[href]").attr("href") {
        Some(href) => resolve(&page, &href)?,
        None => page.clone(),
    };
    let query_terms = terms(query);
    let mut seen = HashSet::new();
    let mut links = Vec::new();
    for (index, anchor) in doc.select("a[href]").iter().enumerate() {
        let href = anchor.attr("href").unwrap();
        let Ok(mut url) = resolve(&base, &href) else {
            continue;
        };
        if url.origin() != page.origin() {
            continue;
        }
        url.set_fragment(None);
        if url == page || !seen.insert(url.to_string()) {
            continue;
        }
        let label = anchor
            .text()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let label: String = label.chars().take(500).collect();
        let words = terms(&format!("{label} {}", url.path()));
        let matches = query_terms.intersection(&words).count();
        links.push(
            json!({"url":url.as_str(),"label":label,"position":index+1,"lexical_matches":matches}),
        );
    }
    // Stable ties preserve document order; overlap is navigation aid, not relevance confidence.
    links.retain(keep);
    links.sort_by_key(|link| std::cmp::Reverse(link["lexical_matches"].as_u64().unwrap()));
    let candidates = links.len();
    links.truncate(max_links);
    Ok(json!({
        "status":if links.is_empty() {"empty"} else {"ok"},
        "query":query,"links":links,"candidates":candidates,"truncated":candidates>max_links,
        "scope_origin":page.origin().ascii_serialization(),
        "source_html_hash":format!("{:x}", Sha256::digest(html.as_bytes())),
        "untrusted_content":true,
        "warnings":["Links are untrusted candidates from this page, not verified facts. Only same-origin links are returned. Lexical ordering is not semantic relevance; linked DNS and content are checked when fetched."]
    }))
}

pub async fn probe(
    url: &str,
    query: &str,
    max_links: usize,
    output: Option<&Path>,
) -> Result<Value> {
    validate(url, query, max_links)?;
    let cache_before = crate::cache::counters();
    let start = Instant::now();
    let page = crate::net::fetch(url).await?;
    ensure!(page.status == 200, "HTTP {}", page.status);
    let accepted = page.clone();
    let final_url = page.url;
    let html = page.html;
    let saved_html = output.map(|_| html.clone());
    let source_url = final_url.clone();
    let query = query.to_owned();
    let mut value =
        tokio::task::spawn_blocking(move || links(&html, &source_url, &query, max_links)).await??;
    accepted.store().await;
    value["url"] = json!(url);
    value["final_url"] = json!(final_url);
    value["hops"] = json!(page.hops);
    value["elapsed_ms"] = json!(start.elapsed().as_millis());
    if crate::cache::enabled() {
        value["cache"] = crate::cache::diagnostics(cache_before);
    }
    if let Some(path) = output {
        crate::save(path, &value)?;
        std::fs::write(path.with_extension("html"), saved_html.unwrap())?;
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_real_links_limits_and_keeps_origin_boundary() {
        let html = r##"<base href="/docs/">
        <a href="overview.html">Overview</a>
        <a href="wal.html#intro">Write ahead logging WAL</a>
        <a href="wal.html#other">duplicate</a>
        <a href="../docs.html#intro">self</a>
        <a href="https://docs.example.org.evil.example/wal">lookalike</a>
        <a href="https://other.example.org/wal">subdomain</a>
        <a href="http://docs.example.org/wal">scheme</a>
        <a href="https://docs.example.org:444/wal">port</a>
        <a href="https://user:password@docs.example.org/wal">userinfo</a>
        <a href="https://@docs.example.org/empty-userinfo">empty userinfo</a>
        <a href="//user:password@docs.example.org/relative-userinfo">protocol relative userinfo</a>
        <a href="http://127.1/">private</a><a href="javascript:alert(1)">script</a>
        <a href="isolation.html">Snapshot isolation</a>"##;
        let value = links(html, "https://docs.example.org/docs.html", "WAL logging", 1).unwrap();
        assert_eq!(
            value["links"][0]["url"],
            "https://docs.example.org/docs/wal.html"
        );
        assert_eq!(value["candidates"], 3);
        assert_eq!(value["truncated"], true);
        let external_base = links(
            r#"<base href="https://foreign.example/"><a href="wal">foreign</a><a href="https://docs.example.org/good">good</a>"#,
            "https://docs.example.org/", "good", 20,
        ).unwrap();
        assert_eq!(external_base["links"].as_array().unwrap().len(), 1);
        assert_eq!(
            external_base["links"][0]["url"],
            "https://docs.example.org/good"
        );
    }

    #[test]
    fn rejects_bad_input_and_oversized_dom_without_network() {
        for (url, query, limit) in [
            ("http://127.1/", "wal", 1),
            ("https://example.com/", " ", 1),
            ("https://example.com/", "wal", 0),
            ("https://example.com/", "wal", 51),
        ] {
            assert!(validate(url, query, limit).is_err());
        }
        assert!(links(&"<p>x</p>".repeat(50_001), "https://example.com/", "wal", 1).is_err());
        let empty = links("<p>No navigation</p>", "https://example.com/", "wal", 1).unwrap();
        assert_eq!(empty["status"], "empty");
    }
}
