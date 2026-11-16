//! Named stage-only diagnostic; ordinary tests never load run files or call a model.
use crate::{
    evidence::{self, Entry, Memory, Passage},
    policy::{Candidate, Classifications},
    rescue,
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

fn restore(path: &Path) -> Result<Memory> {
    let raw: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let mut entries = Vec::new();
    for entry in raw["entries"].as_array().context("entries")? {
        let mut passages = Vec::new();
        for p in entry["passages"].as_array().context("passages")? {
            let field = match p["field"].as_str().context("field")? {
                "title" => "title",
                "snippet" => "snippet",
                "body" => "body",
                "page_title" => "page_title",
                "publication_date" => "publication_date",
                other => bail!("unexpected restored field: {other}"),
            };
            let text = |key: &str| -> Result<String> {
                Ok(p[key].as_str().with_context(|| key.to_owned())?.into())
            };
            passages.push(Passage {
                id: text("id")?,
                text: text("text")?,
                source_url: text("source_url")?,
                text_hash: text("text_hash")?,
                normalized_text_hash: text("normalized_text_hash")?,
                field,
                start: p["start"].as_u64().context("start")? as usize,
                end: p["end"].as_u64().context("end")? as usize,
            });
        }
        entries.push(Entry {
            id: entry["id"].as_str().context("id")?.into(),
            passages,
            checks: serde_json::from_value(entry["checks"].clone())?,
            read_attempted: true,
            audit_supported: None,
            audit_reason: String::new(),
        });
    }
    Ok(Memory {
        version: evidence::VERSION,
        query: raw["query"].as_str().context("query")?.into(),
        subject: String::new(),
        requirements: vec![],
        entries,
        reviewed_rounds: 0,
        offset_unit: "UTF-8 bytes in whitespace-normalized field",
        witness_rejections: vec![],
        audit_conflicts: vec![],
        request_window: raw["request_window"].clone(),
    })
}

fn remote() -> Result<crate::model::Model> {
    crate::model::Model::from_openrouter(
        Path::new(&std::env::var("WEB_SEARCH_OPENROUTER_KEY_FILE")?),
        "deepseek/deepseek-v4.1-flash-1017",
        std::env::var("WEB_SEARCH_OPENROUTER_PROXY").ok().as_deref(),
    )
}

#[tokio::test]
#[ignore = "explicit NN03 restored source/Qwen-context coverage+audit replay; max2 OpenRouter calls; no search or HTTP target reads"]
async fn old_nn03_coverage_stage_replay() -> Result<()> {
    let root =
        PathBuf::from(std::env::var("WEB_SEARCH_COVERAGE_REPLAY_ROOT").context("replay root")?);
    std::fs::create_dir_all(&root)?;
    let workspace = PathBuf::from(
        std::env::var("WEB_SEARCH_COVERAGE_WORKSPACE")
            .unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").into()),
    );
    let input_root = workspace.join("runs/news-coverage-2026-11-03/safety");
    let mut memory = restore(&input_root.join("nn03-restored-memory.json"))?;
    let ids: Vec<_> = memory.entries.iter().map(|e| e.id.clone()).collect();
    ensure!(ids.len() == 2, "expected two historical sources");
    let context: Value = serde_json::from_str(&memory.context(&ids)?)?;
    let expected: Value = serde_json::from_slice(&std::fs::read(
        input_root.join("nn03-restored-context.json"),
    )?)?;
    ensure!(
        context == expected,
        "restored source/clock context mismatch"
    );
    crate::save(&root.join("source-context.json"), &context)?;
    let mut report = rescue::Report::default();
    let mut error = None;
    if let Some(recorded) = std::env::var_os("WEB_SEARCH_COVERAGE_RECORDED") {
        let recorded = PathBuf::from(recorded);
        let call: Value = serde_json::from_slice(&std::fs::read(
            recorded.join("old-nn03-evidence-round1.json"),
        )?)?;
        if let Err(e) = memory.apply_atomic(&call["response"], &ids) {
            error = Some(e.to_string());
        }
        report.calls.push(call);
        let audit_path = recorded.join("old-nn03-evidence-audit.json");
        if error.is_none() && std::env::var_os("WEB_SEARCH_COVERAGE_RECORDED_AUDIT").is_some() {
            let model = remote()?;
            model.check().await?;
            if let Err(e) = evidence::audit(
                &mut memory,
                &model,
                Instant::now(),
                Some(&root),
                "old-nn03",
                &mut report,
            )
            .await
            {
                error = Some(e.to_string());
            }
        } else if error.is_none() && audit_path.exists() {
            let call: Value = serde_json::from_slice(&std::fs::read(audit_path)?)?;
            let audit_ids = memory.audit_ids();
            if let Err(e) = memory.apply_audit(&call["response"], &audit_ids) {
                error = Some(e.to_string());
            }
            report.calls.push(call);
        }
    } else {
        let model = remote()?;
        model.check().await?;
        let started = Instant::now();
        if let Err(e) = evidence::review(
            &mut memory,
            &ids,
            &model,
            started,
            Some(&root),
            "old-nn03",
            &mut report,
        )
        .await
        {
            error = Some(e.to_string());
        }
        if error.is_none()
            && let Err(e) = evidence::audit(
                &mut memory,
                &model,
                started,
                Some(&root),
                "old-nn03",
                &mut report,
            )
            .await
        {
            error = Some(e.to_string());
        }
    }
    let mut labels = Classifications::default();
    let candidates: Vec<_> = memory
        .entries
        .iter()
        .map(|e| {
            let t = e.passages.iter().find(|p| p.id == "T").unwrap();
            let s = e.passages.iter().find(|p| p.id == "S").unwrap();
            labels.kinds.insert(e.id.clone(), 1);
            Candidate {
                id: e.id.clone(),
                url: t.source_url.clone(),
                title: t.text.clone(),
                snippet: s.text.clone(),
                document_http_status: Some(200),
                fetch_method: "restored-original-qwen-context".into(),
            }
        })
        .collect();
    memory.finish(&candidates, &mut labels, &mut report);
    let original: Value = serde_json::from_slice(&std::fs::read(
        workspace.join("runs/news-navigation-2026-11-03/benchmark/fixed/nn03.json"),
    )?)?;
    let mut documents = Vec::new();
    for record in
        original["response"]["result"]["structuredContent"]["page_verification"]["documents"]
            .as_array()
            .context("old documents")?
    {
        if !ids.iter().any(|id| record["id"] == *id) {
            continue;
        }
        let mut record = record.clone();
        record["news"]["date_check"] =
            crate::news::check_date(&record["news"]["metadata"], &memory.request_window);
        ensure!(
            record["news"]["date_check"]["eligible"] == true,
            "historical source date/type is not eligible"
        );
        documents.push(record);
    }
    ensure!(documents.len() == 2, "old date/type records incomplete");
    crate::news_search::Prepared {
        candidates,
        documents,
        cached: std::collections::HashMap::new(),
        trace: Value::Null,
        reads: 0,
    }
    .enforce(&mut labels, &mut report);
    let recorded = std::env::var_os("WEB_SEARCH_COVERAGE_RECORDED").is_some();
    let new_calls = if !recorded {
        report.calls.len()
    } else if std::env::var_os("WEB_SEARCH_COVERAGE_RECORDED_AUDIT").is_some() {
        report.calls.len().saturating_sub(1)
    } else {
        0
    };
    let result = json!({"version":evidence::VERSION,"stage_only":true,"query":memory.query,"source_context_exactly_restored":true,"news_type_date_gates_rechecked":true,"historical_model_response_available":false,"new_model_calls":new_calls,"source_calls":0,"target_reads":0,"qwen_calls":0,"qwen_context":"exact stored v11 selected ranges","error":error,"kinds":labels.kinds,"proofs":report.proofs,"rejected":report.rejected,"memory":report.memory});
    crate::save(&root.join("result.json"), &result)?;
    println!(
        "{}",
        json!({"version":evidence::VERSION,"error":result["error"],"kinds":result["kinds"]})
    );
    Ok(())
}
