//! Named opt-in experiments; ordinary tests never access external services.
use crate::{
    model::Model,
    policy::{self, Candidate, Case},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};
const ROOT: &str = "runs/classifier-relation-2026-10-30";
fn root() -> PathBuf {
    std::env::var_os("WEB_SEARCH_EXPERIMENT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(ROOT))
}
fn remote() -> Result<Model> {
    Model::from_openrouter(
        Path::new(&std::env::var("WEB_SEARCH_OPENROUTER_KEY_FILE")?),
        "deepseek/deepseek-v4.1-flash-1017",
        std::env::var("WEB_SEARCH_OPENROUTER_PROXY").ok().as_deref(),
    )
}
#[tokio::test]
#[ignore = "explicit saved-context completion regression; up to7generations, no search/page network"]
async fn initial_completion_replay() -> Result<()> {
    let corpus: Value = serde_json::from_slice(&std::fs::read(
        "feasibility/reranker-corpus-2026-10-30.json",
    )?)?;
    let model = remote()?;
    model.check().await?;
    let out = root().join(
        std::env::var("WEB_SEARCH_REGRESSION_PHASE").unwrap_or_else(|_| "initial-green".into()),
    );
    std::fs::create_dir(&out)?;
    for row in corpus["sets"]["full"]["capture"]["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["initial_valid"] == false)
    {
        let old = &row["initial_calls"][0];
        let context = old["input"]["messages"][1]["content"]
            .as_str()
            .context("old input")?;
        let input: Value = serde_json::from_str(context)?;
        let candidates: Vec<Candidate> = serde_json::from_value(input["candidates"].clone())?;
        let ids = candidates.iter().map(|c| c.id.clone()).collect::<Vec<_>>();
        let call = model
            .classify(context, &ids, Duration::from_secs(90))
            .await?;
        crate::save(
            &out.join(format!("{}.json", row["id"].as_str().unwrap())),
            &call,
        )?;
        let labels = policy::labels(
            &call["response"],
            &candidates,
            row["query"].as_str().unwrap(),
        )?;
        ensure!(
            labels.kinds.len() == ids.len() && call["warning"].is_null(),
            "all IDs and clean generation"
        );
        println!(
            "{} completed with {}IDs, reasoning_tokens={}",
            row["id"],
            ids.len(),
            call["response"]["usage"]["completion_tokens_details"]["reasoning_tokens"]
        );
    }
    Ok(())
}
#[tokio::test]
#[ignore = "explicit new12 SERP capture only;24 logical safe searches, no model"]
async fn capture_new_search_pool() -> Result<()> {
    let root = root();
    let out = root.join("retrieval");
    std::fs::create_dir(&out)?;
    let plan: Value = serde_json::from_slice(&std::fs::read(
        std::env::var("WEB_SEARCH_CASES")
            .unwrap_or_else(|_| "feasibility/classifier-relation-cases-v1.json".into()),
    )?)?;
    let mut session = policy::Session::new();
    let mut blocked = HashMap::new();
    let mut rows = Vec::new();
    for row in plan["cases"].as_array().unwrap() {
        let case: Case = serde_json::from_value(row.clone())?;
        let mut sources = Vec::new();
        for engine in ["seznam", "naver"] {
            sources.push(
                session
                    .source(engine, &case.query, &mut blocked, Some(&out))
                    .await?,
            );
        }
        let (candidates, rank) = policy::export(&case, &sources)?;
        rows.push(json!({"case":case,"sources":sources,"candidates":candidates,"rank":rank}));
        crate::save(
            &out.join("corpus.json"),
            &json!({"cases":rows,"blocked_sources":blocked,"source_attempts":session.requests}),
        )?;
        println!("captured {}", case.id);
    }
    Ok(())
}

/// Shared URL-versioned snapshots, including failures. Never retry a blocked host.
pub async fn cached_fetch(raw: &str, root: &Path) -> Result<(crate::net::Page, bool)> {
    cached_fetch_mode(
        raw,
        root,
        std::env::var_os("WEB_SEARCH_RAW_CACHE_ONLY").is_some(),
    )
    .await
}

async fn cached_fetch_mode(
    raw: &str,
    root: &Path,
    cache_only: bool,
) -> Result<(crate::net::Page, bool)> {
    use sha2::{Digest, Sha256};
    let requested = crate::net::validate_url(raw)?;
    std::fs::create_dir_all(root)?;
    let path = root.join(format!("{:x}.json", Sha256::digest(raw.as_bytes())));
    let hit = path.exists();
    if !hit {
        ensure!(!cache_only, "cache_only_miss: {raw}");
        let blocks: HashMap<String, String> =
            serde_json::from_slice(&std::fs::read(root.join("blocks.json"))?)?;
        if let Some(reason) = blocks.get(requested.host_str().unwrap()) {
            anyhow::bail!("host_disabled: {reason}");
        }
        let saved = match crate::net::fetch(raw).await {
            Ok(page) => {
                json!({"schema":"raw-document-v1","url":raw,"fetched":{"url":page.url,"status":page.status,"hops":page.hops,"html":page.html},"html_hash":format!("{:x}",Sha256::digest(page.html.as_bytes())),"error":null})
            }
            Err(error) => {
                json!({"schema":"raw-document-v1","url":raw,"fetched":null,"html_hash":null,"error":error.to_string()})
            }
        };
        crate::save(&path, &saved)?;
    }
    let saved: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    ensure!(
        saved["schema"] == "raw-document-v1" && saved["url"] == raw,
        "raw snapshot identity"
    );
    if let Some(error) = saved["error"].as_str() {
        ensure!(
            !error.is_empty() && saved["fetched"].is_null() && saved["html_hash"].is_null(),
            "raw snapshot failure schema"
        );
        anyhow::bail!("{error}");
    }
    ensure!(saved["error"].is_null(), "raw snapshot error schema");
    let fetched = &saved["fetched"];
    let html = fetched["html"].as_str().context("raw snapshot HTML")?;
    ensure!(
        html.len() <= crate::net::MAX_HTML
            && format!("{:x}", Sha256::digest(html.as_bytes())) == saved["html_hash"],
        "raw snapshot HTML integrity"
    );
    let url = fetched["url"].as_str().context("raw snapshot final URL")?;
    crate::net::validate_url(url)?;
    let status = fetched["status"].as_u64().context("raw snapshot status")?;
    ensure!(
        (100..=599).contains(&status) && ![403, 429].contains(&status),
        "raw snapshot HTTP status"
    );
    let hops: Vec<String> = serde_json::from_value(fetched["hops"].clone())?;
    ensure!(
        (1..=6).contains(&hops.len())
            && hops.first().map(String::as_str) == Some(requested.as_str())
            && hops.last().map(String::as_str) == Some(url),
        "raw snapshot redirect provenance"
    );
    for hop in &hops {
        crate::net::validate_url(hop)?;
    }
    Ok((
        crate::net::Page {
            url: url.to_owned(),
            status: status as u16,
            html: html.to_owned(),
            hops,
            cache: Default::default(),
        },
        hit,
    ))
}

#[tokio::test]
async fn cache_only_miss_never_fetches() -> Result<()> {
    let root = tempfile::tempdir()?;
    assert_eq!(
        cached_fetch_mode("https://example.com/uncached", root.path(), true)
            .await
            .err()
            .unwrap()
            .to_string(),
        "cache_only_miss: https://example.com/uncached"
    );
    assert_eq!(std::fs::read_dir(root.path())?.count(), 0);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit all-retrieved raw pre-acquisition; up to480 unique guarded URLs, no model"]
async fn preacquire_source_pool() -> Result<()> {
    use sha2::{Digest, Sha256};
    let root = root();
    let pool: Value = serde_json::from_slice(&std::fs::read(root.join("retrieval/corpus.json"))?)?;
    let cases = pool["cases"].as_array().context("retrieval cases")?;
    ensure!(
        (1..=24).contains(&cases.len()),
        "bounded source cohort required"
    );
    ensure!(
        std::env::var_os("WEB_SEARCH_RAW_CACHE_ONLY").is_none(),
        "pre-acquisition cannot run in cache-only inference mode"
    );
    let cache = root.join("raw-cache");
    let mut blocks: HashMap<String, String> =
        serde_json::from_slice(&std::fs::read(cache.join("blocks.json"))?)?;
    let mut seen = std::collections::HashSet::new();
    let mut rows = Vec::new();
    let mut fetches = 0;
    for row in cases {
        for candidate in row["candidates"]
            .as_array()
            .context("retrieved candidates")?
        {
            let c: Candidate = serde_json::from_value(candidate.clone())?;
            if !seen.insert(c.url.clone()) {
                continue;
            }
            ensure!(seen.len() <= 480, "pre-acquisition URL cap");
            let path = cache.join(format!("{:x}.json", Sha256::digest(c.url.as_bytes())));
            let hit = path.exists();
            let host = crate::net::validate_url(&c.url)?
                .host_str()
                .context("public URL host")?
                .to_owned();
            let disabled = !hit && blocks.contains_key(&host);
            fetches += usize::from(!hit && !disabled);
            let result = cached_fetch(&c.url, &cache).await;
            let mut final_host = None;
            let error = match &result {
                Ok((page, _)) => {
                    final_host = Some(
                        crate::net::validate_url(&page.url)?
                            .host_str()
                            .unwrap()
                            .to_owned(),
                    );
                    crate::search::navigation_guard(&page.html)
                        .err()
                        .map(|e| e.to_string())
                }
                Err(error) => Some(error.to_string()),
            };
            if let Some(error) = &error {
                if !path.exists() {
                    crate::save(
                        &path,
                        &json!({"schema":"raw-document-v1","url":c.url,"fetched":null,"html_hash":null,"error":error}),
                    )?;
                }
                if crate::search::blocks_source(crate::search::error_code(error)) {
                    blocks.insert(host, error.clone());
                    if let Some(host) = final_host {
                        blocks.insert(host, error.clone());
                    }
                    crate::save(&cache.join("blocks.json"), &json!(blocks))?;
                }
            }
            rows.push(json!({"case":row["case"]["id"],"candidate":c.id,"url":c.url,"raw_cache_hit":hit,"host_disabled":disabled,"error":error,"snapshot":path,"snapshot_sha256":format!("{:x}",Sha256::digest(std::fs::read(&path)?))}));
            crate::save(
                &root.join("preacquisition.json"),
                &json!({"documents":rows,"unique_urls":seen.len(),"fetch_attempts":fetches,"complete":false,"model_calls":0}),
            )?;
            println!(
                "pre-acquired {} unique URLs, {fetches} fetch attempts",
                seen.len()
            );
        }
    }
    crate::save(
        &root.join("preacquisition.json"),
        &json!({"documents":rows,"unique_urls":seen.len(),"fetch_attempts":fetches,"complete":true,"model_calls":0}),
    )?;
    Ok(())
}

#[tokio::test]
async fn raw_snapshot_reextracts_and_checks_integrity_without_network() -> Result<()> {
    use sha2::{Digest, Sha256};
    let root = tempfile::tempdir()?;
    crate::save(&root.path().join("blocks.json"), &json!({}))?;
    let url = "https://example.com/empty";
    let path = root
        .path()
        .join(format!("{:x}.json", Sha256::digest(url.as_bytes())));
    let html = "<html><body><script>ignored()</script></body></html>";
    let mut saved = json!({"schema":"raw-document-v1","url":url,"fetched":{"url":url,"status":200,"hops":[url],"html":html},"html_hash":format!("{:x}",Sha256::digest(html.as_bytes())),"error":null});
    crate::save(&path, &saved)?;
    let (page, hit) = cached_fetch(url, root.path()).await?;
    assert!(hit);
    assert!(crate::extract(page.html, url).is_err());
    let (again, hit) = cached_fetch(url, root.path()).await?;
    assert!(hit);
    assert_eq!(again.html, html);
    assert!(saved["error"].is_null());

    saved["fetched"]["status"] = json!(404);
    crate::save(&path, &saved)?;
    assert_eq!(cached_fetch(url, root.path()).await?.0.status, 404);
    saved["fetched"]["status"] = json!(200);
    saved["fetched"]["html"] = json!("changed without changing hash");
    crate::save(&path, &saved)?;
    assert!(
        cached_fetch(url, root.path())
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("HTML integrity")
    );
    saved["fetched"] = Value::Null;
    saved["html_hash"] = Value::Null;
    saved["error"] = json!("fetch_timeout: captured failure");
    crate::save(&path, &saved)?;
    assert_eq!(
        cached_fetch(url, root.path())
            .await
            .err()
            .unwrap()
            .to_string(),
        "fetch_timeout: captured failure"
    );
    crate::save(
        &root.path().join("blocks.json"),
        &json!({"example.com":"source_blocked"}),
    )?;
    assert!(
        cached_fetch("https://example.com/never-fetched", root.path())
            .await
            .err()
            .unwrap()
            .to_string()
            .starts_with("host_disabled:")
    );
    Ok(())
}

/// Historical extracted-document replay; new extraction comparisons use cached_fetch instead.
pub async fn cached_read(
    c: &Candidate,
    query: &str,
    case_id: &str,
    started: std::time::Instant,
    output: Option<&Path>,
) -> (Value, Option<crate::rescue::Page>) {
    use sha2::{Digest, Sha256};
    let root = PathBuf::from(std::env::var("WEB_SEARCH_DOCUMENT_CACHE").unwrap());
    let result: Result<(Value, Option<crate::rescue::Page>)> = async {
        std::fs::create_dir_all(&root)?;
        let path = root.join(format!("{:x}.json", Sha256::digest(c.url.as_bytes())));
        if !path.exists() {
            let host = reqwest::Url::parse(&c.url)?
                .host_str()
                .context("host")?
                .to_owned();
            let blocks: HashMap<String, String> =
                serde_json::from_slice(&std::fs::read(root.join("blocks.json"))?)?;
            if let Some(reason) = blocks.get(&host) {
                return Ok((
                    json!({"id":c.id,"url":c.url,"error":format!("host_disabled: {reason}")}),
                    None,
                ));
            }
            let (record, page) =
                crate::rescue::read_document_live(c, query, case_id, started, output).await;
            let doc = page
                .as_ref()
                .map(|p| json!({"text":p.text,"text_hash":p.text_hash,"page_title":p.title}));
            crate::save(&path, &json!({"url":c.url,"record":record,"document":doc}))?;
            if let Some(error) = record["error"].as_str()
                && crate::search::blocks_source(crate::search::error_code(error))
            {
                let mut blocks = blocks;
                blocks.insert(host, error.into());
                crate::save(&root.join("blocks.json"), &json!(blocks))?;
            }
            return Ok((record, page));
        }
        let saved: Value = serde_json::from_slice(&std::fs::read(path)?)?;
        ensure!(saved["url"] == c.url, "snapshot URL drift");
        let mut record = saved["record"].clone();
        record["id"] = json!(c.id);
        record["snapshot_cache_hit"] = json!(true);
        let doc = &saved["document"];
        if doc.is_null() {
            return Ok((record, None));
        }
        ensure!(
            format!(
                "{:x}",
                Sha256::digest(doc["text"].as_str().context("snapshot text")?.as_bytes())
            ) == doc["text_hash"],
            "snapshot hash"
        );
        crate::net::validate_url(record["final_url"].as_str().context("finalURL")?)?;
        if let Some(out) = output {
            crate::save(&out.join(format!("{case_id}-{}-document.json", c.id)), doc)?;
        }
        let page = crate::rescue::page_from_document(
            c,
            record["final_url"].as_str().unwrap(),
            doc,
            query,
        )?;
        Ok((record, Some(page)))
    }
    .await;
    result.unwrap_or_else(|e| {
        (
            json!({"id":c.id,"url":c.url,"error":format!("snapshot: {e}")}),
            None,
        )
    })
}

#[tokio::test]
#[ignore = "explicit full pipeline on frozen live SERP, initial classifier never bypassed;3generative+4reads/case, Qwen both versions"]
async fn full_pipeline_replay() -> Result<()> {
    use sha2::{Digest, Sha256};
    let root = root();
    let pool: Value = serde_json::from_slice(&std::fs::read(root.join("retrieval/corpus.json"))?)?;
    let mode = std::env::var("WEB_SEARCH_BRANCH")?;
    ensure!(mode == "baseline" || mode == "fixed", "branch");
    let profile = std::env::var("WEB_SEARCH_EXPERIMENT_PROFILE")
        .unwrap_or_else(|_| "classifier-relation".into());
    let expected = match (profile.as_str(), mode.as_str()) {
        ("classifier-relation", "baseline") => {
            ("request-evidence-v2", "conservative-evidence-v3-body")
        }
        ("classifier-relation", "fixed") | ("extraction-selection", "baseline") => (
            "request-evidence-v4-audited",
            "conservative-evidence-v4-bounded-output",
        ),
        ("extraction-selection", "fixed") | ("direct-coverage", "baseline") => (
            "request-evidence-v5-reference-read",
            "conservative-evidence-v4-bounded-output",
        ),
        ("direct-coverage", "fixed")
        | ("passage-coverage", "baseline")
        | ("passage-coverage-final", "baseline")
        | ("passage-coverage-confirm", "baseline") => (
            "request-evidence-v6-context-audit",
            "conservative-evidence-v4-bounded-output",
        ),
        ("passage-coverage", "fixed") => (
            "request-evidence-v7-diverse-coverage",
            "conservative-evidence-v4-bounded-output",
        ),
        ("passage-coverage-final", "fixed") => (
            "request-evidence-v8-atomic-contract",
            "conservative-evidence-v4-bounded-output",
        ),
        ("passage-coverage-confirm", "fixed") | ("admission-literals", "baseline") => (
            "request-evidence-v9-reference-context",
            "conservative-evidence-v4-bounded-output",
        ),
        ("admission-literals", "fixed") => (
            "request-evidence-v10-admission-literals",
            "conservative-evidence-v4-bounded-output",
        ),
        _ => anyhow::bail!("unknown experiment profile"),
    };
    ensure!(
        crate::evidence::VERSION == expected.0 && crate::model::CLASSIFIER_VERSION == expected.1,
        "branch code identity mismatch; no generations made"
    );
    ensure!(
        profile != "admission-literals" || std::env::var_os("WEB_SEARCH_RAW_CACHE_ONLY").is_some(),
        "admission-literals requires cache-only replay"
    );
    let out =
        root.join(std::env::var("WEB_SEARCH_PIPELINE_OUTPUT").unwrap_or_else(|_| mode.clone()));
    std::fs::create_dir(&out)?;
    let model = remote()?.with_reranker(
        Some(Path::new(&std::env::var("WEB_SEARCH_OMLX_SETTINGS")?)),
        true,
    )?;
    model.check().await?;
    model.reranker.as_ref().unwrap().check().await?;
    let raw_cache = std::env::var_os("WEB_SEARCH_RAW_CACHE");
    ensure!(
        !matches!(
            profile.as_str(),
            "extraction-selection"
                | "direct-coverage"
                | "passage-coverage"
                | "passage-coverage-final"
                | "passage-coverage-confirm"
                | "admission-literals"
        ) || raw_cache.is_some(),
        "paired comparison requires raw acquisition cache"
    );
    let cache = PathBuf::from(
        raw_cache
            .or_else(|| std::env::var_os("WEB_SEARCH_DOCUMENT_CACHE"))
            .context("document cache required")?,
    );
    ensure!(!cache.as_os_str().is_empty(), "document cache required");
    let mut session = policy::Session::frozen(
        pool["cases"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| {
                serde_json::from_value::<Vec<policy::Source>>(r["sources"].clone()).unwrap()
            })
            .collect(),
    );
    let mut blocked = HashMap::new();
    let mut rows = Vec::new();
    crate::save(
        &out.join("manifest.json"),
        &json!({"branch":mode,"experiment_profile":profile,"raw_acquisition_cache":std::env::var_os("WEB_SEARCH_RAW_CACHE").is_some(),"binary_sha256":format!("{:x}",Sha256::digest(std::fs::read(std::env::current_exe()?)?)),"classifier":crate::model::CLASSIFIER_VERSION,"evidence_version":crate::evidence::VERSION,"initial_classifier_bypass":false,"reranker":crate::reranker::MODEL}),
    )?;
    for row in pool["cases"].as_array().unwrap() {
        if let Ok(selected) = std::env::var("WEB_SEARCH_DEV_CASES")
            && !selected
                .split(',')
                .any(|id| Some(id) == row["case"]["id"].as_str())
        {
            continue;
        }
        let case: Case = serde_json::from_value(row["case"].clone())?;
        let mut document_blocks: HashMap<String, String> =
            serde_json::from_slice(&std::fs::read(cache.join("blocks.json"))?)?;
        let result = session
            .run(
                &case,
                &model,
                &mut blocked,
                &mut document_blocks,
                Some(&out),
                true,
            )
            .await?;
        if std::env::var_os("WEB_SEARCH_RAW_CACHE").is_some()
            && std::env::var_os("WEB_SEARCH_RAW_CACHE_ONLY").is_none()
        {
            crate::save(&cache.join("blocks.json"), &json!(document_blocks))?;
        }
        ensure!(result["candidates"] == row["candidates"], "pool changed");
        rows.push(result);
        crate::save(
            &out.join("results.json"),
            &json!({"cases":rows,"branch":mode}),
        )?;
        let calls = rows
            .iter()
            .flat_map(|r| r["model_calls"].as_array().unwrap())
            .collect::<Vec<_>>();
        let cost: f64 = calls
            .iter()
            .filter_map(|c| c["response"]["usage"]["cost"].as_f64())
            .sum();
        ensure!(cost < 2.0, "cost cap");
        ensure!(
            !calls
                .iter()
                .any(|c| c.to_string().contains("HTTP 403") || c.to_string().contains("HTTP 429")),
            "provider refusal; stop"
        );
        println!("{} {} complete, cost=${cost:.5}", mode, case.id);
    }
    Ok(())
}

#[tokio::test]
#[ignore = "explicit blind gold audit of preselected retrieved pages; never branch inputs"]
async fn audit_new_gold() -> Result<()> {
    let root = root();
    let out = root.join(
        std::env::var("WEB_SEARCH_GOLD_AUDIT_OUTPUT").unwrap_or_else(|_| "gold-audit".into()),
    );
    std::fs::create_dir(&out)?;
    let plan: Value = serde_json::from_slice(&std::fs::read(
        root.join(
            std::env::var("WEB_SEARCH_GOLD_AUDIT_PLAN")
                .unwrap_or_else(|_| "gold-audit-plan.json".into()),
        ),
    )?)?;
    let mut rows = Vec::new();
    for item in plan["documents"].as_array().unwrap() {
        let case: Case = serde_json::from_value(item["case"].clone())?;
        let c: Candidate = serde_json::from_value(item["candidate"].clone())?;
        let (record, _) = crate::rescue::read_document_live(
            &c,
            &case.query,
            &case.id,
            std::time::Instant::now(),
            Some(&out),
        )
        .await;
        rows.push(json!({"case":case.id,"candidate":c.id,"url":c.url,"record":record}));
        crate::save(
            &out.join("manifest.json"),
            &json!({"purpose":"gold only, not branch inputs","documents":rows}),
        )?;
    }
    Ok(())
}

#[tokio::test]
#[ignore = "explicit historical v5 coverage replay through v6 status guard/audit;up to2generations,no search/fetch/rerank/initial calls"]
async fn historical_audit_replay() -> Result<()> {
    use crate::evidence::Memory;
    use sha2::{Digest, Sha256};
    let corpus: Value = serde_json::from_slice(&std::fs::read(
        "feasibility/extraction-selection-corpus-2026-10-30.json",
    )?)?;
    let out = root().join("historical-audit");
    std::fs::create_dir(&out)?;
    let model = remote()?;
    model.check().await?;
    let mut survived = Vec::new();
    for (cid, false_id) in [("n01", "C020"), ("n07", "C015")] {
        let saved = corpus["branches"]["fixed"]["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == cid)
            .context("historical result")?;
        let pool = corpus["pool"]["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["case"]["id"] == cid)
            .context("historical source pool")?;
        let case: Case = serde_json::from_value(pool["case"].clone())?;
        let cs: Vec<Candidate> = serde_json::from_value(saved["candidates"].clone())?;
        ensure!(
            saved["results"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["id"] == false_id && r["classification"] == "direct"),
            "historical fixture must contain the previous false direct"
        );
        let rank = serde_json::from_value(pool["rank"].clone())?;
        let mut labels = policy::labels(&saved["model_calls"][0]["response"], &cs, &case.query)?;
        let mut memory = Memory::new(&case, &cs, &labels, &rank)?;
        let old_memory = &saved["page_verification"]["evidence_memory"];
        let ids: Vec<_> = memory.entries.iter().map(|e| e.id.clone()).collect();
        ensure!(
            ids == old_memory["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e["id"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "historical candidate pool drift"
        );
        for old_entry in old_memory["entries"].as_array().unwrap() {
            let id = old_entry["id"].as_str().unwrap();
            let ranges: Vec<_> = old_entry["passages"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["field"] == "body")
                .map(|p| {
                    (
                        p["start"].as_u64().unwrap() as usize,
                        p["end"].as_u64().unwrap() as usize,
                    )
                })
                .collect();
            if !ranges.is_empty() {
                let c = cs.iter().find(|c| c.id == id).unwrap();
                let doc = &corpus["documents"]["fixed"][format!("{cid}-{id}-document.json")];
                ensure!(
                    format!(
                        "{:x}",
                        Sha256::digest(doc["text"].as_str().context("historical body")?.as_bytes())
                    ) == doc["text_hash"],
                    "historical document hash"
                );
                let record = saved["page_verification"]["documents"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|d| d["id"] == id)
                    .unwrap();
                let page = crate::rescue::page_from_document(
                    c,
                    record["final_url"].as_str().unwrap(),
                    doc,
                    &case.query,
                )?;
                memory.add_page(&page, Some(&ranges))?;
            }
            let entry = memory.entries.iter_mut().find(|e| e.id == id).unwrap();
            entry.read_attempted = old_entry["read_attempted"].as_bool().unwrap();
            let mut rebuilt = json!(entry.passages);
            for p in rebuilt.as_array_mut().unwrap() {
                p.as_object_mut().unwrap().remove("text");
            }
            ensure!(
                rebuilt == old_entry["passages"],
                "historical passage IDs/ranges/hash drift"
            );
        }
        let coverage = saved["model_calls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["input"]["messages"][0]["content"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("Review SOURCE RELEVANCE"))
            })
            .context("historical coverage call")?;
        memory.apply(&coverage["response"], &ids)?;
        let targets = memory.audit_ids();
        let audit_context = if targets.is_empty() {
            None
        } else {
            Some(serde_json::from_str::<Value>(
                &memory.audit_context(&targets)?,
            )?)
        };
        crate::save(
            &out.join(format!("{cid}-before.json")),
            &json!({"memory":memory,"coverage":coverage,"audit_targets":targets,"audit_context":audit_context,"previous_false_direct":false_id}),
        )?;
        let mut report = crate::rescue::Report::default();
        let checked = crate::evidence::audit(
            &mut memory,
            &model,
            std::time::Instant::now(),
            Some(&out),
            cid,
            &mut report,
        )
        .await;
        memory.finish(&cs, &mut labels, &mut report);
        crate::save(
            &out.join(format!("{cid}-result.json")),
            &json!({"calls":report.calls,"validation_error":checked.as_ref().err().map(ToString::to_string),"labels":labels.kinds,"memory":memory,"proofs":report.proofs,"previous_false_direct":false_id}),
        )?;
        checked?;
        if labels.kinds[false_id] == 0 {
            survived.push(format!("{cid}/{false_id}"));
        }
        println!(
            "{cid}/{false_id} audit direct={}",
            labels.kinds[false_id] == 0
        );
    }
    ensure!(
        survived.is_empty(),
        "historical false direct survived audit: {survived:?}"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit old false-direct window replay through new evidence reviewer;1generation, no search or fetch"]
async fn old_false_direct_replay() -> Result<()> {
    use crate::evidence::{Memory, Passage, Requirement};
    let corpus: Value = serde_json::from_slice(&std::fs::read(
        "feasibility/reranker-corpus-2026-10-30.json",
    )?)?;
    let set = &corpus["sets"]["component"];
    let row = set["capture"]["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "h08")
        .unwrap();
    let old = set["comparison"]["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "h08")
        .unwrap();
    let oldmem = &old["branches"]["qwen"]["memory"];
    let case = Case {
        id: "regression-ifrange".into(),
        query: row["query"].as_str().unwrap().into(),
        forms: vec![row["query"].as_str().unwrap().into()],
    };
    let cs: Vec<Candidate> = serde_json::from_value(row["candidates"].clone())?;
    let sources: Vec<policy::Source> = serde_json::from_value(row["sources"].clone())?;
    let (_, rank) = policy::export(&case, &sources)?;
    let mut labels = policy::Classifications {
        kinds: cs.iter().map(|c| (c.id.clone(), 1)).collect(),
        ..Default::default()
    };
    let mut memory = Memory::new(&case, &cs, &labels, &rank)?;
    memory.subject = "If-Range".into();
    memory.reviewed_rounds = 1;
    memory.requirements = oldmem["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| Requirement {
            id: r["id"].as_str().unwrap().into(),
            anchor: r["anchor"].as_str().unwrap().into(),
            start: r["start"].as_u64().unwrap() as usize,
            end: r["end"].as_u64().unwrap() as usize,
        })
        .collect();
    for entry in &mut memory.entries {
        let saved = oldmem["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == entry.id)
            .unwrap();
        entry.passages = saved["passages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| Passage {
                id: p["id"].as_str().unwrap().into(),
                text: p["text"].as_str().unwrap().into(),
                source_url: p["source_url"].as_str().unwrap().into(),
                text_hash: p["text_hash"].as_str().unwrap().into(),
                normalized_text_hash: p["normalized_text_hash"].as_str().unwrap().into(),
                field: match p["field"].as_str().unwrap() {
                    "body" => "body",
                    "page_title" => "page_title",
                    "title" => "title",
                    _ => "snippet",
                },
                start: p["start"].as_u64().unwrap() as usize,
                end: p["end"].as_u64().unwrap() as usize,
            })
            .collect();
    }
    let ids = vec!["C002".into(), "C009".into(), "C010".into()];
    let mut report = crate::rescue::Report::default();
    let out = root().join(
        std::env::var("WEB_SEARCH_RELATION_PHASE").unwrap_or_else(|_| "relation-dev1".into()),
    );
    std::fs::create_dir(&out)?;
    let checked = crate::evidence::review(
        &mut memory,
        &ids,
        &remote()?,
        std::time::Instant::now(),
        Some(&out),
        &case.id,
        &mut report,
    )
    .await;
    crate::save(
        &out.join("review-result.json"),
        &json!({"calls":report.calls,"validation_error":checked.as_ref().err().map(ToString::to_string)}),
    )?;
    checked?;
    crate::evidence::audit(
        &mut memory,
        &remote()?,
        std::time::Instant::now(),
        Some(&out),
        &case.id,
        &mut report,
    )
    .await?;
    memory.finish(&cs, &mut labels, &mut report);
    let result = json!({"calls":report.calls,"proofs":report.proofs,"labels":labels.kinds,"memory":memory,"previous_false_direct":ids});
    crate::save(&root().join("old-false-direct-replay.json"), &result)?;
    ensure!(
        ids.iter().all(|id| labels.kinds[id] != 0),
        "old neighboring-header false direct survived"
    );
    Ok(())
}
