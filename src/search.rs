use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use dom_query::Document;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub position: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_source: Option<String>,
}

#[derive(Debug)]
pub struct FilteredResults {
    pub results: Vec<SearchResult>,
    pub discarded: usize,
}

pub fn error_code(message: &str) -> &str {
    match message.split(':').next().unwrap_or("") {
        code @ ("invalid_query"
        | "captcha"
        | "parser_changed"
        | "source_challenge"
        | "source_policy_changed"
        | "source_query_mismatch"
        | "source_blocked"
        | "rate_limited"
        | "fetch_timeout") => code,
        "HTTP 202" => "source_challenge",
        _ => "search_failed",
    }
}

pub fn blocks_source(code: &str) -> bool {
    matches!(
        code,
        "captcha"
            | "source_blocked"
            | "rate_limited"
            | "source_challenge"
            | "source_policy_changed"
    )
}

pub fn site_filter(query: &str) -> Result<Option<Url>> {
    ensure!(
        !query.trim().is_empty() && query.chars().count() <= 2000,
        "invalid_query: expected 1..2000 non-blank code points"
    );
    let mut site = None;
    let mut quoted = false;
    let mut has_or = false;
    for token in query.split_whitespace() {
        let inside_quote = quoted || token.starts_with('"');
        quoted ^= token.bytes().filter(|b| *b == b'"').count() % 2 == 1;
        if inside_quote {
            continue;
        }
        // Only explicit uppercase OR is query DSL; ordinary English "or" is lexical text.
        has_or |= token == "OR" || token == "|";
        let plain = token.trim_start_matches('(');
        let lower = plain.to_ascii_lowercase();
        ensure!(
            !lower.starts_with("-site:"),
            "invalid_query: -site is not supported; use one positive site:domain[/path]"
        );
        if lower.starts_with("site:") {
            ensure!(
                site.is_none() && plain == token && !token.contains(['(', ')', '"', '*']),
                "invalid_query: use one unquoted site:domain[/path], without Boolean groups or wildcards"
            );
            let target = &token[5..];
            ensure!(!target.is_empty(), "invalid_query: empty site filter");
            let raw = if target.contains("://") {
                target.to_owned()
            } else {
                format!("https://{target}")
            };
            let url = crate::net::validate_url(&raw)
                .map_err(|e| anyhow::anyhow!("invalid_query: {e}"))?;
            ensure!(
                url.query().is_none() && url.fragment().is_none() && url.port().is_none(),
                "invalid_query: site filter does not support query, fragment or non-default port"
            );
            site = Some(url);
        }
    }
    ensure!(!quoted, "invalid_query: unclosed quote");
    ensure!(
        site.is_none() || !has_or,
        "invalid_query: OR with site is not supported"
    );
    Ok(site)
}

pub fn filter_site(site: Option<&Url>, mut results: Vec<SearchResult>) -> Result<FilteredResults> {
    let original = results.len();
    if let Some(site) = site {
        let host = site.host_str().unwrap().trim_end_matches('.');
        let suffix = format!(".{host}");
        let path = site.path().trim_end_matches('/');
        results.retain(|result| {
            Url::parse(&result.url).is_ok_and(|url| {
                let candidate = url.host_str().unwrap_or("").trim_end_matches('.');
                (candidate == host || candidate.ends_with(&suffix))
                    && (path.is_empty()
                        || url.path() == path
                        || url.path().starts_with(&format!("{path}/")))
            })
        });
        ensure!(
            original == 0 || !results.is_empty(),
            "source_query_mismatch: all {original} results violate site:{host}{path}"
        );
    }
    Ok(FilteredResults {
        discarded: original - results.len(),
        results,
    })
}

