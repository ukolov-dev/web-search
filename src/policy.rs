use crate::{model::Model, search::SearchResult};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Deserialize, Serialize)]
pub struct Case {
    pub id: String,
    pub query: String,
    pub forms: Vec<String>,
}

pub fn cases() -> Vec<Case> {
    #[derive(Deserialize)]
    struct Plan {
        cases: Vec<Case>,
    }
    serde_json::from_str::<Plan>(include_str!("../feasibility/b17-policy.json"))
        .unwrap()
        .cases
}

pub fn case_for(query: &str) -> Result<Case> {
    crate::search::site_filter(query)?;
    // shortcut: frozen translations cover calibration cases; qualify a rewrite stage for arbitrary queries.
    Ok(cases()
        .into_iter()
        .find(|c| c.query == query)
        .unwrap_or(Case {
            id: "custom".into(),
            query: query.into(),
            forms: vec![query.into()],
        }))
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Source {
    pub engine: String,
    pub query: String,
    pub results: Vec<SearchResult>,
    pub error: Option<String>,
    pub http_status: Option<u16>,
    pub hops: Vec<String>,
    pub html_hash: Option<String>,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Candidate {
    pub url: String,
    pub title: String,
    pub snippet: String,
    pub document_http_status: Option<u16>,
    pub fetch_method: String,
    pub id: String,
}

pub fn canonical(raw: &str, classifier: bool) -> Result<String> {
    let u = reqwest::Url::parse(raw)?;
    let mut host = u.host_str().context("no canonical host")?.to_lowercase();
    if let Some(stripped) = host.strip_prefix("www.") {
        host = stripped.into();
    }
    if let Some(port) = u.port()
        && port != 80
        && port != 443
    {
        host.push_str(&format!(":{port}"));
    }
    let mut pairs: Vec<_> = u
        .query_pairs()
        .filter(|(k, _)| {
            if classifier {
                !k.starts_with("utm_") && !matches!(k.as_ref(), "art" | "fbclid" | "gclid")
            } else {
                let k = k.to_lowercase();
                !k.starts_with("utm_") && !matches!(k.as_str(), "fbclid" | "gclid")
            }
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    pairs.sort();
    let mut encoded = reqwest::Url::parse("https://canonical.invalid/")?;
    encoded.query_pairs_mut().extend_pairs(pairs);
    let query = encoded.query().unwrap_or("").to_owned();
    Ok(format!(
        "{host}{}{}",
        if u.path().trim_end_matches('/').is_empty() {
            "/"
        } else {
            u.path().trim_end_matches('/')
        },
        if query.is_empty() {
            String::new()
        } else {
            format!("?{query}")
        }
    ))
}

fn compact(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn prefix(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

pub fn excerpt(text: &str, query: &str, limit: usize) -> String {
    let normalized = compact(text);
    let chars: Vec<_> = normalized.chars().collect();
    if chars.len() <= limit {
        return normalized;
    }
    let stops = [
        "documentation",
        "the",
        "and",
        "http",
        "site",
        "org",
        "lang",
        "doc",
        "rust",
        "com",
    ];
    let terms: HashSet<_> = query
        .to_lowercase()
        .split(|c: char| {
            !(c.is_ascii_lowercase()
                || c.is_ascii_digit()
                || ('а'..='я').contains(&c)
                || c == 'ё'
                || c == '_')
        })
        .filter(|s| !s.is_empty() && !stops.contains(s))
        .map(str::to_owned)
        .collect();
    let mut best = (i64::MIN, String::new());
    for start in (0..chars.len()).step_by((limit / 2).max(1)) {
        let text: String = chars[start..(start + limit).min(chars.len())]
            .iter()
            .collect();
        let lower = text.to_lowercase();
        let hits = terms.iter().filter(|t| lower.contains(t.as_str())).count();
        let cs: Vec<_> = text.chars().collect();
        let sentences = cs
            .iter()
            .enumerate()
            .filter(|(i, c)| {
                matches!(**c, '.' | '!' | '?') && (*i + 1 == cs.len() || cs[*i + 1] == ' ')
            })
            .count();
        let score = hits as i64 * 10
            + sentences.min(3) as i64 * 3
            + if text.split_whitespace().count() >= 30 {
                10
            } else {
                -10
            };
        if score > best.0 {
            best = (score, text);
        }
    }
    best.1
}

pub fn export(case: &Case, sources: &[Source]) -> Result<(Vec<Candidate>, HashMap<String, usize>)> {
    let mut pool: BTreeMap<String, Candidate> = BTreeMap::new();
    let mut scores: HashMap<String, f64> = HashMap::new();
    let terms = format!("{} {}", case.query, case.forms.join(" "));
    for source in sources {
        let mut seen = HashSet::new();
        for (i, r) in source.results.iter().take(10).enumerate() {
            let key = canonical(&r.url, false)?;
            if seen.insert(key.clone()) {
                *scores.entry(key.clone()).or_default() += 1.0 / (61 + i) as f64;
            }
            let c = Candidate {
                url: r.url.clone(),
                title: prefix(&compact(&r.title), 120),
                snippet: excerpt(&r.snippet, &terms, 300),
                document_http_status: None,
                fetch_method: "unknown".into(),
                id: String::new(),
            };
            if pool
                .get(&key)
                .is_none_or(|prior| c.snippet.chars().count() > prior.snippet.chars().count())
            {
                pool.insert(key, c);
            }
        }
    }
    let mut ranked: Vec<_> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut rank = HashMap::new();
    for (i, (key, _)) in ranked.into_iter().enumerate() {
        if let Some(c) = pool.get(&key) {
            rank.insert(canonical(&c.url, true)?, i);
        }
    }
    let candidates = pool
        .into_values()
        .enumerate()
        .map(|(i, mut c)| {
            c.id = format!("C{:03}", i + 1);
            c
        })
        .collect();
    Ok((candidates, rank))
}

// Match Python json.dumps(..., ensure_ascii=False) spacing without changing quoted text.
fn python_json<T: Serialize>(value: &T) -> Result<String> {
    let raw = serde_json::to_string(value)?;
    let mut result = String::new();
    let (mut quoted, mut escaped) = (false, false);
    for c in raw.chars() {
        result.push(c);
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' && quoted {
            escaped = true;
            continue;
        }
        if c == '"' {
            quoted = !quoted;
        }
        if !quoted && matches!(c, ',' | ':') {
            result.push(' ');
        }
    }
    Ok(result)
}

#[derive(Serialize)]
struct ModelContext<'a> {
    query: &'a str,
    candidates: &'a [Candidate],
}
pub fn batches(case: &Case, candidates: &[Candidate]) -> Result<Vec<String>> {
    if candidates.is_empty() {
        return Ok(vec![]);
    }
    let n = candidates.len().div_ceil(24);
    ensure!(
        n <= 3,
        "classification call budget: all IDs must be retained"
    );
    candidates
        .chunks(candidates.len().div_ceil(n))
        .map(|cs| {
            let context = python_json(&ModelContext {
                query: &case.query,
                candidates: cs,
            })?;
            ensure!(context.chars().count() <= 24_000, "source context budget");
            Ok(context)
        })
        .collect()
}

#[derive(Default)]
pub struct Classifications {
    pub kinds: HashMap<String, u8>,
    pub evidence: HashMap<String, String>,
    pub downgraded: usize,
}

pub(crate) fn unique_label_map<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<HashMap<String, Value>, D::Error> {
    struct Unique;
    impl<'de> serde::de::Visitor<'de> for Unique {
        type Value = HashMap<String, Value>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("unique candidate IDs")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut result = HashMap::new();
            while let Some((key, value)) = map.next_entry::<String, Value>()? {
                if result.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate ID"));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Unique)
}

pub(crate) fn literal_present(text: &str, term: &str) -> bool {
    text.match_indices(term).any(|(i, _)| {
        let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
        !text[..i].chars().next_back().is_some_and(word)
            && !text[i + term.len()..].chars().next().is_some_and(word)
    })
}

pub fn missing_literals(query: &str, candidate: &Candidate) -> Vec<String> {
    let text = format!("{} {}", candidate.title, candidate.snippet).to_lowercase();
    let tokens: Vec<_> = query
        .split_whitespace()
        .filter(|w| !w.to_lowercase().starts_with("site:"))
        .flat_map(|w| {
            w.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
        })
        .filter(|w| !w.is_empty())
        .collect();
    let mut missing = Vec::new();
    let codes: Vec<_> = tokens
        .iter()
        .filter(|w| {
            w.len() == 3
                && w.chars().all(|c| c.is_ascii_digit())
                && matches!(w.as_bytes()[0], b'1'..=b'5')
        })
        .collect();
    let comparison = [
        "comparison",
        "difference",
        "versus",
        " vs ",
        "сравнен",
        "отлич",
    ]
    .iter()
    .any(|w| query.to_lowercase().contains(w));
    for token in &tokens {
        if !comparison && codes.len() > 1 && codes.contains(&token) {
            continue;
        }
        let lower = token.to_lowercase();
        let numeric = token.chars().any(|c| c.is_ascii_digit());
        let api = token.contains('_')
            || token.contains('-')
            || (token.chars().any(|c| c.is_ascii_lowercase())
                && token.chars().skip(1).any(|c| c.is_ascii_uppercase()));
        let acronym = token.len() > 1
            && token.chars().all(|c| c.is_ascii_uppercase())
            && !matches!(lower.as_str(), "http" | "https");
        if (numeric || api || acronym) && !literal_present(&text, &lower) {
            missing.push((*token).to_owned());
        }
    }
    if !comparison && codes.len() > 1 && !codes.iter().any(|c| literal_present(&text, c)) {
        missing.push("HTTP status code".into());
    }
    let months = [
        ("january", "jan", "январ", "1월"),
        ("february", "feb", "феврал", "2월"),
        ("march", "mar", "март", "3월"),
        ("april", "apr", "апрел", "4월"),
        ("may", "may", "мая", "5월"),
        ("june", "jun", "июн", "6월"),
        ("july", "jul", "июл", "7월"),
        ("august", "aug", "август", "8월"),
        ("september", "sep", "сентябр", "9월"),
        ("october", "oct", "октябр", "10월"),
        ("november", "nov", "ноябр", "11월"),
        ("december", "dec", "декабр", "12월"),
    ];
    let q = query.to_lowercase();
    for (index, (full, abbr, ru, ko)) in months.iter().enumerate() {
        if (literal_present(&q, full) || q.contains(ru))
            && !(literal_present(&text, full)
                || literal_present(&text, abbr)
                || text.contains(ru)
                || literal_present(&text, ko)
                || text.contains(&format!("-{:02}-", index + 1)))
        {
            missing.push((*full).into());
        }
    }
    missing.sort();
    missing.dedup();
    missing
}

pub fn labels(raw: &Value, expected: &[Candidate], query: &str) -> Result<Classifications> {
    ensure!(
        raw["choices"][0]["finish_reason"] == "stop",
        "classifier incomplete finish"
    );
    let message = &raw["choices"][0]["message"];
    ensure!(
        message["tool_calls"]
            .as_array()
            .is_none_or(|a| a.is_empty()),
        "unexpected tool call"
    );
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Response {
        #[serde(deserialize_with = "unique_label_map")]
        labels: HashMap<String, Value>,
    }
    let obj: Response = serde_json::from_str(
        message["content"]
            .as_str()
            .context("missing classifier content")?,
    )?;
    ensure!(
        obj.labels.len() == expected.len(),
        "all-ID coverage required"
    );
    let mut result = Classifications::default();
    for (id, row) in obj.labels {
        ensure!(
            row.as_object().is_some_and(|m| m.len() == 2),
            "invalid label properties"
        );
        let mut kind = match row["kind"].as_str() {
            Some("direct") => 0,
            Some("background") => 1,
            Some("irrelevant") => 2,
            _ => anyhow::bail!("unknown label kind"),
        };
        let candidate = expected.iter().find(|c| c.id == id).context("unknown ID")?;
        ensure!(!result.kinds.contains_key(&id), "duplicate ID");
        let evidence = compact(
            row["evidence"]
                .as_str()
                .context("evidence string required")?,
        );
        ensure!(evidence.chars().count() <= 160, "evidence length");
        if kind == 0 {
            if !missing_literals(query, candidate).is_empty()
                || evidence.chars().count() < 16
                || !(compact(&candidate.title).contains(&evidence)
                    || compact(&candidate.snippet).contains(&evidence))
            {
                kind = 1;
                result.downgraded += 1;
            } else {
                result.evidence.insert(id.to_owned(), evidence);
            }
        }
        result.kinds.insert(id.to_owned(), kind);
    }
    Ok(result)
}

pub fn select(
    candidates: &[Candidate],
    kinds: &HashMap<String, u8>,
    rank: &HashMap<String, usize>,
) -> Result<Vec<Candidate>> {
    let mut keyed: Vec<_> = candidates
        .iter()
        .map(|c| {
            Ok((
                kinds.get(&c.id).copied().unwrap_or(3),
                *rank.get(&canonical(&c.url, true)?).unwrap_or(&100_000),
                canonical(&c.url, true)?,
                c.clone(),
            ))
        })
        .collect::<Result<_>>()?;
    keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    Ok(keyed.into_iter().map(|(_, _, _, c)| c).collect())
}

pub struct Session {
    cache: HashMap<String, Source>,
    last: HashMap<&'static str, Instant>,
    pub requests: usize,
}
impl Session {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            last: HashMap::new(),
            requests: 0,
        }
    }
    #[cfg(test)]
    pub fn frozen(sources: Vec<Source>) -> Self {
        let mut session = Self::new();
        session.cache = sources
            .into_iter()
            .map(|s| (format!("{}\n{}", s.engine, s.query), s))
            .collect();
        session
    }
    pub(crate) async fn source(
        &mut self,
        engine: &'static str,
        query: &str,
        blocked: &mut HashMap<&'static str, String>,
        output: Option<&Path>,
    ) -> Result<Source> {
        let key = format!("{engine}\n{query}");
        if let Some(source) = self.cache.get(&key) {
            return Ok(source.clone());
        }
        let mut source = Source {
            engine: engine.into(),
            query: query.into(),
            results: vec![],
            error: None,
            http_status: None,
            hops: vec![],
            html_hash: None,
            elapsed_ms: 0,
        };
        if let Some(reason) = blocked.get(engine) {
            source.error = Some(format!("source_disabled: {reason}"));
            return Ok(source);
        }
        if let Some(last) = self.last.get(engine) {
            tokio::time::sleep(Duration::from_millis(2100).saturating_sub(last.elapsed())).await;
        }
        let start = Instant::now();
        self.requests += 1;
        let url = crate::search::request_url(engine, query)?;
        match crate::net::fetch_with_ttl(url.as_str(), crate::cache::SEARCH_TTL).await {
            Ok(page) => {
                source.http_status = Some(page.status);
                source.hops = page.hops.clone();
                source.html_hash = Some(format!("{:x}", Sha256::digest(page.html.as_bytes())));
                if let Some(out) = output {
                    std::fs::write(out.join(format!("S{:03}.html", self.requests)), &page.html)?;
                }
                let parsed = if page.status == 200 {
                    crate::search::parse_policy(engine, &page.html, &page.url)
                } else {
                    Err(anyhow::anyhow!("HTTP {}", page.status))
                };
                match parsed {
                    Ok(results) => {
                        page.store().await;
                        source.results = results;
                    }
                    Err(e) => source.error = Some(e.to_string()),
                }
            }
            Err(e) => source.error = Some(e.to_string()),
        }
        self.last.insert(engine, Instant::now());
        source.elapsed_ms = start.elapsed().as_millis() as u64;
        if let Some(error) = &source.error
            && crate::search::blocks_source(crate::search::error_code(error))
        {
            blocked.insert(engine, error.clone());
        }
        self.cache.insert(key, source.clone());
        if let Some(out) = output {
            crate::save(
                &out.join(format!("S{:03}.json", self.requests)),
                &serde_json::to_value(&source)?,
            )?;
        }
        Ok(source)
    }

    pub async fn run(
        &mut self,
        case: &Case,
        model: &Model,
        blocked: &mut HashMap<&'static str, String>,
        document_blocks: &mut HashMap<String, String>,
        output: Option<&Path>,
        evidence_memory: bool,
    ) -> Result<Value> {
        ensure!(
            !case.forms.is_empty() && case.forms.len() <= 4 && case.forms[0] == case.query,
            "query form budget/original mismatch"
        );
        let site = crate::search::site_filter(&case.query)?;
        for form in &case.forms {
            crate::search::site_filter(form)?;
        }
        let started = Instant::now();
        let mut sources = Vec::new();
        let mut errors = Vec::new();
        for form in &case.forms {
            for engine in ["seznam", "naver"] {
                if started.elapsed() >= Duration::from_secs(180) {
                    errors.push("deadline_exceeded".to_owned());
                    break;
                }
                let remaining = Duration::from_secs(180).saturating_sub(started.elapsed());
                match tokio::time::timeout(remaining, self.source(engine, form, blocked, output))
                    .await
                {
                    Ok(result) => {
                        let source = result?;
                        if let Some(e) = &source.error {
                            errors.push(format!("{engine}: {e}"));
                        }
                        sources.push(source);
                    }
                    Err(_) => errors.push("deadline_exceeded".into()),
                }
            }
        }
        let (mut candidates, mut rank) = export(case, &sources)?;
        let news = if evidence_memory && crate::news::is_query(&case.query) {
            let prepared = crate::news_search::prepare(
                case,
                &candidates,
                &rank,
                document_blocks,
                started,
                output,
            )
            .await;
            candidates = prepared.candidates.clone();
            let articles = prepared.evidence_candidates();
            for value in rank.values_mut() {
                *value += articles.len();
            }
            for (index, article) in articles.iter().enumerate() {
                rank.insert(canonical(&article.url, true)?, index);
            }
            Some(prepared)
        } else {
            None
        };
        let mut calls = Vec::new();
        let mut classification = Classifications::default();
        match batches(case, &candidates) {
            Err(e) => errors.push(e.to_string()),
            Ok(batches) => {
                for context in batches {
                    let remaining = Duration::from_secs(180).saturating_sub(started.elapsed());
                    if remaining.is_zero() {
                        errors.push("deadline_exceeded".into());
                        break;
                    }
                    let parsed: Value = serde_json::from_str(&context)?;
                    let ids: Vec<String> = parsed["candidates"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|c| c["id"].as_str().unwrap().to_owned())
                        .collect();
                    match model.classify(&context, &ids, remaining).await {
                        Ok(call) => {
                            let expected: Vec<Candidate> =
                                serde_json::from_value(parsed["candidates"].clone())?;
                            let checked = labels(&call["response"], &expected, &case.query);
                            if !call["warning"].is_null() {
                                errors.push(format!("model_warning: {}", call["warning"]));
                            }
                            if let Some(out) = output {
                                crate::save(
                                    &out.join(format!("{}-call{:02}.json", case.id, calls.len())),
                                    &call,
                                )?;
                            }
                            calls.push(call);
                            match checked {
                                Ok(found) => {
                                    classification.kinds.extend(found.kinds);
                                    classification.evidence.extend(found.evidence);
                                    classification.downgraded += found.downgraded;
                                }
                                Err(e) => {
                                    errors.push(format!("classifier_invalid: {e}"));
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            calls.push(json!({"error":e.to_string(),"context_chars":context.chars().count()}));
                            errors.push("model_unavailable".into());
                            break;
                        }
                    }
                }
            }
        }
        let valid = classification.kinds.len() == candidates.len();
        if !valid {
            classification = Classifications::default();
        }
        let mut rescue = crate::rescue::Report::default();
        let rescue_skipped = if !valid {
            Some("invalid_initial_classification")
        } else if !evidence_memory && calls.len() + 2 > 3 {
            Some("classification_budget")
        } else {
            None
        };
        if rescue_skipped.is_none() {
            let input = crate::rescue::Input {
                case,
                candidates: &candidates,
                rank: &rank,
                started,
                output,
            };
            rescue = if let Some(news) = &news {
                let articles = news.evidence_candidates();
                crate::evidence::run_with_reader(
                    crate::rescue::Input {
                        candidates: &articles,
                        ..input
                    },
                    &mut classification,
                    model,
                    document_blocks,
                    3usize.saturating_sub(calls.len()),
                    |c| {
                        let result = news.read_cached(&c, &case.query);
                        async move { result }
                    },
                )
                .await
            } else if evidence_memory {
                crate::evidence::run(
                    input,
                    &mut classification,
                    model,
                    document_blocks,
                    3usize.saturating_sub(calls.len()),
                )
                .await
            } else {
                crate::rescue::run(input, &mut classification, model, document_blocks).await
            };
            calls.append(&mut rescue.calls);
        }
        if let Some(news) = &news {
            news.enforce(&mut classification, &mut rescue);
        }
        errors.extend(rescue.errors.iter().map(|e| {
            format!(
                "{}: {e}",
                if evidence_memory {
                    "evidence_verification"
                } else {
                    "page_verification"
                }
            )
        }));
        if classification.downgraded > 0 {
            errors.push(format!(
                "unsupported_direct_evidence_or_constraints: {} downgraded",
                classification.downgraded
            ));
        }
        let kinds = &classification.kinds;
        let ranked = select(&candidates, kinds, &rank)?;
        let before_site: Vec<_> = ranked.iter().take(10).map(|c| c.id.clone()).collect();
        let mut selected = Vec::new();
        let mut discarded = 0;
        for c in ranked {
            let r = SearchResult {
                title: c.title.clone(),
                url: c.url.clone(),
                snippet: c.snippet.clone(),
                position: 1,
                result_source: None,
            };
            if crate::search::filter_site(site.as_ref(), vec![r]).is_err() {
                discarded += 1;
                continue;
            }
            if selected.len() < 10 {
                let document = rescue.documents.iter().find(|d| d["id"] == c.id);
                selected.push(json!({"id":c.id,"url":c.url,"title":c.title,"snippet":c.snippet,"classification":kinds.get(&c.id).map(|k|["direct","background","irrelevant"][*k as usize]),"evidence":classification.evidence.get(&c.id),"requirement_evidence":rescue.proofs.get(&c.id),"document_http_status":document.and_then(|d|d["http_status"].as_u64()),"fetch_method":if document.is_some_and(|d|d["text_hash"].is_string()) {"http_extract"} else {"unknown"},"rrf_rank":rank.get(&canonical(&c.url,true)?),"news":document.and_then(|d|d.get("news"))}));
            }
        }
        let status = if candidates.is_empty() {
            if errors.is_empty() { "empty" } else { "error" }
        } else if !errors.is_empty() || !valid || discarded > 0 {
            "partial"
        } else {
            "ok"
        };
        let mut value = json!({"model_provider":model.provider(),"model":model.name,"policy":"B17-frozen-2026-10-29","classifier":crate::model::CLASSIFIER_VERSION,"grounding_downgrades":classification.downgraded,"page_verification":{"requirements":rescue.requirements,"documents":rescue.documents,"promoted":rescue.promoted,"verified":rescue.proofs.len(),"evidence_memory":rescue.memory,"passage_reranker":{"model":model.reranker.as_ref().map(|m| &m.name),"calls":rescue.reranker_calls},"evidence_mode":if evidence_memory{"coverage"}else{"legacy"},"rejected":rescue.rejected,"skipped":rescue_skipped},"id":case.id,"query":case.query,"forms":case.forms,"status":status,"results":selected,"candidates":candidates,"sources":sources,"model_calls":calls,"selected_before_site":before_site,"discarded_by_site":discarded,"classifier_valid":valid,"direct_candidates_predicted":kinds.values().filter(|&&k|k==0).count(),"errors":errors,"elapsed_ms":started.elapsed().as_millis(),"budget":{"logical_search_calls":case.forms.len()*2,"classification_calls":calls.len(),"source_context_max":calls.iter().filter_map(|c|c["context_chars"].as_u64()).max().unwrap_or(0),"document_reads":rescue.reads,"reranker_calls":rescue.reranker_calls.len(),"reranker_pairs":rescue.reranker_calls.iter().filter_map(|c|c["trace"]["pool_ranges"].as_array().map(Vec::len)).sum::<usize>()},"untrusted_content":true,"warnings":[if evidence_memory {"Experimental coverage layer; unverified direct becomes background; no factual truth verification. Explicitly enabled at startup; qualification remains open."}else{"Experimental legacy relevance; only background page promotions receive body audit; initial SERP-direct is not universally verified."},"Unknown queries use only the original form. Not a full research answer."]});
        value["news_navigation"] = news.map_or(Value::Null, |n| n.trace);
        if let Some(out) = output {
            crate::save(&out.join(format!("{}-result.json", case.id)), &value)?;
        }
        Ok(value)
    }
}

pub async fn suite(model: &Model, output: &Path, evidence_memory: bool) -> Result<()> {
    std::fs::create_dir(output)?;
    model.check().await?;
    let mut session = Session::new();
    let mut blocked = HashMap::new();
    let mut document_blocks = HashMap::new();
    let mut results = Vec::new();
    crate::save(
        &output.join("manifest.json"),
        &json!({"policy":include_str!("../feasibility/b17-policy.json"),"fixture_sha256":format!("{:x}",Sha256::digest(include_bytes!("../feasibility/search-cases.json"))),"binary_sha256":format!("{:x}",Sha256::digest(std::fs::read(std::env::current_exe()?)?)),"model":model.name,"mode":"fresh native same-day run; not day2; no old captures/gold/predictions"}),
    )?;
    for case in cases() {
        let result = session
            .run(
                &case,
                model,
                &mut blocked,
                &mut document_blocks,
                Some(output),
                evidence_memory,
            )
            .await?;
        println!(
            "{} {} results={} predicted_direct={}",
            case.id,
            result["status"],
            result["results"].as_array().unwrap().len(),
            result["direct_candidates_predicted"]
        );
        results.push(result);
        crate::save(
            &output.join("results.json"),
            &json!({"cases":results,"actual_source_requests":session.requests,"blocked_sources":blocked}),
        )?;
    }
    Ok(())
}

pub fn public_result(mut result: Value, max_results: usize) -> Value {
    if let Some(calls) = result["model_calls"].as_array() {
        let costs: Vec<_> = calls
            .iter()
            .filter_map(|c| c["response"]["usage"]["cost"].as_f64())
            .collect();
        result["usage"] = json!({"generative_calls":calls.len(),"reported_cost_usd":costs.iter().sum::<f64>(),"cost_complete":costs.len()==calls.len(),"unknown_cost_calls":calls.len()-costs.len()});
    }
    for key in [
        "candidates",
        "sources",
        "model_calls",
        "selected_before_site",
    ] {
        result.as_object_mut().unwrap().remove(key);
    }
    result["results"]
        .as_array_mut()
        .unwrap()
        .truncate(max_results);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_costs_keep_missing_reports_explicit() {
        let value = public_result(
            json!({"results":[],"model_calls":[{"response":{"usage":{"cost":0.25}}},{"response":{"usage":{}}}]}),
            5,
        );
        assert_eq!(value["usage"]["generative_calls"], 2);
        assert_eq!(value["usage"]["reported_cost_usd"], 0.25);
        assert_eq!(value["usage"]["cost_complete"], false);
        assert_eq!(value["usage"]["unknown_cost_calls"], 1);
        assert!(value.get("model_calls").is_none());
    }
    #[tokio::test]
    #[ignore = "explicit live suite; requires WEB_SEARCH_CASES, WEB_SEARCH_OUTPUT, WEB_SEARCH_OMLX_SETTINGS"]
    async fn capture_relevance_suite() -> Result<()> {
        let fixture_path = std::env::var("WEB_SEARCH_CASES").context("fixture required")?;
        let out = std::path::PathBuf::from(
            std::env::var_os("WEB_SEARCH_OUTPUT").context("new output directory required")?,
        );
        std::fs::create_dir(&out)?;
        let fixture = std::fs::read(fixture_path)?;
        let plan: Value = serde_json::from_slice(&fixture)?;
        let cases = plan["cases"].as_array().context("cases array required")?;
        ensure!(!cases.is_empty() && cases.len() <= 20, "suite case limit");
        crate::save(
            &out.join("manifest.json"),
            &json!({"classifier":crate::model::CLASSIFIER_VERSION,"fixture_sha256":format!("{:x}",Sha256::digest(&fixture)),"binary_sha256":format!("{:x}",Sha256::digest(std::fs::read(std::env::current_exe()?)?))}),
        )?;
        let model = if let Some(key) = std::env::var_os("WEB_SEARCH_OPENROUTER_KEY_FILE") {
            Model::from_openrouter(
                Path::new(&key),
                "deepseek/deepseek-v4.1-flash-1017",
                std::env::var("WEB_SEARCH_OPENROUTER_PROXY").ok().as_deref(),
            )?
        } else {
            Model::from_settings(
                &std::path::PathBuf::from(
                    std::env::var_os("WEB_SEARCH_OMLX_SETTINGS").context("settings required")?,
                ),
                "MiMo-V2.6-Distill-Qwen-9B-OptiQ-4bit",
            )?
        };
        model.check().await?;
        let mut session = Session::new();
        let mut blocked = HashMap::new();
        let mut document_blocks: HashMap<String, String> = serde_json::from_str(
            &std::env::var("WEB_SEARCH_DOCUMENT_BLOCKS").unwrap_or_else(|_| "{}".into()),
        )?;
        let mut results = Vec::new();
        for row in cases {
            let case: Case = serde_json::from_value(row.clone())?;
            results.push(
                session
                    .run(
                        &case,
                        &model,
                        &mut blocked,
                        &mut document_blocks,
                        Some(&out),
                        false,
                    )
                    .await?,
            );
            crate::save(
                &out.join("results.json"),
                &json!({"cases":results,"unique_source_attempts":session.requests,"blocked_sources":blocked,"blocked_document_hosts":document_blocks}),
            )?;
            let cost: f64 = results
                .iter()
                .flat_map(|r| r["model_calls"].as_array().into_iter().flatten())
                .filter_map(|c| c["response"]["usage"]["cost"].as_f64())
                .sum();
            println!("capture {} completed; cost_usd={cost:.6}", case.id);
            ensure!(cost < 2.0, "capture cost budget");
            ensure!(
                !results.last().unwrap()["errors"]
                    .to_string()
                    .contains("HTTP 403")
                    && !results.last().unwrap()["errors"]
                        .to_string()
                        .contains("HTTP 429"),
                "model provider refusal; stop capture"
            );
        }
        Ok(())
    }

    #[test]
    fn label_validation_requires_every_id_clean_finish_and_exact_evidence() {
        let candidate = Candidate {
            id: "C001".into(),
            url: "https://example.org/".into(),
            title: "SQLite WAL transactions and checkpoints".into(),
            snippet: "Readers and writers proceed concurrently in WAL mode.".into(),
            document_http_status: None,
            fetch_method: "unknown".into(),
        };
        let expected = vec![candidate];
        let make = |content: Value| json!({"choices":[{"finish_reason":"stop","message":{"content":content.to_string()}}]});
        let good = make(
            json!({"labels":{"C001":{"kind":"direct","evidence":"SQLite WAL transactions and checkpoints"}}}),
        );
        let checked = labels(&good, &expected, "SQLite WAL transactions").unwrap();
        assert_eq!(checked.kinds["C001"], 0);
        assert_eq!(checked.evidence["C001"], expected[0].title);
        for quote in ["", "Invented quotation about databases", "WAL"] {
            let raw = make(json!({"labels":{"C001":{"kind":"direct","evidence":quote}}}));
            let checked = labels(&raw, &expected, "SQLite WAL transactions").unwrap();
            assert_eq!(checked.kinds["C001"], 1);
            assert_eq!(checked.downgraded, 1);
            assert!(checked.evidence.is_empty());
        }
        for content in [
            json!({"labels":{"unknown":{"kind":"direct","evidence":""}}}),
            json!({"labels":{"C001":{"kind":"direct"}}}),
            json!({"labels":{}}),
            json!({"labels":{},"answer":"invented"}),
        ] {
            assert!(labels(&make(content), &expected, "SQLite WAL transactions").is_err());
        }
        let duplicated = json!({"choices":[{"finish_reason":"stop","message":{"content":r#"{"labels":{"C001":{"kind":"background","evidence":""},"C001":{"kind":"background","evidence":""}}}"#}}]});
        assert!(labels(&duplicated, &expected, "SQLite WAL transactions").is_err());
        let mut bad = good;
        bad["choices"][0]["finish_reason"] = json!("length");
        assert!(labels(&bad, &expected, "SQLite WAL transactions").is_err());
    }

    #[test]
    fn literal_gate_requires_named_apis_versions_and_periods() {
        let mut c = Candidate {
            id: "C001".into(),
            url: "https://example.org/".into(),
            title: "TaskGroup cancellation".into(),
            snippet: "Python 3.13 released in October 2024 with GIL experiments".into(),
            document_http_status: None,
            fetch_method: "unknown".into(),
        };
        assert_eq!(
            missing_literals("TaskGroup ExceptionGroup", &c),
            vec!["ExceptionGroup"]
        );
        assert!(missing_literals("Python 3.13 GIL release October 2024", &c).is_empty());
        assert!(
            missing_literals("Python 3.14 GIL release April 2024", &c).contains(&"3.14".to_owned())
        );
        c.title = "ArrayBufferLike".into();
        assert_eq!(missing_literals("ArrayBuffer", &c), vec!["ArrayBuffer"]);
        c.snippet = "Retry-After controls HTTP 429".into();
        assert!(missing_literals("HTTP Retry-After 429 503 документация", &c).is_empty());
        assert!(!missing_literals("HTTP 429 503 comparison", &c).is_empty());
        assert!(missing_literals("site:example.org/3.13 plain question", &c).is_empty());
        c.snippet = "2024년 11월 publication".into();
        assert_eq!(missing_literals("January 2024", &c), vec!["january"]);
    }

    #[test]
    fn frozen_plan_context_and_ranking_keep_bounds() {
        let plan = cases();
        assert_eq!(plan.len(), 20);
        let original: Vec<Value> =
            serde_json::from_str(include_str!("../feasibility/search-cases.json")).unwrap();
        for (case, expected) in plan.iter().zip(original) {
            assert_eq!(case.query, expected["query"]);
            assert_eq!(case.forms[0], case.query);
            assert!(case.forms.len() <= 4);
        }
        assert!(
            plan.iter()
                .find(|c| c.id == "ru05")
                .unwrap()
                .forms
                .iter()
                .any(|f| f == "excavator cycle time stages digging swinging dumping return")
        );
        assert_eq!(
            case_for("A completely new question").unwrap().forms,
            vec!["A completely new question"]
        );
        let c = Candidate {
            url: "https://example.org/a".into(),
            title: "Quoted \"a,b\": тест".into(),
            snippet: "evidence".into(),
            document_http_status: None,
            fetch_method: "unknown".into(),
            id: "C001".into(),
        };
        let context = batches(&plan[0], std::slice::from_ref(&c)).unwrap();
        let decoded: Value = serde_json::from_str(&context[0]).unwrap();
        assert_eq!(decoded["candidates"][0]["title"], c.title);
        assert!(context[0].starts_with("{\"query\": "));
        let too_many = vec![c.clone(); 73];
        assert!(batches(&plan[0], &too_many).is_err());
        assert_eq!(
            canonical(
                "https://www.example.org/x/?b=2&utm_source=z&a=1#part",
                false
            )
            .unwrap(),
            "example.org/x?a=1&b=2"
        );
        let mut second = c.clone();
        second.id = "C002".into();
        second.url = "https://example.org/b".into();
        let rank = HashMap::from([("example.org/a".into(), 0), ("example.org/b".into(), 1)]);
        let kinds = HashMap::from([("C001".into(), 2), ("C002".into(), 0)]);
        assert_eq!(select(&[c, second], &kinds, &rank).unwrap()[0].id, "C002");
    }

    #[test]
    #[ignore = "explicit offline B17 export parity; requires WEB_SEARCH_RESEARCH_B17_DIR"]
    fn b17_export_parity() -> Result<()> {
        let root = std::path::PathBuf::from(std::env::var("WEB_SEARCH_RESEARCH_B17_DIR")?);
        let rows: Vec<Value> =
            serde_json::from_slice(&std::fs::read(root.join("fresh/enriched.json"))?)?;
        let trace: Vec<Value> =
            serde_json::from_slice(&std::fs::read(root.join("fresh-trace.json"))?)?;
        let expected: Vec<Value> =
            serde_json::from_slice(&std::fs::read(root.join("blinded-candidates.json"))?)?;
        let reference_rrf: Value =
            serde_json::from_slice(&std::fs::read(root.join("rrf-predictions.json"))?)?;
        let mut compared = Vec::new();
        for (case_index, (case, (t, wanted))) in
            cases().iter().zip(trace.iter().zip(&expected)).enumerate()
        {
            let sources = t["request_refs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| {
                    let row = rows.iter().find(|r| r["request_id"] == *id).unwrap();
                    let html = std::fs::read_to_string(
                        root.join("fresh")
                            .join(format!("{}.html", id.as_str().unwrap())),
                    )?;
                    let results = match crate::search::parse_policy(
                        row["engine"].as_str().unwrap(),
                        &html,
                        row["url"].as_str().unwrap(),
                    ) {
                        Ok(results) => results,
                        Err(e)
                            if row["results"].as_array().unwrap().is_empty()
                                && e.to_string() == "parser_changed" =>
                        {
                            vec![]
                        }
                        Err(e) => return Err(e),
                    };
                    Ok(Source {
                        engine: row["engine"].as_str().unwrap().into(),
                        query: row["internal_query"].as_str().unwrap().into(),
                        results,
                        error: None,
                        http_status: Some(200),
                        hops: vec![],
                        html_hash: None,
                        elapsed_ms: 0,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let (cs, rank) = export(case, &sources)?;
            let actual = serde_json::to_value(&cs)?;
            let equal = actual == wanted["candidates"];
            let reference_rank: HashMap<String, usize> =
                reference_rrf["cases"][case_index]["ranked"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .enumerate()
                    .map(|(i, r)| Ok((canonical(r["url"].as_str().unwrap(), true)?, i)))
                    .collect::<Result<_>>()?;
            let rank_equal = rank == reference_rank;
            let contexts = batches(case, &cs)?;
            let mut contexts_equal = true;
            for (batch_index, context) in contexts.iter().enumerate() {
                let reference: Value = serde_json::from_slice(&std::fs::read(
                    root.join("model-run")
                        .join(format!("{case_index:02}-{batch_index}-input.json")),
                )?)?;
                contexts_equal &= reference["messages"][0]["content"] == crate::model::B17_PROMPT
                    && reference["messages"][1]["content"] == *context;
            }
            compared.push(json!({"id":case.id,"equal":equal,"rrf_equal":rank_equal,"contexts_equal":contexts_equal,"actual":if equal{Value::Null}else{actual},"expected":if equal{Value::Null}else{wanted["candidates"].clone()}}));
        }
        let passed = compared
            .iter()
            .all(|r| r["equal"] == true && r["rrf_equal"] == true && r["contexts_equal"] == true);
        crate::save(
            Path::new("runs/recall-2026-10-29/export-parity.json"),
            &json!({"passed":passed,"comparisons":compared,"network_calls":0,"model_calls":0}),
        )?;
        ensure!(passed, "export differences saved in export-parity.json");
        Ok(())
    }
}
