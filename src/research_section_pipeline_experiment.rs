//! Explicit saved-source pipeline experiment; no public HTTP or production wiring.
use super::*;

pub(super) struct Saved<'a> {
    model: &'a crate::model::Model,
    source: &'a Value,
    qwen: bool,
    pace: Duration,
    last_generation_start: Option<Instant>,
    halted: Option<String>,
    capture: Option<&'a Path>,
    attempted_generations: usize,
}
impl<'a> Saved<'a> {
    pub(super) fn generator(model: &'a crate::model::Model, capture: &'a Path) -> Self {
        Self {
            model,
            source: &Value::Null,
            qwen: false,
            pace: Duration::from_secs(10),
            last_generation_start: None,
            halted: None,
            capture: Some(capture),
            attempted_generations: 0,
        }
    }
}
impl Backend for Saved<'_> {
    fn available(&self, engine: &'static str) -> bool {
        engine == "seznam"
    }
    fn has_qwen(&self) -> bool {
        self.qwen
    }
    async fn search(&mut self, engine: &'static str, query: &str) -> Result<crate::policy::Source> {
        Ok(crate::policy::Source {
            engine: engine.into(),
            query: query.into(),
            results: vec![crate::search::SearchResult {
                title: self.source["title"].as_str().unwrap().into(),
                url: self.source["url"].as_str().unwrap().into(),
                snippet: String::new(),
                position: 1,
                result_source: None,
            }],
            error: None,
            http_status: None,
            hops: vec![],
            html_hash: None,
            elapsed_ms: 0,
        })
    }
    async fn read(&mut self, target: &Target, _: &str) -> Result<Document> {
        ensure!(target.url == self.source["url"], "unexpected saved URL");
        let doc = &self.source["extracted"];
        Ok(Document {
            url: target.url.clone(),
            text: doc["text"].as_str().context("saved text")?.into(),
            meta: self
                .source
                .get("metadata")
                .cloned()
                .unwrap_or_else(|| json!({"title":self.source["title"]})),
            links: vec![],
            cache: Value::Null,
            extraction: json!({"method":doc["extraction_method"], "scope":doc["content_scope"],"truncated":doc["truncated"]}),
        })
    }
    async fn rank(
        &mut self,
        question: &str,
        text: &str,
        started: Instant,
    ) -> Result<(Vec<(usize, usize)>, Value)> {
        self.qwen = false;
        let result = crate::reranker::select(
            self.model.reranker.as_deref().context("Qwen required")?,
            question,
            text,
            started,
        )
        .await?;
        self.qwen = true;
        Ok(result)
    }
    async fn generate(
        &mut self,
        prompt: &str,
        context: &str,
        schema: Value,
        remaining: Duration,
    ) -> Result<Value> {
        if let Some(reason) = &self.halted {
            anyhow::bail!("{reason}");
        }
        // Keep the backend halted if an outer Session timeout drops this future.
        // Only a completed, accepted response permits another generation.
        self.halted =
            Some("saved generation interrupted or pacing failed; experiment halted".into());
        let entered = Instant::now();
        let delay = pacing_delay(
            self.last_generation_start.map(|start| start.elapsed()),
            self.pace,
            remaining,
        )?;
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let remaining = remaining.saturating_sub(entered.elapsed());
        ensure!(!remaining.is_zero(), "deadline: saved pipeline pacing");
        self.last_generation_start = Some(Instant::now());
        self.attempted_generations += 1;
        if let Some(directory) = self.capture {
            crate::save(
                &directory.join(format!(
                    "request-attempted-{:02}.json",
                    self.attempted_generations
                )),
                &json!({"sequence":self.attempted_generations,"phase":"before_research_call","dispatch_confirmed":false}),
            )?;
        }
        let outcome = self
            .model
            .research_call(prompt, context, schema, 3072, remaining)
            .await;
        let reason = match &outcome {
            Err(error) => Some(error.to_string()), // Model transport redacts credentials/details.
            Ok(call) => rejected_generation(call),
        };
        if let Some(reason) = reason {
            self.halted = Some(reason.clone()); // Set before capture: a write failure cannot resume POSTs.
            if let Some(directory) = self.capture {
                let rejected = match &outcome {
                    Ok(call) => json!({"reason":reason,"call":call}),
                    Err(_) => {
                        json!({"reason":reason,"status":"transport_error","usage_known":false})
                    }
                };
                crate::save(
                    &directory.join(format!(
                        "request-rejected-{:02}.json",
                        self.attempted_generations
                    )),
                    &rejected,
                )?;
            }
            anyhow::bail!("{reason}");
        }
        self.halted = None;
        outcome
    }
}

