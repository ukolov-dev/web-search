use crate::policy::{Candidate, Case};
use crate::{model::Model, policy::Classifications};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub anchor: String,
    pub requirement: String,
}
#[derive(Serialize)]
pub struct Page {
    pub id: String,
    pub url: String,
    pub final_url: String,
    pub text_hash: String,
    pub title: String,
    pub passages: Vec<String>,
    pub publication: Value,
    #[serde(skip)]
    pub text: String,
}
#[derive(Default)]
pub struct Report {
    pub requirements: Vec<Requirement>,
    pub documents: Vec<Value>,
    pub calls: Vec<Value>,
    pub proofs: HashMap<String, Value>,
    pub rejected: HashMap<String, String>,
    pub errors: Vec<String>,
    pub reads: usize,
    pub promoted: usize,
    pub memory: Value,
    pub reranker_calls: Vec<Value>,
}
fn compact(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn response(raw: &Value) -> Result<&str> {
    ensure!(
        raw["choices"][0]["finish_reason"] == "stop",
        "incomplete finish"
    );
    let msg = &raw["choices"][0]["message"];
    ensure!(
        msg["tool_calls"].as_array().is_none_or(|a| a.is_empty()),
        "unexpected tools"
    );
    msg["content"].as_str().context("missing content")
}
pub fn requirements(query: &str) -> Vec<Requirement> {
    ["Quote evidence directly addressing the main subject and requested action of the ORIGINAL question.",
     "Quote evidence demonstrating ALL requested relationships, differences or behavior; mere mentions are insufficient.",
     "Quote evidence satisfying the ORIGINAL question's constraints, including named APIs, version/date/platform. If no extra constraint is asked, quote the concrete main behavior instead."]
    .into_iter().map(|text|Requirement{anchor:query.into(),requirement:text.into()}).collect()
}
pub fn passages(text: &str, query: &str) -> Vec<String> {
    let query = query
        .split_whitespace()
        .filter(|w| !w.to_lowercase().starts_with("site:"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut segments = Vec::new();
    let mut segment = String::new();
    for line in text.split_inclusive('\n') {
        if line.contains("....") {
            if !segment.is_empty() {
                segments.push(std::mem::take(&mut segment));
            }
        } else {
            segment.push_str(line);
        }
    }
    if !segment.is_empty() {
        segments.push(segment);
    }
    let original = compact(text);
    let terms: Vec<_> = query
        .split(|c: char| {
            !(c.is_ascii_alphanumeric()
                || ('а'..='я').contains(&c)
                || c == 'ё'
                || matches!(c, '_' | '-' | '.'))
        })
        .filter(|w| {
            w.len() > 2
                && !matches!(
                    w.to_lowercase().as_str(),
                    "documentation" | "docs" | "official" | "the" | "and"
                )
        })
        .map(str::to_lowercase)
        .collect();
    let mut anchors: Vec<_> = terms
        .iter()
        .filter(|w| original.to_lowercase().contains(w.as_str()))
        .cloned()
        .collect();
    anchors.sort_by_key(|a| original.to_lowercase().matches(a).count());
    anchors.dedup();
    let mut selected = Vec::new();
    for anchor in std::iter::once(query).chain(anchors) {
        let wanted: Vec<_> = anchor
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let best = segments
            .iter()
            .map(|s| crate::policy::excerpt(s, &anchor, 1200))
            .filter(|p| p.chars().count() >= 16)
            .max_by_key(|p| {
                wanted
                    .iter()
                    .filter(|w| p.to_lowercase().contains(w.as_str()))
                    .count()
            });
        if let Some(p) = best
            && original.contains(&p)
            && !selected.contains(&p)
        {
            selected.push(p);
        }
        if selected.len() == 2 {
            break;
        }
    }
    selected
}

fn verified_passages(raw: &Value, query: &str, pages: &[Page]) -> Result<HashMap<String, Value>> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Response {
        #[serde(deserialize_with = "crate::policy::unique_label_map")]
        labels: HashMap<String, Value>,
    }
    let parsed: Response = serde_json::from_str(response(raw)?)?;
    ensure!(parsed.labels.len() == pages.len(), "all page IDs required");
    let mut proofs = HashMap::new();
    for (id, row) in parsed.labels {
        let page = pages
            .iter()
            .find(|p| p.id == id)
            .context("unknown page ID")?;
        ensure!(
            row.as_object().is_some_and(|r| r.len() == 2),
            "invalid page label fields"
        );
        let kind = row["kind"].as_str().context("kind")?;
        ensure!(
            ["direct", "background", "irrelevant"].contains(&kind),
            "invalid page label"
        );
        let slot = row["passage"].as_str().context("passage")?;
        if kind != "direct" {
            ensure!(slot.is_empty(), "non-direct passage");
            continue;
        }
        let selected = match slot {
            "P1" => 0,
            "P2" => 1,
            _ => anyhow::bail!("invalid source passage ID"),
        };
        ensure!(selected < page.passages.len(), "unknown source passage ID");
        if !eligible(page, query) {
            continue;
        }
        let cited = Candidate {
            id: id.clone(),
            url: page.final_url.clone(),
            title: page.title.clone(),
            snippet: page.passages.join(" "),
            document_http_status: Some(200),
            fetch_method: "http_extract".into(),
        };
        if !crate::policy::missing_literals(query, &cited).is_empty() {
            continue;
        }
        let mut evidence = Vec::new();
        for i in
            std::iter::once(selected).chain((0..page.passages.len()).filter(|&i| i != selected))
        {
            let quote = &page.passages[i];
            ensure!(
                quote.chars().count() >= 16 && compact(&page.text).contains(quote),
                "passage is not contiguous fetched text"
            );
            evidence.push(json!({"quote":quote,"passage":format!("P{}",i+1),"source_url":page.final_url,"page_title":page.title,"text_hash":page.text_hash,"whitespace_normalized":true}));
        }
        proofs.insert(id, json!(evidence));
    }
    Ok(proofs)
}
pub fn audit(raw: &Value, ids: &[String]) -> Result<(Vec<String>, HashMap<String, String>)> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Response {
        #[serde(deserialize_with = "crate::policy::unique_label_map")]
        labels: HashMap<String, Value>,
    }
    let parsed: Response = serde_json::from_str(response(raw)?)?;
    ensure!(parsed.labels.len() == ids.len(), "all audit IDs required");
    let mut accepted = Vec::new();
    let mut rejected = HashMap::new();
    for (id, row) in parsed.labels {
        ensure!(
            ids.contains(&id) && row.as_object().is_some_and(|r| r.len() == 2),
            "unknown audit ID/properties"
        );
        let supported = row["supported"].as_bool().context("audit supported bool")?;
        let missing = row["missing_aspect"]
            .as_str()
            .context("audit missing aspect")?;
        ensure!(missing.chars().count() <= 240, "audit reason length");
        if supported && matches!(missing.trim(), "" | "none") {
            accepted.push(id);
        } else {
            rejected.insert(id, missing.into());
        }
    }
    Ok((accepted, rejected))
}

fn remaining(started: Instant) -> Duration {
    Duration::from_secs(180).saturating_sub(started.elapsed())
}

pub struct Input<'a> {
    pub case: &'a Case,
    pub candidates: &'a [Candidate],
    pub rank: &'a HashMap<String, usize>,
    pub started: Instant,
    pub output: Option<&'a Path>,
}
pub async fn run(
    input: Input<'_>,
    classification: &mut Classifications,
    model: &Model,
    blocked: &mut HashMap<String, String>,
) -> Report {
    let mut report = Report::default();
    if let Err(e) = run_inner(input, classification, model, blocked, &mut report).await {
        report.errors.push(e.to_string());
    }
    report
}
async fn run_inner(
    input: Input<'_>,
    classification: &mut Classifications,
    model: &Model,
    blocked: &mut HashMap<String, String>,
    report: &mut Report,
) -> Result<()> {
    let Input {
        case,
        candidates,
        rank,
        started,
        output,
    } = input;
    let site = crate::search::site_filter(&case.query)?;
    let mut chosen = Vec::new();
    for c in crate::policy::select(candidates, &classification.kinds, rank)? {
        if classification.kinds.get(&c.id) != Some(&1) {
            continue;
        }
        let result = crate::search::SearchResult {
            title: c.title.clone(),
            url: c.url.clone(),
            snippet: c.snippet.clone(),
            position: 1,
            result_source: None,
        };
        if crate::search::filter_site(site.as_ref(), vec![result]).is_ok() {
            chosen.push(c);
        }
    }
    if chosen.is_empty() {
        return Ok(());
    }
    report.requirements = requirements(&case.query);
    let mut pages = Vec::new();
    for c in chosen {
        if report.reads >= 4 {
            break;
        }
        if remaining(started).is_zero() {
            anyhow::bail!("deadline_exceeded");
        }
        let host = reqwest::Url::parse(&c.url)?
            .host_str()
            .context("host")?
            .to_owned();
        if let Some(reason) = blocked.get(&host) {
            report
                .errors
                .push(format!("{}: host_disabled: {reason}", c.id));
            report
                .documents
                .push(json!({"id":c.id,"url":c.url,"error":format!("host_disabled: {reason}")}));
            continue;
        }
        report.reads += 1;
        let (saved, page) = read_document(&c, &case.query, &case.id, started, output).await;
        let mut record = saved;
        let result = page.ok_or_else(|| {
            anyhow::anyhow!(record["error"].as_str().unwrap_or("read failed").to_owned())
        });
        match result {
            Ok(page) => {
                if eligible(&page, &case.query) {
                    pages.push(page);
                } else {
                    record["verification_skipped"] = json!("missing_literal_constraints");
                }
            }
            Err(e) => {
                let reason = e.to_string();
                if crate::search::blocks_source(crate::search::error_code(&reason)) {
                    blocked.insert(host, reason.clone());
                }
                report.errors.push(format!("{}: {}", c.id, reason));
                record["error"] = json!(reason);
            }
        }
        report.documents.push(record);
    }
    if pages.is_empty() {
        return Ok(());
    }
    assess(
        Assessment {
            case,
            pages: &pages,
            started,
            output,
        },
        classification,
        model,
        report,
    )
    .await
}

