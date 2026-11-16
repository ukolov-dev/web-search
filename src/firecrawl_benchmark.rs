//! Explicit cached-source component A/B; no extra reader is published by the product.
use crate::{
    evidence,
    model::Model,
    news,
    policy::{Candidate, Case, Classifications},
    rescue,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn root() -> PathBuf {
    PathBuf::from(
        std::env::var("WEB_SEARCH_FIRECRAWL_ROOT")
            .unwrap_or_else(|_| "runs/firecrawl-2026-11-05".into()),
    )
}
fn load(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}
fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn multipart(html: &str, id: &str) -> Result<(String, Vec<u8>)> {
    ensure!(
        !id.is_empty() && id.bytes().all(|c| c.is_ascii_alphanumeric()),
        "upload ID"
    );
    let boundary = format!("websearch{}", hash(html));
    ensure!(!html.contains(&boundary), "multipart boundary collision");
    let options = json!({"formats":["markdown","html","rawHtml"],"onlyMainContent":true,"timeout":30000,"parsers":[]});
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{id}.html\"\r\nContent-Type: text/html; charset=utf-8\r\n\r\n{html}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"options\"\r\n\r\n{options}\r\n--{boundary}--\r\n"
    );
    Ok((boundary, body.into_bytes()))
}

async fn parse(html: &str, id: &str) -> Result<Value> {
    let runtime = load(&root().join("runtime.json"))?;
    ensure!(
        runtime["internal_network"] == true,
        "isolated worker required"
    );
    let endpoint = reqwest::Url::parse(runtime["endpoint"].as_str().context("endpoint")?)?;
    let ip: std::net::Ipv4Addr = endpoint.host_str().context("host")?.parse()?;
    ensure!(
        endpoint.scheme() == "http"
            && ip.is_private()
            && endpoint.host_str() == runtime["container_ip"].as_str()
            && endpoint.port() == Some(3002)
            && endpoint.path() == "/v2/parse",
        "trusted private experiment endpoint"
    );
    let (boundary, bytes) = multipart(html, id)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(45))
        .build()?;
    let started = Instant::now();
    let response = client
        .post(endpoint)
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(bytes)
        .send()
        .await?;
    let status = response.status().as_u16();
    let raw = response.text().await?;
    ensure!(raw.len() <= 20 * 1024 * 1024, "local response bound");
    let value =
        serde_json::from_str::<Value>(&raw).unwrap_or_else(|_| json!({"unparsed_body":raw}));
    Ok(
        json!({"http_status":status,"elapsed_us":started.elapsed().as_micros(),"input_sha256":hash(html),"response":value}),
    )
}

fn from_firecrawl(call: &Value, html: &str, url: &str) -> Result<Value> {
    ensure!(
        call["http_status"] == 200 && call["response"]["success"] == true,
        "Firecrawl parse failed: {}",
        call["response"]
    );
    let data = &call["response"]["data"];
    let raw = data["rawHtml"].as_str().context("Firecrawl rawHtml")?;
    ensure!(
        hash(raw) == hash(html),
        "returned rawHtml does not preserve the supplied source bytes"
    );
    let cleaned =
        dom_query::Document::from(data["html"].as_str().context("Firecrawl cleaned HTML")?);
    cleaned.select("script,style,noscript,template,nav,aside,footer,button,[hidden],[aria-hidden=true],.sr-only").remove();
    // Our evidence client consumes plain text; reuse its serializer instead of Markdown escapes.
    let body = cleaned.formatted_text().to_string();
    let truncated = body.chars().count() > 100_000;
    let body: String = body.chars().take(100_000).collect();
    let original = dom_query::Document::from(raw)
        .select("head title")
        .text()
        .trim()
        .to_owned();
    // The /parse synthetic upload URL is not a fetched source URL; bind returned bytes to the frozen snapshot URL.
    Ok(
        json!({"title":original,"page_title":original,"text_hash":hash(&body),"chars":body.chars().count(),"text":body,"truncated":truncated,"extraction_method":"firecrawl_uploaded_html_main_content","content_scope":"provider_main_content","coverage":"not_verified","news":news::metadata(raw,url),"source_provenance":"same uploaded snapshot bytes; original URL from frozen manifest, not live redirect evidence"}),
    )
}

#[test]
fn upload_and_returned_source_are_bound() {
    assert!(multipart("hello", "bad\r\nID").is_err());
    let html = "<html><head><title>Example</title></head><body>Actual source body</body></html>";
    let call = json!({"http_status":200,"response":{"success":true,"data":{"rawHtml":html,"html":"<p>Actual source body</p>","markdown":"Actual source body"}}});
    let doc = from_firecrawl(&call, html, "https://example.com/article").unwrap();
    assert_eq!(doc["page_title"], "Example");
    assert!(from_firecrawl(&call, "different source", "https://example.com/article").is_err());
}