pub fn normalize(raw: &str, base: &str) -> Result<String> {
    let mut url = Url::parse(base)?.join(raw)?;
    if url
        .host_str()
        .is_some_and(|h| h == "duckduckgo.com" || h.ends_with(".duckduckgo.com"))
    {
        if let Some(target) = url
            .query_pairs()
            .find(|(k, _)| k == "uddg")
            .map(|(_, v)| v.into_owned())
        {
            url = Url::parse(&target)?;
        }
    } else if url.host_str() == Some("www.bing.com")
        && url.path() == "/ck/a"
        && let Some(encoded) = url
            .query_pairs()
            .find(|(k, _)| k == "u")
            .map(|(_, v)| v.into_owned())
        && let Some(value) = encoded.strip_prefix("a1")
    {
        url = Url::parse(&String::from_utf8(URL_SAFE_NO_PAD.decode(value)?)?)?;
    }
    crate::net::validate_url(url.as_str())?;
    url.set_fragment(None);
    let pairs: Vec<_> = url
        .query_pairs()
        .filter(|(k, _)| {
            !k.starts_with("utm_") && !matches!(k.as_ref(), "fbclid" | "gclid" | "msclkid")
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    url.set_query(None);
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    Ok(url.into())
}

pub fn request_url(engine: &str, query: &str) -> Result<Url> {
    let (base, parameter) = match engine {
        "ddg" => ("https://html.duckduckgo.com/html/", "q"),
        "bing" => ("https://www.bing.com/search", "q"),
        "mwmbl" => ("https://mwmbl.org/search", "q"),
        "seznam" => ("https://search.seznam.cz/", "q"),
        "naver" => ("https://search.naver.com/search.naver?where=web", "query"),
        _ => anyhow::bail!("unknown engine: {engine}"),
    };
    let mut url = Url::parse(base)?;
    url.query_pairs_mut().append_pair(parameter, query);
    Ok(url)
}

pub(crate) fn navigation_guard(html: &str) -> Result<()> {
    let guard = Document::from(html);
    for node in guard.select("iframe,form,input").iter() {
        let widget = ["src", "action", "id", "name", "title"].iter().any(|key| {
            node.attr(key).is_some_and(|v| {
                let v = v.to_lowercase();
                ["captcha", "recaptcha", "hcaptcha", "verify-human"]
                    .iter()
                    .any(|needle| v.contains(needle))
            })
        });
        ensure!(!widget, "source_challenge: verification widget; no retry");
    }
    // Organic text can discuss CAPTCHA; only surrounding verification UI disables a source.
    guard
        .select("a[data-e-a='heading'],a[data-nlog-area$='.link'],.e69e8d,script,style,template")
        .remove();
    // Article headings can quote UI messages; unrelated article nodes must not hide challenge UI.
    for node in guard.select("title,h1,h2,h3,[role='heading']").iter() {
        if !node.ancestors(None).filter("article").is_empty() {
            continue;
        }
        let heading = compact(&node.text()).to_lowercase();
        ensure!(
            !matches!(
                heading.as_str(),
                "verify to continue" | "checking if the site connection is secure"
            ),
            "source_challenge: connection verification page; no retry"
        );
    }
    let text = guard.select("html").text().to_lowercase();
    ensure!(
        ![
            "verify you are human",
            "human verification",
            "access denied",
            "enter the captcha",
            "complete the captcha",
            "client challenge",
            "验证码",
            "자동입력 방지",
            "보안 확인",
            "nejste robot",
            "příliš mnoho požadavků"
        ]
        .iter()
        .any(|needle| text.contains(needle)),
        "source_challenge: verification page; no retry"
    );
    Ok(())
}

fn compact(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn seznam_snippet(anchor: dom_query::Selection<'_>) -> String {
    let mut container = anchor.parent();
    // Bound malformed/deep markup; an unavailable snippet must remain empty.
    for _ in 0..64 {
        if container.length() == 0 {
            break;
        }
        let headings = container.select("a[data-e-a='heading']").length();
        if headings > 1 {
            break;
        }
        let snippets = container.select(".e69e8d");
        if headings == 1 && snippets.length() > 0 {
            return compact(
                &snippets
                    .iter()
                    .map(|node| node.text().to_string())
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
        container = container.parent();
    }
    String::new()
}

pub fn parse(engine: &str, html: &str, base: &str) -> Result<Vec<SearchResult>> {
    parse_inner(engine, html, base, false)
}

pub fn parse_policy(engine: &str, html: &str, base: &str) -> Result<Vec<SearchResult>> {
    ensure!(matches!(engine, "seznam" | "naver"), "policy engine");
    parse_inner(engine, html, base, true)
}

fn parse_inner(
    engine: &str,
    html: &str,
    base: &str,
    preserve_url: bool,
) -> Result<Vec<SearchResult>> {
    let doc = Document::from(html);
    if matches!(engine, "seznam" | "naver") {
        navigation_guard(html)?;
        doc.select("script,style,template,noscript,[aria-hidden='true']")
            .remove();
    }
    ensure!(
        !(doc
            .select("h1")
            .iter()
            .any(|heading| heading.text().trim() == "Wait A Moment")
            && doc.select("#countdown").length() > 0
            && doc.select("body").text().contains("bot activity")),
        "source_challenge: anti-bot waiting page; no retry"
    );
    ensure!(
        doc.select(".anomaly-modal, #b_captcha, form[action*='anomaly'], iframe[src*='captcha']")
            .length()
            == 0,
        "captcha"
    );
    let (blocks, link, snippet) = match engine {
        "ddg" => (
            ".result:not(.result--ad)",
            "a.result__a",
            ".result__snippet",
        ),
        "bing" => ("li.b_algo", "h2 a", ".b_caption p"),
        "seznam" => ("a[data-e-a='heading']", "", ""),
        "naver" => ("a[data-nlog-area$='.link']", "", ""),
        "mwmbl" => (
            "a:has([data-slot=card-title])",
            "[data-slot=card-title]",
            "[data-slot=card-description]",
        ),
        _ => anyhow::bail!("unknown engine"),
    };
    let mut results: Vec<SearchResult> = Vec::new();
    let mut seen = HashSet::new();
    if engine == "mwmbl" {
        ensure!(
            doc.select("#seed-search-switch")
                .attr("aria-checked")
                .is_none_or(|v| v.as_ref() == "false"),
            "source_policy_changed: Seed Search must stay disabled"
        );
    }
    for (index, block) in doc.select(blocks).iter().enumerate() {
        let a = if matches!(engine, "seznam" | "naver") {
            block.clone()
        } else {
            block.select(link)
        };
        let result_source = if engine == "mwmbl" {
            let label = block
                .select("[title='Result source']")
                .text()
                .trim()
                .to_owned();
            ensure!(
                matches!(label.as_str(), "Mwmbl" | "Wikipedia"),
                "source_policy_changed: unsupported result source {label:?}"
            );
            Some(label)
        } else {
            None
        };
        let href = if matches!(engine, "mwmbl" | "seznam" | "naver") {
            block.attr("href")
        } else {
            a.attr("href")
        };
        let Some(href) = href else { continue };
        if matches!(engine, "seznam" | "naver") && !href.starts_with("http") {
            continue;
        }
        let parsed_url = if preserve_url {
            crate::net::validate_url(&href).map(String::from)
        } else {
            normalize(&href, base)
        };
        let Ok(url) = parsed_url else {
            continue;
        };
        let title = if matches!(engine, "seznam" | "naver") {
            compact(&a.text())
        } else {
            a.text().trim().to_owned()
        };
        if matches!(engine, "seznam" | "naver") && title.chars().count() <= 5 {
            continue;
        }
        // Frozen B17 Naver keeps first URL order but the last matching anchor text.
        if engine == "naver"
            && let Some(prior) = results.iter_mut().find(|result| result.url == url)
        {
            prior.title = title.clone();
            prior.snippet = title;
            continue;
        }
        if engine == "naver" && results.len() == 10 {
            continue;
        }
        if title.is_empty() || !seen.insert(url.clone()) {
            continue;
        }
        let snippet = match engine {
            "seznam" => seznam_snippet(a),
            "naver" => title.clone(),
            _ => block.select(snippet).text().trim().to_owned(),
        };
        results.push(SearchResult {
            title,
            url,
            snippet,
            position: index + 1,
            result_source,
        });
        if results.len() == 10 && engine != "naver" {
            break;
        }
    }
    results.truncate(10);
    if results.is_empty() {
        let text = doc.select("body").text().to_lowercase();
        ensure!(
            doc.select(".no-results, .b_no").length() > 0
                || text.contains("no results found")
                || text.contains("there are no results")
                || text.contains("ничего не найдено"),
            "parser_changed"
        );
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sources_keep_snippets_with_their_organic_results() {
        let html = r#"<html><body><div><article><h3><a data-e-a="heading" href="https://example.org/one">Captcha handling reference</a></h3><p class="e69e8d">verify you are human — discussed by this result</p></article><article><h3><a data-e-a="heading" href="https://example.org/two">Second reference</a></h3><p class="e69e8d">Other snippet</p></article><a href="https://ads.example.org/">Advertisement</a></div></body></html>"#;
        let results = parse("seznam", html, "https://search.seznam.cz/").unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0].snippet,
            "verify you are human — discussed by this result"
        );
        assert_eq!(results[1].snippet, "Other snippet");
        let missing = r#"<div><article><a data-e-a="heading" href="https://example.org/one">Missing snippet</a></article><article><a data-e-a="heading" href="https://example.org/two">Second result</a><p class="e69e8d">Must not leak</p></article></div>"#;
        assert_eq!(
            parse("seznam", missing, "https://search.seznam.cz/").unwrap()[0].snippet,
            ""
        );
        let naver = r#"<a data-nlog-area="web.link" href="https://example.org/a">验证码 handling reference</a><a data-nlog-area="ad.link-ad" href="https://example.org/ad">Advertisement</a><a data-nlog-area="web.link" href="/internal">Provider navigation</a>"#;
        let results = parse("naver", naver, "https://search.naver.com/").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, results[0].snippet);
        let repeated = r#"<a data-nlog-area="web.link" href="https://example.org/first">First headline</a><a data-nlog-area="web.link" href="https://example.org/second">Second headline</a><a data-nlog-area="web.link" href="https://example.org/first">Last actual description</a>"#;
        let results = parse("naver", repeated, "https://search.naver.com/").unwrap();
        assert_eq!(results[0].url, "https://example.org/first");
        assert_eq!(results[0].snippet, "Last actual description");
        assert_eq!(results[0].position, 1);
        let mut many: String = (0..12)
            .map(|i| {
                format!(
                    "<a data-nlog-area='web.link' href='https://example.org/{i}'>Headline {i}</a>"
                )
            })
            .collect();
        many.push_str("<a data-nlog-area='web.link' href='https://example.org/0'>Last description after first ten</a>");
        let results = parse("naver", &many, "https://search.naver.com/").unwrap();
        assert_eq!(results.len(), 10);
        assert_eq!(results[0].snippet, "Last description after first ten");
        for engine in ["seznam", "naver"] {
            for html in [
                "<h1>Verify you are human</h1>",
                "<iframe src='https://example.org/recaptcha'></iframe>",
            ] {
                let error = parse(engine, html, "https://example.org/")
                    .unwrap_err()
                    .to_string();
                assert!(blocks_source(error_code(&error)), "{error}");
            }
            assert!(
                parse(engine, "<h1>Unknown layout</h1>", "https://example.org/")
                    .unwrap_err()
                    .to_string()
                    .contains("parser_changed")
            );
        }
        let url = request_url("naver", "site:example.org ошибки & WAL").unwrap();
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(pairs["where"], "web");
        assert_eq!(pairs["query"], "site:example.org ошибки & WAL");
        assert!(request_url("unknown", "rust").is_err());
    }

    #[test]
    #[ignore = "offline B17 replay; requires explicit WEB_SEARCH_RESEARCH_B17_DIR, no network"]
    fn replay_b17_source_captures() -> Result<()> {
        use sha2::{Digest, Sha256};
        let root = std::path::PathBuf::from(std::env::var("WEB_SEARCH_RESEARCH_B17_DIR")?);
        let import: serde_json::Value =
            serde_json::from_str(include_str!("../feasibility/b17-import-2026-10-29.json"))?;
        for (path, expected) in import["frozen_files_sha256"].as_object().unwrap() {
            assert_eq!(
                format!("{:x}", Sha256::digest(std::fs::read(root.join(path))?)),
                expected.as_str().unwrap(),
                "frozen file {path}"
            );
        }
        let enriched = std::fs::read(root.join("fresh/enriched.json"))?;
        assert_eq!(
            format!("{:x}", Sha256::digest(&enriched)),
            import["expected_enriched_sha256"].as_str().unwrap()
        );
        let rows: Vec<serde_json::Value> = serde_json::from_slice(&enriched)?;
        let mut comparisons = Vec::new();
        for row in rows {
            let id = row["request_id"].as_str().unwrap();
            let raw = std::fs::read(root.join("fresh").join(format!("{id}.html")))?;
            let expected_hash = import["captures"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["request_id"] == id)
                .unwrap()["html_sha256"]
                .as_str()
                .unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(&raw)),
                expected_hash,
                "capture {id}"
            );
            let engine = row["engine"].as_str().unwrap();
            let base = row["url"].as_str().unwrap();
            let expected = row["results"].as_array().unwrap();
            let actual = match parse(engine, std::str::from_utf8(&raw)?, base) {
                Ok(results) => results,
                Err(error) if expected.is_empty() && error.to_string() == "parser_changed" => {
                    comparisons.push(serde_json::json!({"id":id,"engine":engine,"results":0,"order_matches":true,"snippet_differences":[],"native_error":"parser_changed","interpretation":"Upstream extracted no recognized organic links; native refuses to call unknown markup an empty web result."}));
                    continue;
                }
                Err(error) => anyhow::bail!("{id} {engine}: {error}"),
            };
            let mut seen = HashSet::new();
            let expected_urls: Vec<_> = expected
                .iter()
                .filter_map(|r| normalize(r["url"].as_str().unwrap(), base).ok())
                .filter(|u| seen.insert(u.clone()))
                .collect();
            let actual_urls: Vec<_> = actual.iter().map(|r| r.url.clone()).collect();
            let mut snippet_differences = Vec::new();
            for result in &actual {
                if let Some(original) = expected.iter().find(|r| {
                    normalize(r["url"].as_str().unwrap(), base).is_ok_and(|u| u == result.url)
                }) {
                    let title = compact(original["title"].as_str().unwrap());
                    let snippet = compact(original["snippet"].as_str().unwrap());
                    if result.title != title || result.snippet != snippet {
                        snippet_differences.push(serde_json::json!({"url":result.url,"title":result.title,"expected_title":title,"snippet":result.snippet,"expected_snippet":snippet}));
                    }
                }
            }
            comparisons.push(serde_json::json!({"id":id,"engine":engine,"results":actual.len(),"order_matches":actual_urls==expected_urls,"snippet_differences":snippet_differences}));
        }
        let passed = comparisons.iter().all(|r| {
            r["order_matches"] == true && r["snippet_differences"].as_array().unwrap().is_empty()
        });
        crate::save(
            std::path::Path::new("runs/native-search-2026-10-29/parser-replay.json"),
            &serde_json::json!({"network_calls":0,"model_calls":0,"compared":comparisons.len(),"passed":passed,"comparisons":comparisons}),
        )?;
        assert!(passed, "parser differences recorded in parser-replay.json");
        Ok(())
    }

    #[test]
    fn site_scope_never_returns_foreign_or_lookalike_domains() {
        let html = r#"<li class="b_algo"><h2><a href="https://doc.rust-lang.org/book/ch04.html">Ownership</a></h2></li>
        <li class="b_algo"><h2><a href="https://doc.rust-lang.org.evil.example/book/">Lookalike</a></h2></li>
        <li class="b_algo"><h2><a href="https://en.wikipedia.org/wiki/Ownership">Foreign</a></h2></li>"#;
        let results = parse("bing", html, "https://www.bing.com/search").unwrap();
        let site = Url::parse("https://doc.rust-lang.org/").unwrap();
        let filtered = filter_site(Some(&site), results).unwrap();
        assert_eq!(
            filtered.results.len(),
            1,
            "site query leaked foreign domains"
        );
        assert_eq!(filtered.discarded, 2);
        assert_eq!(
            filtered.results[0].url,
            "https://doc.rust-lang.org/book/ch04.html"
        );
    }

    #[test]
    fn natural_or_keeps_site_boundary_and_explicit_boolean_is_rejected() {
        let question = "site:docs.python.org/3.11/library/tomllib.html Must load receive a binary or text file?";
        let site = site_filter(question).unwrap().unwrap();
        assert_eq!(site.path(), "/3.11/library/tomllib.html");
        let input: crate::research::Input =
            serde_json::from_value(serde_json::json!({"question":question})).unwrap();
        input.validate().unwrap();
        for operator in ["OR", "|"] {
            assert!(
                site_filter(&format!(
                    "site:docs.python.org/3.11/library/tomllib.html binary {operator} text"
                ))
                .is_err()
            );
        }
        let rows = [
            "https://docs.python.org/3.11/library/tomllib.html",
            "https://docs.python.org/3.12/library/tomllib.html",
            "https://docs.python.org/3.11/library/pathlib.html",
            "https://evil.example/3.11/library/tomllib.html",
        ]
        .into_iter()
        .enumerate()
        .map(|(i, url)| SearchResult {
            title: String::new(),
            url: url.into(),
            snippet: String::new(),
            position: i + 1,
            result_source: None,
        })
        .collect();
        let kept = filter_site(Some(&site), rows).unwrap();
        assert_eq!(kept.results.len(), 1);
        assert_eq!(
            kept.results[0].url,
            "https://docs.python.org/3.11/library/tomllib.html"
        );
    }

    #[test]
    fn site_syntax_path_boundaries_and_total_mismatch() {
        for query in [
            "site:",
            "site:localhost",
            "site:a.example site:b.example",
            "site:a.example OR Rust",
            "-site:a.example Rust",
            "site:*.example.com Rust",
            "site:a.example?q=x",
            "site:user:pass@a.example",
            "(site:a.example)",
            "site:a.example \"unclosed",
        ] {
            assert!(site_filter(query).is_err(), "accepted {query}");
        }
        assert!(
            site_filter("\"site:a.example\" as literal text")
                .unwrap()
                .is_none()
        );
        let site = site_filter("ownership SITE:Doc.Rust-Lang.Org/book")
            .unwrap()
            .unwrap();
        let results = [
            "https://doc.rust-lang.org/book",
            "https://sub.doc.rust-lang.org/book/part",
            "https://doc.rust-lang.org/bookish",
            "https://notdoc.rust-lang.org/book",
            "https://doc.rust-lang.org.evil.example/book",
        ]
        .into_iter()
        .enumerate()
        .map(|(i, url)| SearchResult {
            title: "page".into(),
            url: url.into(),
            snippet: String::new(),
            position: i + 1,
            result_source: None,
        })
        .collect();
        let filtered = filter_site(Some(&site), results).unwrap();
        assert_eq!(filtered.results.len(), 2);
        assert_eq!(filtered.discarded, 3);
        assert_eq!(filtered.results[1].position, 2);
        let wrong = vec![SearchResult {
            title: "Foreign".into(),
            url: "https://foreign.example/".into(),
            snippet: String::new(),
            position: 1,
            result_source: None,
        }];
        assert!(
            filter_site(Some(&site), wrong)
                .unwrap_err()
                .to_string()
                .starts_with("source_query_mismatch:")
        );
        assert!(filter_site(Some(&site), vec![]).unwrap().results.is_empty());
    }

    #[test]
    fn parses_mwmbl_own_index_and_stops_on_challenges_or_policy_change() {
        let html = r#"<html><body><button id="seed-search-switch" aria-checked="false"></button>
        <a href="https://footer.example">Footer</a><a href="https://docs.example/guide"><div data-slot="card"><span title="Result source">Mwmbl</span><div data-slot="card-title">Rust <strong>Ownership</strong></div><p data-slot="card-description">Move and borrow</p></div></a></body></html>"#;
        let results = parse("mwmbl", html, "https://mwmbl.org/search").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Rust Ownership");
        assert_eq!(results[0].snippet, "Move and borrow");
        assert_eq!(results[0].result_source.as_deref(), Some("Mwmbl"));
        assert_eq!(
            parse(
                "mwmbl",
                &html.replace(">Mwmbl<", ">Wikipedia<"),
                "https://mwmbl.org/search"
            )
            .unwrap()[0]
                .result_source
                .as_deref(),
            Some("Wikipedia")
        );
        assert!(
            parse(
                "mwmbl",
                &html.replace("aria-checked=\"false\"", "aria-checked=\"true\""),
                "https://mwmbl.org/search"
            )
            .unwrap_err()
            .to_string()
            .starts_with("source_policy_changed:")
        );
        assert!(
            parse(
                "mwmbl",
                &html.replace(">Mwmbl<", ">External provider<"),
                "https://mwmbl.org/search"
            )
            .is_err()
        );
        let challenge = "<body><h1>Search engine</h1><h1>Wait A Moment</h1><span id='countdown'>2</span>aggressive bot activity</body>";
        assert!(
            parse("bing", challenge, "https://example.com")
                .unwrap_err()
                .to_string()
                .starts_with("source_challenge:")
        );
        assert!(
            parse(
                "mwmbl",
                "<body>New layout</body>",
                "https://mwmbl.org/search"
            )
            .is_err()
        );
    }

    #[test]
    fn removes_tracking_preserves_content_and_page_identity() {
        let url=normalize("//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa%3Fpage%3D2%26utm_source%3Dx%23h", "https://html.duckduckgo.com/").unwrap();
        assert_eq!(url, "https://example.com/a?page=2");
        assert_ne!(
            url,
            normalize("https://example.com/b?page=2", "https://example.com").unwrap()
        );
    }

    #[test]
    fn decodes_bing_redirect_and_rejects_private_target() {
        let target = URL_SAFE_NO_PAD.encode("https://example.com/doc?page=2&utm_source=bing");
        assert_eq!(
            normalize(&format!("/ck/a?u=a1{target}"), "https://www.bing.com").unwrap(),
            "https://example.com/doc?page=2"
        );
        assert!(
            normalize(
                "https://duckduckgo.com/l/?uddg=http%3A%2F%2F127.0.0.1%2F",
                "https://duckduckgo.com"
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_connection_verification_ui_but_allows_article_discussion() {
        let captured = include_str!("../tests/fixtures/research/connection-challenge.html");
        for html in [
            captured,
            "<html><body><h2> Checking if the site connection is secure </h2></body></html>",
            "<html><body><article></article><h2>Checking if the site connection is secure</h2></body></html>",
            "<html><head><title>Verify to continue</title></head><body>Please wait.</body></html>",
        ] {
            assert_eq!(
                error_code(&navigation_guard(html).unwrap_err().to_string()),
                "source_challenge"
            );
        }
        for html in [
            "<article><h1>Connection verification explained</h1><p>Checking if the site connection is secure is a challenge message.</p></article>",
            "<html><head><title>Verification guide</title></head><body><article><h1>Security guide</h1><h2>Verify to continue</h2><p>This section explains the UI message.</p></article></body></html>",
            "<article><h1>What does Verify to continue mean?</h1><p>A guide to verification.</p></article>",
            "<script>const message = 'Verify to continue';</script><article><h1>Ordinary content</h1></article>",
        ] {
            navigation_guard(html).unwrap();
        }
    }

    #[test]
    fn distinguishes_results_ads_empty_challenge_and_markup_change() {
        let html = r#"<div class="result result--ad"><a class="result__a" href="https://ad.example/">Ad</a></div><div class="result"><a class="result__a" href="https://example.com/a">Title</a><p class="result__snippet">Evidence</p></div>"#;
        let results = parse("ddg", html, "https://html.duckduckgo.com").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].snippet, "Evidence");
        assert!(
            parse(
                "ddg",
                "<div class='no-results'>None</div>",
                "https://example.com"
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            parse(
                "ddg",
                "<div class='anomaly-modal'>challenge</div>",
                "https://example.com"
            )
            .unwrap_err()
            .to_string()
            .contains("captcha")
        );
        assert!(parse("bing", "<h1>New layout</h1>", "https://example.com").is_err());
    }
}
