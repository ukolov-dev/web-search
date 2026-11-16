//! Explicit development experiment; no network in ordinary cargo test.
use crate::{
    evidence::{self, Memory},
    model::Model,
    policy::{self, Candidate, Case, Classifications},
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
        std::env::var("WEB_SEARCH_RERANK_ROOT")
            .unwrap_or_else(|_| "runs/reranker-2026-10-30".into()),
    )
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn remote() -> Result<Model> {
    Model::from_openrouter(
        Path::new(&std::env::var("WEB_SEARCH_OPENROUTER_KEY_FILE")?),
        "deepseek/deepseek-v4.1-flash-1017",
        std::env::var("WEB_SEARCH_OPENROUTER_PROXY").ok().as_deref(),
    )
}
fn cost(calls: &[Value]) -> f64 {
    calls
        .iter()
        .filter_map(|c| c["response"]["usage"]["cost"].as_f64())
        .sum()
}
fn no_refusal(calls: &[Value]) -> Result<()> {
    ensure!(
        !calls
            .iter()
            .any(|c| c.to_string().contains("HTTP 403") || c.to_string().contains("HTTP 429")),
        "model refusal; stop"
    );
    Ok(())
}
#[tokio::test]
#[ignore = "Explicit new12 capture; 24 search attempts, max48 safe page reads; no reranking/prediction comparison"]
async fn prepare_reranker_corpus() -> Result<()> {
    let out = root().join("capture");
    std::fs::create_dir(&out)?;
    let fixture = std::fs::read(
        std::env::var("WEB_SEARCH_CASES")
            .unwrap_or_else(|_| "feasibility/reranker-cases-v1.json".into()),
    )?;
    let component = std::env::var_os("WEB_SEARCH_PASSAGE_ONLY").is_some();
    let plan: Value = serde_json::from_slice(&fixture)?;
    let model = remote()?;
    model.check().await?;
    let mut session = policy::Session::new();
    let mut blocked = HashMap::new();
    let mut hosts: HashMap<String, String> = [
        "curl.se",
        "redis.io",
        "stackoverflow.com",
        "superuser.com",
        "www.npmjs.com",
    ]
    .into_iter()
    .map(|h| {
        (
            h.into(),
            "preserved block from evidence-layer 2026-10-30".into(),
        )
    })
    .collect();
    let mut rows = Vec::new();
    let mut total_cost = 0.0;
    for row in plan["cases"].as_array().context("cases")? {
        let case: Case = serde_json::from_value(row.clone())?;
        let started = Instant::now();
        let mut sources = Vec::new();
        for engine in ["seznam", "naver"] {
            sources.push(
                session
                    .source(engine, &case.query, &mut blocked, Some(&out))
                    .await?,
            );
        }
        let (candidates, rank) = policy::export(&case, &sources)?;
        let mut labels = Classifications::default();
        let mut calls = Vec::new();
        let mut errors = Vec::new();
        if component {
            labels.kinds = candidates.iter().map(|c| (c.id.clone(), 1)).collect();
        }
        for context in if component {
            Vec::new()
        } else {
            policy::batches(&case, &candidates)?
        } {
            let input: Value = serde_json::from_str(&context)?;
            let expected: Vec<Candidate> = serde_json::from_value(input["candidates"].clone())?;
            let ids = expected.iter().map(|c| c.id.clone()).collect::<Vec<_>>();
            match model
                .classify(
                    &context,
                    &ids,
                    Duration::from_secs(180).saturating_sub(started.elapsed()),
                )
                .await
            {
                Ok(call) => {
                    let found = policy::labels(&call["response"], &expected, &case.query);
                    let warning = !call["warning"].is_null();
                    calls.push(call);
                    match found {
                        Ok(found) if !warning => {
                            labels.kinds.extend(found.kinds);
                            labels.evidence.extend(found.evidence);
                        }
                        Ok(_) => errors.push("classifier warning".into()),
                        Err(e) => errors.push(e.to_string()),
                    }
                }
                Err(e) => errors.push(e.to_string()),
            }
        }
        let initial_valid = errors.is_empty()
            && labels.kinds.len() == candidates.len()
            && (component || calls.len() == 1)
            && !candidates.is_empty();
        let mut first = rescue::Report::default();
        let mut docs = Vec::new();
        let mut memory_trace = Value::Null;
        if initial_valid {
            let mut memory = Memory::new(&case, &candidates, &labels, &rank)?;
            let ids = memory
                .entries
                .iter()
                .map(|e| e.id.clone())
                .collect::<Vec<_>>();
            match evidence::review(
                &mut memory,
                &ids,
                &model,
                started,
                Some(&out),
                &case.id,
                &mut first,
            )
            .await
            {
                Ok(()) => {
                    memory_trace = json!(memory);
                    for id in memory.next_reads() {
                        let c = candidates.iter().find(|c| c.id == id).unwrap();
                        let host = reqwest::Url::parse(&c.url)?
                            .host_str()
                            .context("host")?
                            .to_owned();
                        if hosts.contains_key(&host) {
                            docs.push(json!({"id":id,"error":"host_disabled"}));
                            continue;
                        }
                        let (record, page) =
                            rescue::read_document(c, &case.query, &case.id, started, Some(&out))
                                .await;
                        if let Some(error) = record["error"].as_str()
                            && crate::search::blocks_source(crate::search::error_code(error))
                        {
                            hosts.insert(host, error.into());
                        }
                        if let Some(page) = page {
                            ensure!(page.id == id, "page ID");
                        }
                        docs.push(record);
                    }
                }
                Err(e) => errors.push(format!("first review: {e}")),
            }
        }
        total_cost += cost(&calls) + cost(&first.calls);
        no_refusal(&calls)?;
        no_refusal(&first.calls)?;
        rows.push(json!({"id":case.id,"query":case.query,"sources":sources,"candidates":candidates,"initial_calls":calls,"initial_labels":labels.kinds,"initial_evidence":labels.evidence,"initial_valid":initial_valid,"first_calls":first.calls,"first_memory":memory_trace,"documents":docs,"errors":errors,"elapsed_ms":started.elapsed().as_millis()}));
        crate::save(
            &out.join("corpus.json"),
            &json!({"fixture_sha256":hash(&fixture),"component_isolation":component,"cases":rows,"blocked_sources":blocked,"blocked_document_hosts":hosts,"cost_usd":total_cost}),
        )?;
        println!(
            "prepared {} valid={} cost=${total_cost:.5}",
            case.id, initial_valid
        );
        ensure!(total_cost < 2.0, "cost cap");
    }
    Ok(())
}

