use crate::{
    model::Model,
    policy::{self, Candidate, Case, Classifications},
    rescue,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, Instant},
};

fn corpus() -> Result<Value> {
    Ok(serde_json::from_slice(&std::fs::read(
        "feasibility/model-benchmark-corpus-2026-10-30.json",
    )?)?)
}

#[test]
fn frozen_benchmark_alignment_and_input_isolation() -> Result<()> {
    let data = corpus()?;
    let cases = data["cases"].as_array().context("cases")?;
    ensure!(cases.len() == 46, "case count");
    let mut count = 0;
    let mut judged = 0;
    for row in cases {
        let case: Case = serde_json::from_value(row.clone())?;
        let candidates: Vec<Candidate> = serde_json::from_value(row["candidates"].clone())?;
        let sources: Vec<policy::Source> = serde_json::from_value(row["sources"].clone())?;
        let (exported, _) = policy::export(&case, &sources)?;
        ensure!(
            serde_json::to_value(exported)? == row["candidates"],
            "export drift: {}",
            case.id
        );
        for g in row["gold"].as_array().unwrap() {
            let c = candidates
                .iter()
                .find(|c| c.id == g["id"])
                .context("unknown gold ID")?;
            if let Some(url) = g["url"].as_str() {
                ensure!(url == c.url, "gold URL mismatch");
            }
            ensure!(g["grade"].as_u64().is_some_and(|v| v <= 2), "invalid grade");
            judged += 1;
        }
        for context in policy::batches(&case, &candidates)? {
            let input: Value = serde_json::from_str(&context)?;
            ensure!(
                input.as_object().unwrap().len() == 2
                    && input["query"] == case.query
                    && input["candidates"].is_array(),
                "input/gold isolation"
            );
        }
        count += candidates.len();
    }
    ensure!(count == 1058 && judged == 660, "pool size drift");
    ensure!(
        format!(
            "{:x}",
            Sha256::digest(std::fs::read("feasibility/search-cases.json")?)
        ) == data["protocol"]["original20_sha256"],
        "original20 changed"
    );
    Ok(())
}

fn selected(
    candidates: &[Candidate],
    kinds: &HashMap<String, u8>,
    rank: &HashMap<String, usize>,
    query: &str,
) -> Result<Vec<String>> {
    let site = crate::search::site_filter(query)?;
    Ok(policy::select(candidates, kinds, rank)?
        .into_iter()
        .filter(|c| {
            crate::search::filter_site(
                site.as_ref(),
                vec![crate::search::SearchResult {
                    title: c.title.clone(),
                    url: c.url.clone(),
                    snippet: c.snippet.clone(),
                    position: 1,
                    result_source: None,
                }],
            )
            .is_ok()
        })
        .take(10)
        .map(|c| c.id)
        .collect())
}