// Every non-stop envelope halts this experiment, including length (no schema-repair retry).
fn rejected_generation(call: &Value) -> Option<String> {
    let choice = &call["response"]["choices"][0];
    let message = &choice["message"];
    if !message["refusal"].is_null() && message["refusal"] != "" {
        return Some("model explicit refusal; experiment halted".into());
    }
    if !message["tool_calls"].is_null() && message["tool_calls"] != json!([]) {
        return Some("model unexpected tool calls; experiment halted".into());
    }
    if choice["finish_reason"] != "stop" {
        return Some("model non-stop finish reason; experiment halted".into());
    }
    None
}

#[test]
fn saved_generation_envelope_halts_refusal_filter_length_tools_but_not_json_repair() {
    let good =
        json!({"response":{"choices":[{"finish_reason":"stop","message":{"content":"not JSON"}}]}});
    assert!(rejected_generation(&good).is_none());
    for finish in ["content_filter", "length", "tool_calls"] {
        let mut bad = good.clone();
        bad["response"]["choices"][0]["finish_reason"] = json!(finish);
        assert!(rejected_generation(&bad).is_some());
    }
    for field in ["refusal", "tool_calls"] {
        let mut bad = good.clone();
        bad["response"]["choices"][0]["message"][field] = json!("rejected");
        assert!(rejected_generation(&bad).is_some());
    }
}

#[tokio::test]
async fn saved_generation_cancellation_keeps_halt_before_transport() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let settings = directory.path().join("settings.json");
    crate::save(
        &settings,
        &json!({"server":{"port":1},"auth":{"api_key":"fixture"}}),
    )?;
    let model = crate::model::Model::from_settings(&settings, "fixture")?;
    let source = json!({});
    let mut saved = Saved {
        model: &model,
        source: &source,
        qwen: false,
        pace: Duration::from_secs(10),
        last_generation_start: Some(Instant::now()),
        halted: None,
        capture: None,
        attempted_generations: 0,
    };
    // Cancel in pacing, before any network operation; the same latch spans transport awaits.
    assert!(
        tokio::time::timeout(
            Duration::from_millis(1),
            saved.generate("fixture", "{}", json!({}), Duration::from_secs(20))
        )
        .await
        .is_err()
    );
    let reason = saved
        .generate("audit", "{}", json!({}), Duration::from_secs(20))
        .await
        .unwrap_err()
        .to_string();
    assert!(reason.contains("interrupted"));
    assert_eq!(saved.attempted_generations, 0);
    Ok(())
}

// Delay is charged to the existing Session budget, including the first-call zero-budget case.
fn pacing_delay(
    elapsed: Option<Duration>,
    pace: Duration,
    remaining: Duration,
) -> Result<Duration> {
    let delay = elapsed.map_or(Duration::ZERO, |elapsed| pace.saturating_sub(elapsed));
    ensure!(remaining > delay, "deadline: saved pipeline pacing");
    Ok(delay)
}