#[tokio::test]
#[ignore = "Explicit paired frozen corpus evaluation after gold freeze; local rerank + shared reviewer, no source network"]
async fn compare_reranker() -> Result<()> {
    let root = root();
    let capture = root.join("capture");
    let corpus: Value = serde_json::from_slice(&std::fs::read(capture.join("corpus.json"))?)?;
    let freeze: Value = serde_json::from_slice(&std::fs::read(root.join("gold-freeze.json"))?)?;
    for (path, expected) in freeze.as_object().context("freeze")? {
        ensure!(
            hash(&std::fs::read(path)?) == expected.as_str().unwrap(),
            "freeze changed: {path}"
        );
    }
    let out = root.join("comparison");
    std::fs::create_dir(&out)?;
    let model = remote()?;
    model.check().await?;
    let reranker = Model::from_settings(
        Path::new(&std::env::var("WEB_SEARCH_OMLX_SETTINGS")?),
        crate::reranker::MODEL,
    )?;
    reranker.check().await?;
    let mut rows = Vec::new();
    let mut total_cost = 0.0;
    for row in corpus["cases"].as_array().context("cases")? {
        let case = Case {
            id: row["id"].as_str().unwrap().into(),
            query: row["query"].as_str().unwrap().into(),
            forms: vec![row["query"].as_str().unwrap().into()],
        };
        let candidates: Vec<Candidate> = serde_json::from_value(row["candidates"].clone())?;
        let sources: Vec<policy::Source> = serde_json::from_value(row["sources"].clone())?;
        let (_, rank) = policy::export(&case, &sources)?;
        let mut branches = serde_json::Map::new();
        // Alternate order by query to reduce time/provider drift.
        let modes = if rows.len() % 2 == 0 {
            ["lexical", "qwen"]
        } else {
            ["qwen", "lexical"]
        };
        for mode in modes {
            let mut labels = Classifications {
                kinds: serde_json::from_value(row["initial_labels"].clone())?,
                evidence: serde_json::from_value(row["initial_evidence"].clone())?,
                ..Default::default()
            };
            let mut memory = Memory::new(&case, &candidates, &labels, &rank)?;
            let mut report = rescue::Report::default();
            let wall = Instant::now();
            let mut shared_ms = 0u64;
            for c in row["initial_calls"]
                .as_array()
                .unwrap()
                .iter()
                .chain(row["first_calls"].as_array().unwrap())
            {
                shared_ms += c["elapsed_ms"].as_u64().unwrap_or(0);
            }
            shared_ms += row["documents"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|d| d["elapsed_ms"].as_u64())
                .sum::<u64>();
            let started = wall
                .checked_sub(Duration::from_millis(shared_ms))
                .context("clock")?;
            let valid = row["initial_valid"] == true && row["first_memory"].is_object();
            let mut selected_ids = Vec::new();
            if valid {
                let ids = memory
                    .entries
                    .iter()
                    .map(|e| e.id.clone())
                    .collect::<Vec<_>>();
                memory.apply(&row["first_calls"][0]["response"], &ids)?;
                for id in memory.next_reads() {
                    memory
                        .entries
                        .iter_mut()
                        .find(|e| e.id == id)
                        .unwrap()
                        .read_attempted = true;
                    let Some(record) = row["documents"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|r| r["id"] == id && r["text_hash"].is_string())
                    else {
                        continue;
                    };
                    let c = candidates.iter().find(|c| c.id == id).unwrap();
                    let doc: Value = serde_json::from_slice(&std::fs::read(
                        capture.join(format!("{}-{}-document.json", case.id, id)),
                    )?)?;
                    ensure!(
                        hash(doc["text"].as_str().context("text")?.as_bytes()) == doc["text_hash"],
                        "document hash"
                    );
                    let page = rescue::page_from_document(
                        c,
                        record["final_url"].as_str().context("final URL")?,
                        &doc,
                        &case.query,
                    )?;
                    memory
                        .add_page_ranked(
                            &page,
                            (mode == "qwen").then_some(&reranker),
                            started,
                            &mut report.reranker_calls,
                        )
                        .await?;
                    selected_ids.push(id.clone());
                }
                if !selected_ids.is_empty()
                    && let Err(e) = evidence::review(
                        &mut memory,
                        &selected_ids,
                        &model,
                        started,
                        None,
                        &case.id,
                        &mut report,
                    )
                    .await
                {
                    report.errors.push(e.to_string());
                }
            } else {
                labels = Classifications::default();
            }
            let full_memory = json!(memory);
            memory.finish(&candidates, &mut labels, &mut report);
            total_cost += cost(&report.calls);
            no_refusal(&report.calls)?;
            let ranked = policy::select(&candidates, &labels.kinds, &rank)?
                .iter()
                .take(10)
                .map(|c| c.id.clone())
                .collect::<Vec<_>>();
            branches.insert(mode.into(),json!({"valid":valid,"labels":labels.kinds,"proofs":report.proofs,"ranked":ranked,"memory":full_memory,"calls":report.calls,"reranker_calls":report.reranker_calls,"errors":report.errors,"rejected":report.rejected,"shared_ms":shared_ms,"branch_ms":wall.elapsed().as_millis(),"logical_ms":started.elapsed().as_millis()}));
            ensure!(total_cost < 2.0, "cost cap");
        }
        rows.push(json!({"id":case.id,"branches":branches}));
        crate::save(
            &out.join("results.json"),
            &json!({"cases":rows,"review_cost_usd":total_cost,"corpus_sha256":hash(&std::fs::read(capture.join("corpus.json"))?)}),
        )?;
        println!("compared {} cost=${total_cost:.5}", case.id);
    }
    Ok(())
}