pub async fn read_document(
    c: &Candidate,
    query: &str,
    case_id: &str,
    started: Instant,
    output: Option<&Path>,
) -> (Value, Option<Page>) {
    #[cfg(test)]
    if std::env::var_os("WEB_SEARCH_DOCUMENT_CACHE").is_some()
        && std::env::var_os("WEB_SEARCH_RAW_CACHE").is_none()
    {
        return crate::reliability_benchmark::cached_read(c, query, case_id, started, output).await;
    }
    read_document_live(c, query, case_id, started, output).await
}

pub(crate) async fn read_document_live(
    c: &Candidate,
    query: &str,
    case_id: &str,
    started: Instant,
    output: Option<&Path>,
) -> (Value, Option<Page>) {
    let mut record = json!({"id":c.id,"url":c.url,"http_status":null});
    #[cfg(test)]
    let fetched = if let Some(root) = std::env::var_os("WEB_SEARCH_RAW_CACHE") {
        tokio::time::timeout(
            remaining(started),
            crate::reliability_benchmark::cached_fetch(&c.url, Path::new(&root)),
        )
        .await
        .map(|result| {
            result.map(|(page, hit)| {
                record["raw_snapshot_cache_hit"] = json!(hit);
                page
            })
        })
    } else {
        tokio::time::timeout(remaining(started), crate::net::fetch(&c.url)).await
    };
    #[cfg(not(test))]
    let fetched = tokio::time::timeout(remaining(started), crate::net::fetch(&c.url)).await;
    let result: Result<Page> = async {
        let page = fetched.context("deadline_exceeded")??;
        record["http_status"] = json!(page.status);
        record["final_url"] = json!(page.url);
        record["hops"] = json!(page.hops);
        if crate::cache::enabled() {
            record["source_cache"] = serde_json::to_value(&page.cache)?;
        }
        record["html_hash"] = json!(format!("{:x}", Sha256::digest(page.html.as_bytes())));
        if let Some(out) = output {
            std::fs::write(out.join(format!("{}-{}.html", case_id, c.id)), &page.html)?;
        }
        ensure!(page.status == 200, "HTTP {}", page.status);
        crate::search::navigation_guard(&page.html)?;
        let final_result = crate::search::SearchResult {
            title: c.title.clone(),
            url: page.url.clone(),
            snippet: String::new(),
            position: 1,
            result_source: None,
        };
        crate::search::filter_site(
            crate::search::site_filter(query)?.as_ref(),
            vec![final_result],
        )
        .context("site_redirect_mismatch")?;
        let url = page.url.clone();
        let accepted = page.clone();
        let doc = tokio::task::spawn_blocking(move || {
            crate::extract_cached(page.html, &url, page.cache.allowed)
        })
        .await??;
        let source_cache = accepted.store().await;
        if crate::cache::enabled() {
            record["source_cache"] = serde_json::to_value(source_cache)?;
        }
        if let Some(out) = output {
            crate::save(
                &out.join(format!("{}-{}-document.json", case_id, c.id)),
                &doc,
            )?;
        }
        record["text_hash"] = doc["text_hash"].clone();
        record["content_scope"] = doc["content_scope"].clone();
        record["truncated"] = doc["truncated"].clone();
        page_from_document(c, &page.url, &doc, query)
    }
    .await;
    match result {
        Ok(page) => (record, Some(page)),
        Err(e) => {
            record["error"] = json!(e.to_string());
            (record, None)
        }
    }
}