#[tokio::test]
#[ignore = "explicit new-set DeepSeek evidence A/B; reuses saved initial calls and SERP, up to4 cached/fresh public page reads per case"]
async fn evidence_layer_replay() -> Result<()> {
    let root = std::path::PathBuf::from("runs/evidence-layer-2026-10-30");
    let baseline: Value =
        serde_json::from_slice(&std::fs::read(root.join("baseline-live/results.json"))?)?;
    let fixture = std::fs::read("feasibility/evidence-layer-cases-v1.json")?;
    let plan: Value = serde_json::from_slice(&fixture)?;
    ensure!(
        baseline["cases"]
            .as_array()
            .context("baseline cases")?
            .len()
            == plan["cases"].as_array().context("fixture cases")?.len(),
        "complete baseline required"
    );
    let out = root
        .join(std::env::var("WEB_SEARCH_EVIDENCE_OUTPUT").unwrap_or_else(|_| "layer-live".into()));
    std::fs::create_dir(&out)?;
    let model = Model::from_openrouter(
        Path::new(&std::env::var("WEB_SEARCH_OPENROUTER_KEY_FILE")?),
        "deepseek/deepseek-v4.1-flash-1017",
        std::env::var("WEB_SEARCH_OPENROUTER_PROXY").ok().as_deref(),
    )?;
    model.check().await?;
    crate::save(
        &out.join("manifest.json"),
        &json!({"fixture_sha256":format!("{:x}",Sha256::digest(&fixture)),"evidence_prompt_sha256":format!("{:x}",Sha256::digest(crate::evidence::PROMPT.as_bytes())),"initial_prompt_sha256":format!("{:x}",Sha256::digest(crate::model::PROMPT.as_bytes())),"binary_sha256":format!("{:x}",Sha256::digest(std::fs::read(std::env::current_exe()?)?)),"shared_initial_calls":true,"new_search_calls":0}),
    )?;
    let mut blocked: HashMap<String, String> =
        serde_json::from_value(baseline["blocked_document_hosts"].clone())?;
    let cache_path = root.join("source-cache/manifest.json");
    let cache: Value = if cache_path.exists() {
        serde_json::from_slice(&std::fs::read(cache_path)?)?
    } else {
        json!({})
    };
    if let Some(hosts) = cache["blocked_document_hosts"].as_object() {
        for (host, reason) in hosts {
            blocked.insert(
                host.clone(),
                reason.as_str().context("blocked reason")?.into(),
            );
        }
    }
    let mut rows = Vec::new();
    let mut cost = 0.0;
    for base in baseline["cases"].as_array().unwrap() {
        let case: Case = serde_json::from_value(
            plan["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["id"] == base["id"])
                .context("case ID")?
                .clone(),
        )?;
        let candidates: Vec<Candidate> = serde_json::from_value(base["candidates"].clone())?;
        let sources: Vec<policy::Source> = serde_json::from_value(base["sources"].clone())?;
        let (exported, rank) = policy::export(&case, &sources)?;
        ensure!(
            serde_json::to_value(exported)? == base["candidates"],
            "frozen pool mismatch"
        );
        let mut classification = Classifications::default();
        let mut shared = Vec::new();
        let mut initial_error = None;
        for call in base["model_calls"].as_array().unwrap() {
            if call["input"]["messages"][0]["content"] != crate::model::PROMPT {
                continue;
            }
            shared.push(call.clone());
            let context: Value = serde_json::from_str(
                call["input"]["messages"][1]["content"]
                    .as_str()
                    .context("initial context")?,
            )?;
            let expected: Vec<Candidate> = serde_json::from_value(context["candidates"].clone())?;
            match policy::labels(&call["response"], &expected, &case.query) {
                Ok(found) => {
                    classification.kinds.extend(found.kinds);
                    classification.evidence.extend(found.evidence);
                    classification.downgraded += found.downgraded;
                }
                Err(e) => {
                    initial_error = Some(e.to_string());
                    break;
                }
            }
        }
        let valid = initial_error.is_none() && classification.kinds.len() == candidates.len();
        let initial = classification.kinds.clone();
        let case_out = out.join(&case.id);
        std::fs::create_dir(&case_out)?;
        let cached = std::cell::Cell::new(0usize);
        let network = std::cell::Cell::new(0usize);
        let start = Instant::now();
        let shared_ms: u64 = shared.iter().filter_map(|c| c["elapsed_ms"].as_u64()).sum();
        let started = start
            .checked_sub(Duration::from_millis(shared_ms))
            .context("timer")?;
        let mut report = rescue::Report::default();
        if valid {
            let saved = &root;
            let cache = &cache;
            let ccase = &case;
            let cached = &cached;
            let network = &network;
            let case_out = &case_out;
            report = crate::evidence::run_with_reader(
                rescue::Input {
                    case: &case,
                    candidates: &candidates,
                    rank: &rank,
                    started,
                    output: Some(case_out),
                },
                &mut classification,
                &model,
                &mut blocked,
                3usize.saturating_sub(shared.len()),
                |c| async move {
                    let cached_record = &cache["documents"][format!("{}-{}", ccase.id, c.id)];
                    let path = if cached_record["path"].is_string() {
                        std::path::PathBuf::from(cached_record["path"].as_str().unwrap())
                    } else {
                        saved
                            .join("baseline-live")
                            .join(format!("{}-{}-document.json", ccase.id, c.id))
                    };
                    let record = if cached_record["record"].is_object() {
                        Some(&cached_record["record"])
                    } else {
                        base["page_verification"]["documents"]
                            .as_array()
                            .and_then(|ds| {
                                ds.iter()
                                    .find(|d| d["id"] == c.id && d["text_hash"].is_string())
                            })
                    };
                    if path.exists()
                        && let Some(record) = record
                    {
                        cached.set(cached.get() + 1);
                        let result: Result<rescue::Page> = (|| {
                            let doc: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
                            ensure!(
                                format!(
                                    "{:x}",
                                    Sha256::digest(
                                        doc["text"].as_str().context("text")?.as_bytes()
                                    )
                                ) == doc["text_hash"],
                                "cached document hash"
                            );
                            ensure!(record["url"] == c.url, "cached source URL mismatch");
                            crate::net::validate_url(
                                record["final_url"].as_str().context("final URL")?,
                            )?;
                            crate::save(
                                &case_out.join(format!("{}-{}-document.json", ccase.id, c.id)),
                                &doc,
                            )?;
                            rescue::page_from_document(
                                &c,
                                record["final_url"].as_str().context("final URL")?,
                                &doc,
                                &ccase.query,
                            )
                        })();
                        match result {
                            Ok(page) => (record.clone(), Some(page)),
                            Err(e) => (json!({"id":c.id,"error":e.to_string()}), None),
                        }
                    } else {
                        network.set(network.get() + 1);
                        rescue::read_document(&c, &ccase.query, &ccase.id, started, Some(case_out))
                            .await
                    }
                },
            )
            .await;
        } else {
            classification = Classifications::default();
        }
        for call in &report.calls {
            if call.get("response").is_some() {
                cost += call["response"]["usage"]["cost"]
                    .as_f64()
                    .context("reported cost required; stop paid run")?;
            }
        }
        let result = json!({"id":case.id,"query":case.query,"classifier_valid":valid,"initial_error":initial_error,"initial_labels":initial,"final_labels":classification.kinds,"initial_calls_shared":shared,"new_calls":report.calls,"new_model_elapsed_ms":start.elapsed().as_millis(),"logical_shared_initial_ms":shared_ms,"candidates":candidates,"final_top10":selected(&candidates,&classification.kinds,&rank,&case.query)?,"memory":report.memory,"proofs":report.proofs,"rejected":report.rejected,"errors":report.errors,"document_records":report.documents,"logical_reads":report.reads,"cached_reads":cached.get(),"network_reads":network.get()});
        crate::save(&case_out.join("result.json"), &result)?;
        rows.push(result);
        crate::save(
            &out.join("results.json"),
            &json!({"cases":rows,"new_reported_cost_usd":cost,"new_search_calls":0,"blocked_document_hosts":blocked}),
        )?;
        println!("layer {} complete; new_cost_usd={cost:.6}", case.id);
        ensure!(cost < 2.0, "layer cost budget");
        ensure!(
            !report
                .errors
                .iter()
                .any(|e| e.contains("HTTP 403") || e.contains("HTTP 429")),
            "model provider refusal; stop paid run"
        );
    }
    Ok(())
}

#[test]
#[ignore = "offline replay of captured evidence A/B; requires completed layer-production artifacts; no network"]
fn evidence_layer_first_round_replay() -> Result<()> {
    let root = std::path::PathBuf::from("runs/evidence-layer-2026-10-30");
    let base: Value =
        serde_json::from_slice(&std::fs::read(root.join("baseline-live/results.json"))?)?;
    let layer: Value = serde_json::from_slice(&std::fs::read(root.join("layer-v2/results.json"))?)?;
    let mut rows = Vec::new();
    for row in layer["cases"].as_array().context("layer cases")? {
        let b = base["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == row["id"])
            .context("baseline case")?;
        let case = Case {
            id: row["id"].as_str().unwrap().into(),
            query: row["query"].as_str().unwrap().into(),
            forms: vec![row["query"].as_str().unwrap().into()],
        };
        let candidates: Vec<Candidate> = serde_json::from_value(b["candidates"].clone())?;
        let sources: Vec<policy::Source> = serde_json::from_value(b["sources"].clone())?;
        let (_, rank) = policy::export(&case, &sources)?;
        let mut initial = Classifications {
            kinds: serde_json::from_value(row["initial_labels"].clone())?,
            ..Default::default()
        };
        let mut memory = crate::evidence::Memory::new(&case, &candidates, &initial, &rank)?;
        let mut valid = false;
        if let Some(call) = row["new_calls"].as_array().and_then(|cs| cs.first()) {
            let context: Value = serde_json::from_str(
                call["input"]["messages"][1]["content"]
                    .as_str()
                    .context("context")?,
            )?;
            let ids: Vec<String> = context["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["id"].as_str().unwrap().into())
                .collect();
            ensure!(
                serde_json::from_str::<Value>(&memory.context(&ids)?)? == context,
                "initial evidence context drift"
            );
            valid = call["warning"].is_null() && memory.apply(&call["response"], &ids).is_ok();
        }
        memory.finish(&candidates, &mut initial, &mut rescue::Report::default());
        ensure!(
            row["new_calls"].as_array().unwrap().len()
                + row["initial_calls_shared"].as_array().unwrap().len()
                <= 3,
            "call budget"
        );
        ensure!(row["logical_reads"].as_u64().unwrap() <= 4, "read budget");
        rows.push(
            json!({"id":case.id,"first_evidence_valid":valid,"first_round_labels":initial.kinds}),
        );
    }
    crate::save(
        &root.join("first-round-v2.json"),
        &json!({"passed":true,"cases":rows}),
    )?;
    Ok(())
}

#[tokio::test]
#[ignore = "explicit frozen46 paired model benchmark; paid remote or controlled local reload; no web fetch"]
async fn paired_model_benchmark() -> Result<()> {
    let provider = std::env::var("WEB_SEARCH_BENCH_PROVIDER").context("provider required")?;
    let remote = provider == "openrouter";
    ensure!(remote || provider == "omlx", "provider");
    let out = std::path::PathBuf::from(
        std::env::var_os("WEB_SEARCH_BENCH_OUTPUT").context("new output required")?,
    );
    std::fs::create_dir(&out)?;
    let bytes = std::fs::read("feasibility/model-benchmark-corpus-2026-10-30.json")?;
    let data: Value = serde_json::from_slice(&bytes)?;
    let model = if remote {
        Model::from_openrouter(
            Path::new(&std::env::var("WEB_SEARCH_OPENROUTER_KEY_FILE").context("key file")?),
            "deepseek/deepseek-v4.1-flash-1017",
            std::env::var("WEB_SEARCH_OPENROUTER_PROXY").ok().as_deref(),
        )?
    } else {
        Model::from_settings(
            Path::new(&std::env::var("WEB_SEARCH_OMLX_SETTINGS").context("local settings")?),
            "MiMo-V2.6-Distill-Qwen-9B-OptiQ-4bit",
        )?
    };
    crate::save(
        &out.join("manifest.json"),
        &json!({"provider":provider,"model":model.name,"corpus_sha256":format!("{:x}",Sha256::digest(&bytes)),"prompt_sha256":format!("{:x}",Sha256::digest(crate::model::PROMPT.as_bytes())),"binary_sha256":format!("{:x}",Sha256::digest(std::fs::read(std::env::current_exe()?)?)),"no_new_web_fetch":true}),
    )?;
    model.check().await?;
    let mut rows = Vec::new();
    let mut total_calls = 0;
    let mut known_cost = 0.0;
    let mut unknown_cost = 0;
    for row in data["cases"].as_array().unwrap() {
        let case: Case = serde_json::from_value(row.clone())?;
        let candidates: Vec<Candidate> = serde_json::from_value(row["candidates"].clone())?;
        let sources: Vec<policy::Source> = serde_json::from_value(row["sources"].clone())?;
        let (_, rank) = policy::export(&case, &sources)?;
        let lifecycle = if remote {
            Value::Null
        } else {
            model.reload_for_benchmark().await?
        };
        let start = Instant::now();
        let mut calls = Vec::new();
        let mut errors = Vec::new();
        let mut checked = Classifications::default();
        let mut raw = HashMap::<String, Value>::new();
        for context in policy::batches(&case, &candidates)? {
            let input: Value = serde_json::from_str(&context)?;
            let expected: Vec<Candidate> = serde_json::from_value(input["candidates"].clone())?;
            let ids: Vec<_> = expected.iter().map(|c| c.id.clone()).collect();
            let remaining = Duration::from_secs(180).saturating_sub(start.elapsed());
            match model.classify(&context, &ids, remaining).await {
                Ok(call) => {
                    let parsed = policy::labels(&call["response"], &expected, &case.query);
                    let warning = !call["warning"].is_null();
                    calls.push(call);
                    if warning {
                        errors.push("model_warning".to_owned());
                        break;
                    }
                    match parsed {
                        Ok(found) => {
                            let raw_labels:Value=serde_json::from_str(calls.last().unwrap()["response"]["choices"][0]["message"]["content"].as_str().unwrap())?;
                            raw.extend(
                                raw_labels["labels"]
                                    .as_object()
                                    .unwrap()
                                    .iter()
                                    .map(|(id, r)| (id.clone(), r["kind"].clone())),
                            );
                            checked.kinds.extend(found.kinds);
                            checked.evidence.extend(found.evidence);
                            checked.downgraded += found.downgraded;
                        }
                        Err(e) => {
                            errors.push(format!("classifier_invalid: {e}"));
                            break;
                        }
                    }
                }
                Err(e) => {
                    errors.push(e.to_string());
                    calls.push(json!({"error":e.to_string()}));
                    break;
                }
            }
        }
        let valid = checked.kinds.len() == candidates.len();
        let serp_elapsed = start.elapsed().as_millis();
        let initial = checked.kinds.clone();
        let initial_calls = calls.len();
        let mut report = rescue::Report::default();
        let mut body_skip = "not_body_group";
        let body_start = Instant::now();
        if row["group"] == "recall8_full_mixed_SERP_body" && valid && calls.len() + 2 <= 3 {
            body_skip = "no_eligible_cached_pages";
            let mut pages = Vec::new();
            for d in row["body_documents"].as_array().unwrap() {
                let c = candidates.iter().find(|c| c.id == d["id"]).unwrap();
                if checked.kinds.get(&c.id) != Some(&1) {
                    continue;
                }
                let page = rescue::page_from_document(
                    c,
                    d["final_url"].as_str().unwrap(),
                    &d["document"],
                    &case.query,
                )?;
                if rescue::eligible(&page, &case.query) && !page.passages.is_empty() {
                    pages.push(page);
                }
            }
            report.requirements = rescue::requirements(&case.query);
            if !pages.is_empty() {
                body_skip = "attempted";
                if let Err(e) = rescue::assess(
                    rescue::Assessment {
                        case: &case,
                        pages: &pages,
                        started: start,
                        output: Some(&out),
                    },
                    &mut checked,
                    &model,
                    &mut report,
                )
                .await
                {
                    errors.push(format!("body: {e}"));
                }
                calls.append(&mut report.calls);
            }
        }
        for call in &calls {
            if remote {
                if let Some(cost) = call["response"]["usage"]["cost"].as_f64() {
                    known_cost += cost;
                } else {
                    unknown_cost += 1;
                }
            }
        }
        total_calls += calls.len();
        let result = json!({"id":case.id,"group":row["group"],"valid":valid,"errors":errors,"lifecycle":lifecycle,"raw_labels":raw,"initial_labels":initial,"final_labels":checked.kinds,"downgraded":checked.downgraded,"serp_top10":selected(&candidates,&initial,&rank,&case.query)?,"final_top10":selected(&candidates,&checked.kinds,&rank,&case.query)?,"serp_elapsed_ms":serp_elapsed,"body_elapsed_ms":body_start.elapsed().as_millis(),"elapsed_ms":start.elapsed().as_millis(),"initial_calls":initial_calls,"body_skipped":body_skip,"promotions":report.proofs,"rejected":report.rejected,"calls":calls});
        crate::save(&out.join(format!("{}-result.json", case.id)), &result)?;
        println!(
            "{} {} valid={} calls={} promoted={} elapsed_ms={} cost_usd={:.6}",
            provider,
            case.id,
            valid,
            result["calls"].as_array().unwrap().len(),
            report.proofs.len(),
            result["elapsed_ms"],
            known_cost
        );
        rows.push(result);
        crate::save(
            &out.join("results.json"),
            &json!({"provider":provider,"model":model.name,"cases":rows,"total_calls":total_calls,"known_cost_usd":if remote{Some(known_cost)}else{None},"unknown_cost_calls":unknown_cost,"new_search_calls":0,"new_target_reads":0}),
        )?;
        if remote
            && (known_cost >= 2.0
                || unknown_cost >= 3
                || total_calls >= 90
                || errors
                    .iter()
                    .any(|e| e.contains("HTTP 403") || e.contains("HTTP 429")))
        {
            anyhow::bail!("benchmark provider/budget stop; partial results preserved");
        }
    }
    Ok(())
}
