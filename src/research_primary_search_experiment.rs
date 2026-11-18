//! Full native Session, with only a test-only SERP read-order intervention.
use super::section_pipeline_experiment::Saved;
use super::*;
use std::sync::{Arc, Mutex};

// shortcut: four project documentation domains; extend only with a separately frozen suite.
pub(crate) fn official(question: &str, raw: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(raw) else {
        return false;
    };
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return false;
    }
    let words: HashSet<_> = question
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .map(str::to_owned)
        .collect();
    match url.host_str().unwrap_or("") {
        "docs.python.org" => words.contains("python") || words.contains("cpython"),
        "doc.rust-lang.org" => words.contains("rust"),
        "www.postgresql.org" => words.contains("postgresql") && url.path().starts_with("/docs/"),
        "sqlite.org" | "www.sqlite.org" => words.contains("sqlite"),
        _ => false,
    }
}

struct Guarded<'a> {
    live: Live<'a>,
    generation: Saved<'a>,
    halt: Arc<Mutex<Option<String>>>,
    source_aware: bool,
}
impl Guarded<'_> {
    fn check(&self) -> Result<()> {
        if let Some(reason) = self.halt.lock().unwrap().as_ref() {
            anyhow::bail!("{reason}");
        }
        Ok(())
    }
}
// An outer Session timeout can drop any backend future before its error handler runs.
async fn guarded<T>(
    halt: &Mutex<Option<String>>,
    future: impl Future<Output = Result<T>>,
) -> Result<T> {
    if let Some(reason) = halt.lock().unwrap().as_ref() {
        anyhow::bail!("{reason}");
    }
    *halt.lock().unwrap() = Some("native operation interrupted; experiment halted".into());
    let result = future.await;
    *halt.lock().unwrap() = result.as_ref().err().map(|e| bounded(&e.to_string(), 200));
    result
}

impl Backend for Guarded<'_> {
    fn available(&self, engine: &'static str) -> bool {
        self.check().is_ok() && self.live.available(engine)
    }
    fn has_qwen(&self) -> bool {
        self.live.has_qwen()
    }
    async fn search(&mut self, engine: &'static str, query: &str) -> Result<crate::policy::Source> {
        let result = guarded(&self.halt, self.live.search(engine, query)).await;
        if let Ok(source) = &result
            && let Some(reason) = &source.error
        {
            *self.halt.lock().unwrap() = Some(bounded(reason, 200));
        }
        result
    }
    async fn read(&mut self, target: &Target, question: &str) -> Result<Document> {
        self.check()?;
        if !self.source_aware {
            return guarded(&self.halt, self.live.read(target, question)).await;
        }
        let (host, key) = guarded(&self.halt, async {
            let mut url = crate::net::validate_url(&target.url)?;
            let host = url.host_str().context("document host")?.to_owned();
            url.set_fragment(None);
            Ok((host, url.to_string()))
        })
        .await?;
        if self.live.document_blocks.contains_key(&host) {
            anyhow::bail!("source_blocked: campaign host exclusion");
        }
        if self.live.document_url_blocks.contains_key(&key) {
            anyhow::bail!("source_url_blocked: campaign URL exclusion");
        }
        let previous = self.live.document_blocks.clone();
        self.live.observed_read_urls.clear();
        self.live.observed_read_urls.push(key);
        let result = guarded(&self.halt, self.live.read(target, question)).await;
        // Native redirect refusal can decorate an old reason; preserve the original ledger value.
        for (host, reason) in previous {
            self.live.document_blocks.insert(host, reason);
        }
        if let Err(error) = &result {
            let reason = error.to_string();
            if recover_document_error(
                &self.halt,
                self.live.document_blocks,
                self.live.document_url_blocks,
                &self.live.observed_read_urls,
                &reason,
            ) {
                crate::mcp::update_host_blocks(self.live.blocked, self.live.document_blocks);
            }
        }
        result
    }
    async fn rank(
        &mut self,
        question: &str,
        text: &str,
        started: Instant,
    ) -> Result<(Vec<(usize, usize)>, Value)> {
        guarded(&self.halt, self.live.rank(question, text, started)).await
    }
    async fn generate(
        &mut self,
        prompt: &str,
        context: &str,
        schema: Value,
        remaining: Duration,
    ) -> Result<Value> {
        guarded(
            &self.halt,
            self.generation.generate(prompt, context, schema, remaining),
        )
        .await
    }
}