#[tokio::test]
#[ignore = "explicit gold-only audit of retrieved primary documents; does not enter product A/B inputs"]
async fn audit_reranker_primary_documents() -> Result<()> {
    let root = root();
    let capture = root.join("capture");
    let corpus: Value = serde_json::from_slice(&std::fs::read(capture.join("corpus.json"))?)?;
    let out = root.join("gold-audit");
    std::fs::create_dir(&out)?;
    let (hosts, paths): (&[&str], &[&str]) = if corpus["component_isolation"] == true {
        (
            &[
                "docs.python.org",
                "doc.rust-lang.org",
                "postgresql.org",
                "sqlite.org",
                "docs.docker.com",
                "developer.mozilla.org",
                "git-scm.com",
                "developer.mozilla.org",
            ],
            &[
                "weakref.html",
                "struct.Condvar.html",
                "sql-createindex",
                "pragma",
                "tmpfs",
                "min-width",
                "git-restore",
                "If-Range",
            ],
        )
    } else {
        (
            &[
                "docs.python.org",
                "doc.rust-lang.org",
                "postgresql.org",
                "sqlite.org",
                "docs.docker.com",
                "developer.mozilla.org",
                "git-scm.com",
                "developer.mozilla.org",
                "developer.mozilla.org",
                "docs.python.org",
                "doc.rust-lang.org",
                "postgresql.org",
            ],
            &[
                "pathlib.html",
                "struct.Mutex.html",
                "sql-vacuum",
                "foreignkeys.html",
                "bind-mounts",
                "box-sizing",
                "git-reset",
                "307",
                "structuredClone",
                "asyncio-task.html",
                "struct.Mutex.html",
                "sql-vacuum",
            ],
        )
    };
    let mut rows = Vec::new();
    for (i, row) in corpus["cases"].as_array().unwrap().iter().enumerate() {
        let candidates: Vec<Candidate> = serde_json::from_value(row["candidates"].clone())?;
        let found = candidates.iter().find(|c| {
            reqwest::Url::parse(&c.url).ok().is_some_and(|u| {
                u.host_str().is_some_and(|h| {
                    h == hosts[i] || h.strip_suffix(hosts[i]).is_some_and(|p| p.ends_with('.'))
                })
            }) && c.url.contains(paths[i])
        });
        if let Some(c) = found {
            if let Some(record) = row["documents"]
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["id"] == c.id)
            {
                rows.push(json!({"case":row["id"],"id":c.id,"url":c.url,"runtime_record":record}));
                continue;
            }
            let (record, _) = rescue::read_document(
                c,
                row["query"].as_str().unwrap(),
                row["id"].as_str().unwrap(),
                Instant::now(),
                Some(&out),
            )
            .await;
            rows.push(json!({"case":row["id"],"id":c.id,"url":c.url,"record":record}));
        }
        crate::save(
            &out.join("manifest.json"),
            &json!({"purpose":"blinded source gold only, never branch evidence","documents":rows}),
        )?;
    }
    crate::save(
        &out.join("manifest.json"),
        &json!({"purpose":"blinded source gold only, never branch evidence","documents":rows}),
    )?;
    Ok(())
}