#[test]
fn saved_pipeline_pacing_rejects_exhausted_budget_before_transport() -> Result<()> {
    let pace = Duration::from_secs(10);
    for elapsed in [None, Some(Duration::ZERO), Some(pace)] {
        assert!(pacing_delay(elapsed, pace, Duration::ZERO).is_err());
    }
    assert!(pacing_delay(Some(Duration::ZERO), pace, pace).is_err());
    assert_eq!(
        pacing_delay(None, pace, Duration::from_secs(1))?,
        Duration::ZERO
    );
    assert_eq!(
        pacing_delay(Some(Duration::from_secs(4)), pace, Duration::from_secs(20))?,
        Duration::from_secs(6)
    );
    assert_eq!(
        pacing_delay(Some(Duration::from_secs(12)), pace, Duration::from_secs(1))?,
        Duration::ZERO
    );
    assert_eq!(
        pacing_delay(Some(Duration::ZERO), Duration::ZERO, Duration::from_secs(1))?,
        Duration::ZERO
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit saved-source Qwen/model pipeline; no public HTTP; frozen input/output directory required"]
async fn section_saved_source_pipeline() -> Result<()> {
    let path = std::path::PathBuf::from(std::env::var("SECTION_PIPELINE_INPUT")?);
    let directory = std::path::PathBuf::from(std::env::var("SECTION_PIPELINE_OUTPUT")?);
    // Never overwrite an attempted model trial.
    std::fs::create_dir(&directory)?;
    let fixture: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let pace_seconds = match std::env::var("SECTION_PIPELINE_PACE_SECONDS") {
        Ok(value) => {
            let seconds: u64 = value.parse().context("pipeline pace must be seconds")?;
            ensure!(
                (10..=60).contains(&seconds),
                "pipeline pace must be 10..60 seconds"
            );
            seconds
        }
        Err(std::env::VarError::NotPresent) => 0,
        Err(error) => return Err(error.into()),
    };
    crate::save(
        &directory.join("pipeline-settings.json"),
        &json!({"generative_pace_seconds":pace_seconds,"pacing_charged_to_session_budget":true}),
    )?;
    let home = std::path::PathBuf::from(std::env::var("HOME")?);
    let model = crate::model::Model::from_openrouter(
        &home.join(".config/web-search/openrouter.key"),
        "deepseek/deepseek-v4.1-flash-1017",
        Some("http://127.0.0.1:10808"),
    )?
    .with_reranker(Some(&home.join(".omlx/settings.json")), true)?;
    let input: Input =
        serde_json::from_value(json!({"question":fixture["question"],"language":"ru"}))?;
    let text = fixture["extracted"]["text"].as_str().context("text")?;
    ensure!(
        fixture["extracted"]["text_hash"] == crate::cache::hash(text.as_bytes()),
        "saved hash"
    );
    ensure!(text.chars().count() <= 100_000, "saved cap");
    crate::save(&directory.join("fixture.json"), &fixture)?;
    let backend = Saved {
        model: &model,
        source: &fixture,
        qwen: true,
        pace: Duration::from_secs(pace_seconds),
        last_generation_start: None,
        halted: None,
        capture: Some(&directory),
        attempted_generations: 0,
    };
    let result = Session::new(&input, backend, Some(&directory))
        .execute(std::future::pending())
        .await?;
    crate::save(&directory.join("result.json"), &result)?;
    let message = result["error"].as_str().unwrap_or("").to_lowercase();
    ensure!(
        !["http 401", "http 403", "http 429"]
            .iter()
            .any(|s| message.contains(s)),
        "model refusal; stop without retry"
    );
    Ok(())
}

// Oracle plan/source component: bypass model planner and retrieval, never fabricate SERP success.
async fn saved_draft_audit<B: Backend>(session: &mut Session<'_, B>) -> Result<&'static str> {
    ensure!(
        session.read_round().await? == 1,
        "saved document unavailable"
    );
    ensure!(
        !session
            .actions
            .iter()
            .any(|action| action["action"] == "qwen" && action["status"] != "ok"),
        "local Qwen unavailable; stop component before generation"
    );
    let (context, visible) = session.context(&[])?;
    let draft = if !rfc_numbers(&session.input.question).is_empty() {
        let selected: Draft<QuoteRange> = session
            .generate(
                "draft",
                &format!("{DRAFT_PROMPT}{RFC_RANGE_PROMPT}"),
                &context,
            )
            .await?;
        session.select_quotes(selected)
    } else {
        session.generate("draft", DRAFT_PROMPT, &context).await?
    };
    let draft = session.repair_draft_text(draft, &context, &visible).await?;
    if draft.claims.is_empty() {
        return Ok("no_supported_draft");
    }
    let (context, visible, conflict_visible) = session.audit_context(&draft.claims)?;
    let audit: Audit = session.generate("audit", AUDIT_PROMPT, &context).await?;
    let (sufficient, _) = session.apply_audit(draft, audit, &visible, &conflict_visible)?;
    Ok(if sufficient {
        "evidence_sufficient"
    } else {
        "oracle_component_complete"
    })
}

#[tokio::test]
async fn primary_saved_draft_audit_uses_new_draft_without_search_or_plan() -> Result<()> {
    let input = input();
    let mut backend = Fake::new();
    backend.replies.pop_front(); // Drop the Fake planner reply; draft/audit are the only stages.
    let mut session = Session::new(&input, backend, None);
    session.plan.facets = vec!["Condition".into(), "Exception".into()];
    session.missing = vec![0, 1];
    session.queue.push_back(Target {
        url: "https://source1.example/doc".into(),
        parent: None,
    });
    saved_draft_audit(&mut session).await?;
    assert_eq!(session.budget.counts.generations, 2);
    assert_eq!(session.budget.counts.forms, 0);
    assert_eq!(session.budget.counts.serp, 0);
    assert_eq!(session.successful_serps, 0);
    assert!(session.audited);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit oracle-plan/source NEWDRAFT to unchanged audit; no source HTTP; root serializes"]
async fn primary_saved_draft_audit() -> Result<()> {
    let fixture: Value =
        serde_json::from_slice(&std::fs::read(std::env::var("SECTION_PIPELINE_INPUT")?)?)?;
    let directory = std::path::PathBuf::from(std::env::var("SECTION_PIPELINE_OUTPUT")?);
    std::fs::create_dir(&directory)?;
    let input: Input = serde_json::from_value(
        json!({"question":fixture["question"],"language":fixture["language"]}),
    )?;
    input.validate()?;
    let facets: Vec<String> = serde_json::from_value(fixture["facets"].clone())?;
    ensure!(
        (1..=4).contains(&facets.len()) && facets.iter().all(|s| clean(s, 160)),
        "invalid saved model plan"
    );
    let url = fixture["url"].as_str().context("saved URL")?;
    crate::net::validate_url(url)?;
    let site = crate::search::site_filter(&input.question)?;
    let result = crate::search::SearchResult {
        title: String::new(),
        url: url.into(),
        snippet: String::new(),
        position: 1,
        result_source: None,
    };
    ensure!(
        !crate::search::filter_site(site.as_ref(), vec![result])?
            .results
            .is_empty(),
        "saved URL violates original question site"
    );
    let text = fixture["extracted"]["text"]
        .as_str()
        .context("saved text")?;
    ensure!(
        text.chars().count() <= 100_000 && !text.trim().is_empty(),
        "saved body cap"
    );
    ensure!(
        fixture["extracted"]["text_hash"] == crate::cache::hash(text.as_bytes()),
        "saved hash"
    );
    let pace_seconds: u64 = std::env::var("SECTION_PIPELINE_PACE_SECONDS")?.parse()?;
    ensure!(
        (10..=60).contains(&pace_seconds),
        "oracle pipeline requires pace10..60"
    );
    crate::save(&directory.join("fixture.json"), &fixture)?;
    crate::save(
        &directory.join("pipeline-settings.json"),
        &json!({"component":"oracle_plan_source_newdraft_oldaudit","generative_pace_seconds":pace_seconds,"source_http":0,"planner_calls":0}),
    )?;
    let home = std::path::PathBuf::from(std::env::var("HOME")?);
    let model = crate::model::Model::from_openrouter(
        &home.join(".config/web-search/openrouter.key"),
        "deepseek/deepseek-v4.1-flash-1017",
        Some("http://127.0.0.1:10808"),
    )?
    .with_reranker(Some(&home.join(".omlx/settings.json")), true)?;
    let backend = Saved {
        model: &model,
        source: &fixture,
        qwen: true,
        pace: Duration::from_secs(pace_seconds),
        last_generation_start: None,
        halted: None,
        capture: Some(&directory),
        attempted_generations: 0,
    };
    let mut session = Session::new(&input, backend, Some(&directory));
    session.plan.facets = facets;
    session.missing = (0..session.plan.facets.len()).collect();
    session.queue.push_back(Target {
        url: url.into(),
        parent: None,
    });
    let outcome = saved_draft_audit(&mut session).await;
    let error = outcome.as_ref().err().map(ToString::to_string);
    let stop = if session.budget.remaining().is_zero() {
        "deadline"
    } else {
        outcome.as_ref().copied().unwrap_or("model_error")
    };
    crate::save(
        &directory.join("result.json"),
        &session.result(stop, error.as_deref()),
    )?;
    outcome?;
    Ok(())
}

#[tokio::test]
#[ignore = "explicit loopback HTTP429 fixture; no external model/source calls"]
async fn saved_generation_429_is_captured_and_blocks_later_post() -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let count = Arc::new(AtomicUsize::new(0));
    let requests = count.clone();
    let app = axum::Router::new().fallback(move || {
        let requests = requests.clone();
        async move {
            requests.fetch_add(1, Ordering::SeqCst);
            (
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                axum::Json(json!({"error":{"message":"fixture-secret rate limited"}})),
            )
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let directory = tempfile::tempdir()?;
    let settings = directory.path().join("settings.json");
    crate::save(
        &settings,
        &json!({"server":{"port":port},"auth":{"api_key":"fixture-secret"}}),
    )?;
    let model = crate::model::Model::from_settings(&settings, "fixture")?;
    let source = json!({});
    let mut saved = Saved {
        model: &model,
        source: &source,
        qwen: false,
        pace: Duration::ZERO,
        last_generation_start: None,
        halted: None,
        capture: Some(directory.path()),
        attempted_generations: 0,
    };
    let first = saved
        .generate("fixture", "{}", json!({}), Duration::from_secs(5))
        .await
        .unwrap_err()
        .to_string();
    let second = saved
        .generate("audit", "{}", json!({}), Duration::from_secs(5))
        .await
        .unwrap_err()
        .to_string();
    server.abort();
    assert_eq!(first, second);
    assert!(first.contains("429") && !first.contains("fixture-secret"));
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(saved.attempted_generations, 1);
    let capture = std::fs::read_to_string(directory.path().join("request-rejected-01.json"))?;
    assert!(capture.contains("429") && !capture.contains("fixture-secret"));
    Ok(())
}