pub(crate) fn document_url_key(raw: &str) -> Result<String> {
    let mut url = crate::net::validate_url(raw)?;
    url.set_fragment(None);
    Ok(url.to_string())
}

pub(crate) fn check_document_url(
    url: &reqwest::Url,
    blocks: &HashMap<String, String>,
    observed: &mut Vec<String>,
) -> Result<()> {
    let key = document_url_key(url.as_str())?;
    observed.push(key.clone());
    ensure!(
        !blocks.contains_key(&key),
        "source_url_blocked: campaign URL exclusion"
    );
    Ok(())
}

fn recover_document_error(
    halt: &Mutex<Option<String>>,
    hosts: &mut HashMap<String, String>,
    urls: &mut HashMap<String, String>,
    observed: &[String],
    reason: &str,
) -> bool {
    if !recoverable_document_error(reason) {
        return false;
    }
    let code = crate::search::error_code(reason);
    let hard = (crate::search::blocks_source(code) && code != "source_blocked")
        || matches!(reason, "HTTP 403" | "HTTP 429");
    if hard {
        if let Some(host) = observed
            .last()
            .and_then(|raw| reqwest::Url::parse(raw).ok())
            .and_then(|u| u.host_str().map(str::to_owned))
        {
            hosts.entry(host).or_insert_with(|| bounded(reason, 200));
        }
    } else {
        for url in observed {
            urls.entry(url.clone())
                .or_insert_with(|| bounded(reason, 200));
        }
    }
    *halt.lock().unwrap() = None;
    true
}

// Phase-specific: never classify model/Qwen failures using this document-only predicate.
fn recoverable_document_error(reason: &str) -> bool {
    let code = crate::search::error_code(reason);
    reason.starts_with("source_url_blocked:")
        || reason.starts_with("host_disabled:")
        || code == "fetch_timeout"
        || crate::search::blocks_source(code)
        || reason
            .strip_prefix("HTTP ")
            .and_then(|s| s.parse::<u16>().ok())
            .is_some_and(|s| (400..600).contains(&s))
}

#[test]
fn source_aware_document_errors_do_not_include_model_or_controller_failures() {
    for reason in [
        "fetch_timeout",
        "source_blocked: campaign exclusion",
        "captcha: page",
        "HTTP 404",
        "HTTP 503",
        "host_disabled: blocked.example",
    ] {
        assert!(recoverable_document_error(reason));
    }
    for reason in [
        "OpenRouter HTTP 429 Too Many Requests",
        "Qwen HTTP 429",
        "deadline: document read",
        "native operation interrupted; experiment halted",
        "model explicit refusal",
        "invalid JSON",
        "HTTP 200",
    ] {
        assert!(!recoverable_document_error(reason));
    }
}

#[test]
fn official_priority_exact_project_and_origin_only() {
    for (q, u) in [
        (
            "Python lru_cache",
            "https://docs.python.org/3/library/functools.html",
        ),
        (
            "Rust Vec::retain",
            "https://doc.rust-lang.org/std/vec/struct.Vec.html",
        ),
        (
            "PostgreSQL CTE",
            "https://www.postgresql.org/docs/12/queries-with.html",
        ),
        ("SQLite foreign_keys", "https://sqlite.org/pragma.html"),
    ] {
        assert!(official(q, u));
    }
    for u in [
        "https://docs.python.org.evil.test/3/",
        "https://evil.test/docs.python.org/",
        "https://user@docs.python.org/3/",
        "http://docs.python.org/3/",
        "https://docs.python.org:1234/3/",
        "https://doc.rust-lang.org/std/",
    ] {
        assert!(!official("Python lru_cache", u));
    }
    assert!(!official("Pythonic news", "https://docs.python.org/3/"));
    assert!(!official(
        "PostgreSQL CTE",
        "https://www.postgresql.org/about/"
    ));
}