#[tokio::test]
#[ignore = "explicit local Firecrawl HTML-upload smoke; no target HTTP, search, model or browser"]
async fn local_parse_smoke() -> Result<()> {
    let html = "<!doctype html><html><head><title>Firecrawl local smoke</title></head><body><main><h1>Local parse smoke</h1><p>This controlled document checks the real upstream HTML upload endpoint. The supplied text is processed locally without loading any external website. SOURCE_WITNESS_LOCAL_20261105 must remain in the returned Markdown as a real source fragment.</p><p>Only deterministic Markdown, HTML and raw HTML formats are requested. No model, browser, image parser or external search provider is configured for this experiment.</p></main></body></html>";
    let call = parse(html, "SMOKE").await?;
    crate::save(&root().join("local-parse-smoke.json"), &call)?;
    let doc = from_firecrawl(&call, html, "https://example.com/local-smoke")?;
    ensure!(
        doc["text"]
            .as_str()
            .context("body")?
            .contains("SOURCE_WITNESS_LOCAL_20261105"),
        "upstream smoke witness lost"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit 12 saved pages x3 reader timings; local Firecrawl upload, no target HTTP/search/models/browser"]
async fn capture_paired_readers() -> Result<()> {
    let manifest = load(&root().join("cases.json"))?;
    let capture =
        std::env::var("WEB_SEARCH_FIRECRAWL_CAPTURE").unwrap_or_else(|_| "reader-capture".into());
    ensure!(
        matches!(capture.as_str(), "reader-capture" | "latency-capture"),
        "fixed capture directory"
    );
    let output = root().join(capture);
    std::fs::create_dir(&output)?;
    let mut rows = Vec::new();
    let pages = manifest["pages"].as_array().context("pages")?;
    ensure!(pages.len() == 12, "fixed twelve pages");
    for round in 0..3usize {
        for (i, page) in pages.iter().enumerate() {
            let id = page["page_id"].as_str().context("page ID")?;
            let html =
                std::fs::read_to_string(root().join(page["html_path"].as_str().context("HTML")?))?;
            ensure!(hash(&html) == page["html_sha256"], "frozen source hash");
            let url = page["url"].as_str().context("URL")?;
            crate::net::validate_url(url)?;
            for arm in if (i + round) % 2 == 0 {
                ["baseline", "firecrawl"]
            } else {
                ["firecrawl", "baseline"]
            } {
                let prefix = output.join(format!("{id}-r{round}-{arm}"));
                let started = Instant::now();
                let mut recorded_call = None;
                let result = if arm == "baseline" {
                    crate::extract(html.clone(), url)
                } else {
                    match parse(&html, id).await {
                        Ok(call) => {
                            let document = from_firecrawl(&call, &html, url);
                            recorded_call = Some(call);
                            document
                        }
                        Err(e) => Err(e),
                    }
                };
                let elapsed_us = started.elapsed().as_micros();
                if let Some(call) = recorded_call {
                    crate::save(&prefix.with_extension("call.json"), &call)?;
                }
                let row = match result {
                    Ok(doc) => {
                        crate::save(&prefix.with_extension("document.json"), &doc)?;
                        json!({"page_id":id,"round":round,"arm":arm,"elapsed_us":elapsed_us,"document":prefix.with_extension("document.json").file_name().unwrap().to_string_lossy(),"error":null})
                    }
                    Err(e) => {
                        json!({"page_id":id,"round":round,"arm":arm,"elapsed_us":elapsed_us,"document":null,"error":e.to_string()})
                    }
                };
                rows.push(row);
                crate::save(
                    &output.join("timings.json"),
                    &json!({"rows":rows,"target_requests":0,"search_calls":0,"model_calls":0,"reader_mode":"uploaded HTML, no JS execution"}),
                )?;
            }
        }
    }
    Ok(())
}

#[tokio::test]
#[ignore = "explicit frozen12 source-reader A/B with current Qwen and max16 OpenRouter calls, no search or target HTTP"]
async fn compare_final_direct() -> Result<()> {
    let manifest = load(&root().join("cases.json"))?;
    let variant = std::env::var("WEB_SEARCH_SELECTOR_VARIANT").ok();
    ensure!(
        variant
            .as_deref()
            .is_none_or(|v| matches!(v, "baseline" | "fixed")),
        "fixed selector experiment variant"
    );
    ensure!(
        (manifest["comparison"] == "qwen-selector") == variant.is_some(),
        "selector comparison requires an explicit binary variant"
    );
    let group_filter = std::env::var("WEB_SEARCH_SELECTOR_GROUP").ok();
    if let Some(group) = &group_filter {
        ensure!(
            variant.is_some()
                && manifest["groups"]
                    .as_array()
                    .context("groups")?
                    .iter()
                    .any(|v| v["case"]["id"] == *group),
            "known selector experiment group"
        );
    }
    let output = root().join("evidence-capture");
    std::fs::create_dir_all(&output)?;
    let model = Model::from_openrouter(
        Path::new(&std::env::var("WEB_SEARCH_OPENROUTER_KEY_FILE")?),
        "deepseek/deepseek-v4.1-flash-1017",
        std::env::var("WEB_SEARCH_OPENROUTER_PROXY").ok().as_deref(),
    )?
    .with_reranker(
        Some(Path::new(&std::env::var("WEB_SEARCH_OMLX_SETTINGS")?)),
        true,
    )?;
    model.check().await?;
    model
        .reranker
        .as_ref()
        .context("Qwen required")?
        .check()
        .await?;
    for (ordinal, group) in manifest["groups"]
        .as_array()
        .context("groups")?
        .iter()
        .enumerate()
    {
        let case: Case = serde_json::from_value(group["case"].clone())?;
        if group_filter.as_ref().is_some_and(|id| id != &case.id) {
            continue;
        }
        let pages: Vec<_> = manifest["pages"]
            .as_array()
            .context("pages")?
            .iter()
            .filter(|p| p["group"] == case.id)
            .collect();
        ensure!(pages.len() == 3, "three frozen candidates per group");
        let candidates: Vec<Candidate> = pages
            .iter()
            .map(|p| Candidate {
                id: p["id"].as_str().unwrap().into(),
                url: p["url"].as_str().unwrap().into(),
                title: p["title"].as_str().unwrap().into(),
                snippet: String::new(),
                document_http_status: None,
                fetch_method: "frozen-source-reader-component".into(),
            })
            .collect();
        let rank: HashMap<String, usize> = candidates
            .iter()
            .enumerate()
            .map(|(i, c)| Ok((crate::policy::canonical(&c.url, true)?, i)))
            .collect::<Result<_>>()?;
        let arms = if let Some(variant) = &variant {
            vec![(variant.as_str(), "firecrawl")]
        } else if ordinal % 2 == 0 {
            vec![("baseline", "baseline"), ("firecrawl", "firecrawl")]
        } else {
            vec![("firecrawl", "firecrawl"), ("baseline", "baseline")]
        };
        for (arm, reader_arm) in arms {
            let dir = output.join(format!("{}-{arm}", case.id));
            std::fs::create_dir(&dir)?;
            let started = Instant::now();
            let mut labels = Classifications::default();
            labels
                .kinds
                .extend(candidates.iter().map(|c| (c.id.clone(), 1)));
            let mut records = Vec::new();
            let mut documents = HashMap::new();
            for p in &pages {
                let pid = p["page_id"].as_str().unwrap();
                let path = root()
                    .join("reader-capture")
                    .join(format!("{pid}-r0-{reader_arm}.document.json"));
                if !path.exists() {
                    records.push(json!({"id":p["id"],"url":p["url"],"error":"reader failed; source not repaired from another arm"}));
                    continue;
                }
                let mut doc = load(&path)?;
                if !group["window"].is_null() {
                    doc["news"]["request_window"] = group["window"].clone();
                }
                let date = if group["window"].is_null() {
                    Value::Null
                } else {
                    news::check_date(&doc["news"], &group["window"])
                };
                records.push(json!({"id":p["id"],"url":p["url"],"final_url":p["url"],"http_status":200,"text_hash":doc["text_hash"],"news":{"metadata":doc["news"],"date_check":date},"provenance":"preselected snapshot replay, not a live target response"}));
                documents.insert(p["id"].as_str().unwrap().to_string(), doc);
            }
            let eligible: Vec<_> = candidates
                .iter()
                .filter(|c| {
                    group["window"].is_null() && documents.contains_key(&c.id)
                        || records
                            .iter()
                            .any(|d| d["id"] == c.id && d["news"]["date_check"]["eligible"] == true)
                })
                .cloned()
                .collect();
            let mut report = evidence::run_with_reader(
                rescue::Input {
                    case: &case,
                    candidates: &eligible,
                    rank: &rank,
                    started,
                    output: Some(&dir),
                },
                &mut labels,
                &model,
                &mut HashMap::new(),
                2,
                |c| {
                    let record = records.iter().find(|r| r["id"] == c.id).unwrap().clone();
                    let page = documents
                        .get(&c.id)
                        .and_then(|d| rescue::page_from_document(&c, &c.url, d, &case.query).ok());
                    async move { (record, page) }
                },
            )
            .await;
            if !group["window"].is_null() {
                crate::news_search::Prepared {
                    candidates: candidates.clone(),
                    documents: records.clone(),
                    cached: HashMap::new(),
                    trace: Value::Null,
                    reads: pages.len(),
                }
                .enforce(&mut labels, &mut report);
            }
            ensure!(
                report.calls.len() <= 2 && report.reranker_calls.len() <= 3,
                "existing call budgets"
            );
            crate::save(
                &dir.join("result.json"),
                &json!({"scope":"fixed-URL source-injected component; initial labels ambiguous, not full retrieval or initial classifier","arm":arm,"reader_arm":reader_arm,"case":case,"evidence_version":evidence::VERSION,"labels":labels.kinds,"proofs":report.proofs,"rejected":report.rejected,"errors":report.errors,"memory":report.memory,"documents":records,"model_calls":report.calls,"reranker_calls":report.reranker_calls,"verification_elapsed_ms":started.elapsed().as_millis(),"search_calls":0,"fresh_target_http":0}),
            )?;
            ensure!(
                !report
                    .calls
                    .iter()
                    .any(|c| c.to_string().contains("HTTP 403")
                        || c.to_string().contains("HTTP 429")),
                "model refusal: stop whole cohort, no retry"
            );
        }
    }
    Ok(())
}