pub fn eligible(page: &Page, query: &str) -> bool {
    let c = Candidate {
        id: page.id.clone(),
        url: page.final_url.clone(),
        title: page.title.clone(),
        snippet: page.text.clone(),
        document_http_status: Some(200),
        fetch_method: "http_extract".into(),
    };
    crate::policy::missing_literals(query, &c).is_empty()
}
pub fn page_from_document(
    c: &Candidate,
    final_url: &str,
    doc: &Value,
    query: &str,
) -> Result<Page> {
    let text = doc["text"].as_str().context("text")?.to_owned();
    Ok(Page {
        id: c.id.clone(),
        url: c.url.clone(),
        final_url: final_url.into(),
        text_hash: doc["text_hash"].as_str().context("hash")?.into(),
        title: doc["page_title"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| doc["title"].as_str())
            .unwrap_or("")
            .chars()
            .take(500)
            .collect(),
        passages: passages(&text, query),
        publication: doc["news"].clone(),
        text,
    })
}
pub struct Assessment<'a> {
    pub case: &'a Case,
    pub pages: &'a [Page],
    pub started: Instant,
    pub output: Option<&'a Path>,
}
pub async fn assess(
    input: Assessment<'_>,
    classification: &mut Classifications,
    model: &Model,
    report: &mut Report,
) -> Result<()> {
    let Assessment {
        case,
        pages,
        started,
        output,
    } = input;
    let context = serde_json::to_string(
        &json!({"question":case.query,"pages":pages.iter().map(|p|json!({"id":p.id,"page_title":p.title,"passages":p.passages.iter().enumerate().map(|(i,t)|json!({"id":format!("P{}",i+1),"text":t})).collect::<Vec<_>>()})).collect::<Vec<_>>()}),
    )?;
    let ids = pages.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
    let call = model.review_pages(&context, &ids, remaining(started)).await;
    match call {
        Ok(call) => {
            if let Some(out) = output {
                crate::save(
                    &out.join(format!("{}-page-verification.json", case.id)),
                    &call,
                )?;
            }
            report.calls.push(call);
        }
        Err(e) => {
            report.calls.push(json!({"error":e.to_string()}));
            return Err(e);
        }
    }
    let call = report.calls.last().unwrap();
    ensure!(call["warning"].is_null(), "model warning");
    let proposals = verified_passages(&call["response"], &case.query, pages)?;
    if proposals.is_empty() {
        return Ok(());
    }
    let proposed: Vec<_> = pages.iter().filter(|p| proposals.contains_key(&p.id)).map(|p| {
        json!({"id":p.id,"page_title":p.title,"first_text":p.text.chars().take(600).collect::<String>(),"proposed_quote":proposals[&p.id][0]["quote"],"passages":p.passages})
    }).collect();
    let context = serde_json::to_string(&json!({"question":case.query,"proposals":proposed}))?;
    let ids: Vec<String> = proposals.keys().cloned().collect();
    let call = model.audit_pages(&context, &ids, remaining(started)).await;
    match call {
        Ok(call) => {
            if let Some(out) = output {
                crate::save(&out.join(format!("{}-page-audit.json", case.id)), &call)?;
            }
            report.calls.push(call);
        }
        Err(e) => {
            report.calls.push(json!({"error":e.to_string()}));
            return Err(e);
        }
    }
    let call = report.calls.last().unwrap();
    ensure!(call["warning"].is_null(), "audit warning");
    let (accepted, rejected) = audit(&call["response"], &ids)?;
    report.rejected = rejected;
    report.proofs = proposals
        .into_iter()
        .filter(|(id, _)| accepted.contains(id))
        .collect();
    report.promoted = report.proofs.len();
    for (id, proof) in &report.proofs {
        classification.kinds.insert(id.clone(), 0);
        classification.evidence.insert(
            id.clone(),
            proof[0]["quote"]
                .as_str()
                .unwrap()
                .chars()
                .take(160)
                .collect(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn raw(content: Value) -> Value {
        json!({"choices":[{"finish_reason":"stop","message":{"content":content.to_string()}}]})
    }
    #[test]
    fn passage_ids_cannot_invent_or_join_source_text() {
        let text = "HTTP 206 serves a partial response to Range. If-Range checks the validator before serving ranges.";
        let mut page = Page {
            publication: Value::Null,
            id: "C001".into(),
            url: "https://example.org".into(),
            final_url: "https://example.org".into(),
            text_hash: "hash".into(),
            title: "HTTP ranges".into(),
            passages: vec![text.into()],
            text: text.into(),
        };
        let make = |slot: &str| raw(json!({"labels":{"C001":{"kind":"direct","passage":slot}}}));
        assert_eq!(
            verified_passages(
                &make("P1"),
                "HTTP 206 Range If-Range",
                std::slice::from_ref(&page)
            )
            .unwrap()
            .len(),
            1
        );
        for slot in ["P2", "P0", "invented quote", ""] {
            assert!(
                verified_passages(
                    &make(slot),
                    "HTTP 206 Range If-Range",
                    std::slice::from_ref(&page)
                )
                .is_err()
            );
        }
        page.passages = vec!["HTTP 206 partial If-Range checks invented".into()];
        assert!(verified_passages(&make("P1"), "HTTP 206 Range If-Range", &[page]).is_err());
    }

    #[test]
    fn fetched_head_title_preserves_technology_constraint() {
        let mut page = Page {
            publication: Value::Null,
            id: "C001".into(),
            url: "https://example.org".into(),
            final_url: "https://example.org".into(),
            text_hash: "hash".into(),
            title: "PostgreSQL Documentation: TRUNCATE".into(),
            passages: vec![],
            text: "TRUNCATE versus DELETE. RESTART IDENTITY resets sequences.".into(),
        };
        let query = "PostgreSQL TRUNCATE DELETE RESTART IDENTITY отличие";
        assert!(eligible(&page, query));
        page.title = "TRUNCATE".into();
        assert!(!eligible(&page, query));
    }

    #[test]
    fn table_of_contents_is_not_used_and_no_gap_is_fabricated() {
        let text = "Heading before index\nIf-Range ......... 9\n206 Partial Content ....... 10\nIf-Range tests a validator before applying the Range request and a 206 partial response.";
        let found = passages(text, "HTTP 206 Range If-Range");
        assert!(!found.is_empty());
        assert!(
            found
                .iter()
                .all(|p| !p.contains("....") && compact(text).contains(p))
        );
    }
    #[test]
    fn skeptical_audit_must_agree_without_a_missing_aspect() {
        let ids = vec!["C001".into()];
        let good = raw(json!({"labels":{"C001":{"supported":true,"missing_aspect":""}}}));
        assert_eq!(audit(&good, &ids).unwrap().0, ids);
        let none = raw(json!({"labels":{"C001":{"supported":true,"missing_aspect":"none"}}}));
        assert_eq!(audit(&none, &ids).unwrap().0, ids);
        let partial = raw(
            json!({"labels":{"C001":{"supported":true,"missing_aspect":"Only a table of contents"}}}),
        );
        assert!(audit(&partial, &ids).unwrap().0.is_empty());
        assert!(
            audit(
                &raw(json!({"labels":{"unknown":{"supported":true,"missing_aspect":""}}})),
                &ids
            )
            .is_err()
        );
    }

    #[test]
    fn requirements_are_fixed_neutral_checks_and_never_generated_facts() {
        let q = "Git merge revert -m mainline parent документация";
        let plan = requirements(q);
        assert_eq!(plan.len(), 3);
        assert!(plan.iter().all(|r| r.anchor == q));
        assert!(!plan.iter().any(|r| r.requirement.contains("merge driver")));
    }
}