#[tokio::test]
async fn official_priority_keeps_candidate_set_and_baseline_order() -> Result<()> {
    // The actual sort hook is exercised through Session.search and its existing site filter.
    let input: Input =
        serde_json::from_value(json!({"question":"Python itertools.groupby","language":"en"}))?;
    let urls = vec![
        "https://third.example/tutorial".to_owned(),
        "https://docs.python.org/3/library/itertools.html".to_owned(),
        "https://fourth.example/doc".to_owned(),
    ];
    let mut original = Fake::new();
    original.urls = Some(urls.clone());
    let mut alternative = Fake::new();
    alternative.urls = Some(urls);
    let mut baseline = Session::new(&input, original, None);
    let mut candidate = Session::new(&input, alternative, None);
    candidate.prefer_official = true;
    baseline.search("Python groupby".into()).await?;
    candidate.search("Python groupby".into()).await?;
    let urls = |s: &Session<'_, Fake>| s.queue.iter().map(|t| t.url.clone()).collect::<Vec<_>>();
    assert_eq!(urls(&baseline)[0], "https://third.example/tutorial");
    assert_eq!(
        urls(&candidate)[0],
        "https://docs.python.org/3/library/itertools.html"
    );
    let mut a = urls(&baseline);
    let mut b = urls(&candidate);
    a.sort();
    b.sort();
    assert_eq!(a, b);
    assert_eq!(
        urls(&candidate)[1..],
        [
            "https://third.example/tutorial",
            "https://fourth.example/doc"
        ]
    );
    assert_eq!(baseline.budget.counts.serp, candidate.budget.counts.serp);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit serialized full native research experiment; root freezes cases/ledger/code first"]
async fn full_native_primary_search() -> Result<()> {
    native_primary_search(false).await
}

#[tokio::test]
#[ignore = "explicit new source-aware campaign; never resume old attempts or excluded hosts"]
async fn full_native_source_aware_search() -> Result<()> {
    native_primary_search(true).await
}

async fn native_primary_search(source_aware: bool) -> Result<()> {
    let fixture: Value =
        serde_json::from_slice(&std::fs::read(std::env::var("PRIMARY_SEARCH_INPUT")?)?)?;
    let directory = std::path::PathBuf::from(std::env::var("PRIMARY_SEARCH_OUTPUT")?);
    std::fs::create_dir(&directory)?;
    let input: Input = serde_json::from_value(
        json!({"question":fixture["question"],"language":fixture["language"]}),
    )?;
    input.validate()?;
    let arm = std::env::var("PRIMARY_SEARCH_ARM")?;
    ensure!(arm == "baseline" || arm == "candidate", "unknown arm");
    let state_path = std::path::PathBuf::from(std::env::var("PRIMARY_SEARCH_STATE")?);
    let state: Value = serde_json::from_slice(&std::fs::read(&state_path)?)?;
    let mut blocked = HashMap::new();
    for engine in ["bing", "ddg", "mwmbl", "seznam", "naver"] {
        if let Some(reason) = state["blocked"][engine].as_str() {
            blocked.insert(engine, reason.to_owned());
        }
    }
    let mut hosts: HashMap<String, String> =
        serde_json::from_value(state["document_blocks"].clone())?;
    let mut url_blocks: HashMap<String, String> = match state.get("document_url_blocks") {
        Some(value) => serde_json::from_value(value.clone())?,
        None => HashMap::new(),
    };
    url_blocks = url_blocks
        .into_iter()
        .map(|(url, reason)| Ok((document_url_key(&url)?, reason)))
        .collect::<Result<_>>()?;
    crate::mcp::update_host_blocks(&blocked, &hosts);
    let home = std::path::PathBuf::from(std::env::var("HOME")?);
    let model = crate::model::Model::from_openrouter(
        &home.join(".config/web-search/openrouter.key"),
        "deepseek/deepseek-v4.1-flash-1017",
        Some("http://127.0.0.1:10808"),
    )?
    .with_reranker(Some(&home.join(".omlx/settings.json")), true)?;
    let halt = Arc::new(Mutex::new(None));
    let backend = Guarded {
        live: Live {
            model: &model,
            blocked: &mut blocked,
            document_blocks: &mut hosts,
            capture: Some(&directory),
            site: crate::search::site_filter(&input.question)?,
            qwen_enabled: true,
            read_sequence: 0,
            serp_sequence: 0,
            observed_read_urls: Vec::new(),
            document_url_blocks: &mut url_blocks,
        },
        generation: Saved::generator(&model, &directory),
        halt: halt.clone(),
        source_aware,
    };
    let mut session = Session::new(&input, backend, Some(&directory));
    session.prefer_official = arm == "candidate";
    let outcome = session.execute(std::future::pending::<()>()).await;
    crate::mcp::update_host_blocks(&blocked, &hosts);
    let temporary = state_path.with_extension("next.json");
    let mut after = state.clone();
    after["blocked"] = json!(blocked);
    after["document_blocks"] = json!(hosts);
    after["document_url_blocks"] = json!(url_blocks);
    crate::save(&temporary, &after)?;
    std::fs::rename(temporary, state_path)?;
    crate::save(
        &directory.join("experiment.json"),
        &json!({"arm":arm,"source_aware":source_aware,"halt":*halt.lock().unwrap(),"model":"deepseek/deepseek-v4.1-flash-1017","pace_seconds":10,"oracle_urls":false}),
    )?;
    outcome?;
    ensure!(
        halt.lock().unwrap().is_none(),
        "experiment stopped on first error"
    );
    Ok(())
}

#[tokio::test]
async fn native_cancellation_blocks_next_operation_before_polling() -> Result<()> {
    let halt = Mutex::new(None);
    assert_eq!(guarded(&halt, async { Ok(7) }).await?, 7);
    assert!(halt.lock().unwrap().is_none());
    let future = guarded(&halt, std::future::pending::<Result<()>>());
    assert!(
        tokio::time::timeout(Duration::from_millis(1), future)
            .await
            .is_err()
    );
    assert!(halt.lock().unwrap().is_some());
    let mut polled = false;
    assert!(
        guarded(&halt, async {
            polled = true;
            Ok(())
        })
        .await
        .is_err()
    );
    assert!(!polled);
    let separate = Mutex::new(None);
    assert!(
        guarded::<()>(&separate, async { anyhow::bail!("source_challenge") })
            .await
            .is_err()
    );
    assert_eq!(
        separate.lock().unwrap().as_deref(),
        Some("source_challenge")
    );
    Ok(())
}

#[tokio::test]
async fn document_timeout_keeps_exclusions_and_allows_next_generation() -> Result<()> {
    let halt = Mutex::new(None);
    let mut blocks = HashMap::from([("old.example".into(), "original refusal".into())]);
    let touched = vec![
        "https://origin.example/doc".into(),
        "https://redirect.example/bad".into(),
    ];
    let mut urls = HashMap::new();
    let result: Result<()> = guarded(&halt, async { anyhow::bail!("fetch_timeout") }).await;
    assert!(recover_document_error(
        &halt,
        &mut blocks,
        &mut urls,
        &touched,
        &result.unwrap_err().to_string()
    ));
    assert_eq!(
        blocks.get("old.example").map(String::as_str),
        Some("original refusal")
    );
    assert!(!blocks.contains_key("origin.example") && !blocks.contains_key("redirect.example"));
    assert!(
        urls.contains_key("https://origin.example/doc")
            && urls.contains_key("https://redirect.example/bad")
    );
    assert!(!urls.contains_key("https://redirect.example/other"));
    assert_eq!(
        guarded(&halt, async { Ok("draft after document timeout") }).await?,
        "draft after document timeout"
    );
    let error: Result<()> = guarded(&halt, async {
        anyhow::bail!("OpenRouter HTTP 429 Too Many Requests")
    })
    .await;
    assert!(!recover_document_error(
        &halt,
        &mut blocks,
        &mut urls,
        &touched,
        &error.unwrap_err().to_string()
    ));
    let mut polled = false;
    assert!(
        guarded(&halt, async {
            polled = true;
            Ok(())
        })
        .await
        .is_err()
    );
    assert!(!polled);
    Ok(())
}

#[tokio::test]
async fn excluded_host_is_rejected_before_native_read_is_polled() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let settings = dir.path().join("settings.json");
    crate::save(
        &settings,
        &json!({"server":{"port":1},"auth":{"api_key":"fixture"}}),
    )?;
    let model = crate::model::Model::from_settings(&settings, "fixture")?;
    let mut engines = HashMap::new();
    let mut hosts = HashMap::from([(
        "excluded.example".into(),
        "fetch_timeout: earlier trial".into(),
    )]);
    let halt = Arc::new(Mutex::new(None));
    let mut url_blocks = HashMap::new();
    let mut backend = Guarded {
        live: Live {
            model: &model,
            blocked: &mut engines,
            document_blocks: &mut hosts,
            capture: None,
            site: None,
            qwen_enabled: false,
            read_sequence: 0,
            serp_sequence: 0,
            observed_read_urls: Vec::new(),
            document_url_blocks: &mut url_blocks,
        },
        generation: Saved::generator(&model, dir.path()),
        halt: halt.clone(),
        source_aware: true,
    };
    let error = backend
        .read(
            &Target {
                url: "https://excluded.example/doc".into(),
                parent: None,
            },
            "question",
        )
        .await
        .err()
        .context("excluded host unexpectedly read")?;
    assert!(error.to_string().contains("source_blocked"));
    assert!(backend.live.observed_read_urls.is_empty());
    assert!(halt.lock().unwrap().is_none());
    assert_eq!(
        backend
            .live
            .document_blocks
            .get("excluded.example")
            .map(String::as_str),
        Some("fetch_timeout: earlier trial")
    );
    let invalid = Target {
        url: "http://127.0.0.1/private".into(),
        parent: None,
    };
    assert!(backend.read(&invalid, "question").await.is_err());
    assert!(halt.lock().unwrap().is_some());
    assert!(backend.live.observed_read_urls.is_empty());
    assert!(
        backend
            .generate("prompt", "{}", json!({}), Duration::from_secs(1))
            .await
            .is_err()
    );
    assert!(!dir.path().join("request-attempted-01.json").exists());
    Ok(())
}

#[test]
fn url_exclusions_keep_query_path_and_hard_refusals_separate() -> Result<()> {
    let mut hosts = HashMap::from([(
        "old.example".to_owned(),
        "source_challenge: original".to_owned(),
    )]);
    let mut urls = HashMap::new();
    let halt = Mutex::new(Some("fetch_timeout".into()));
    let chain = vec![
        document_url_key("https://origin.example/start?a=1#top")?,
        document_url_key("https://docs.example/bad?q=1#anchor")?,
    ];
    assert!(recover_document_error(
        &halt,
        &mut hosts,
        &mut urls,
        &chain,
        "fetch_timeout"
    ));
    assert!(!hosts.contains_key("docs.example") && !hosts.contains_key("origin.example"));
    let mut seen = vec![];
    assert!(
        check_document_url(
            &reqwest::Url::parse("https://docs.example/bad?q=1#different")?,
            &urls,
            &mut seen
        )
        .is_err()
    );
    assert!(
        check_document_url(
            &reqwest::Url::parse("https://docs.example/other?q=1")?,
            &urls,
            &mut seen
        )
        .is_ok()
    );
    assert!(
        check_document_url(
            &reqwest::Url::parse("https://docs.example/bad?q=2")?,
            &urls,
            &mut seen
        )
        .is_ok()
    );
    assert!(recover_document_error(
        &halt, &mut hosts, &mut urls, &chain, "HTTP 403"
    ));
    assert!(hosts.contains_key("docs.example"));
    assert!(
        !hosts.contains_key("origin.example"),
        "redirect origin must not inherit the destination refusal"
    );
    assert_eq!(hosts["old.example"], "source_challenge: original");
    Ok(())
}
