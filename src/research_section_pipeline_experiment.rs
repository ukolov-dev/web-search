//! Explicit saved-source pipeline experiment; no public HTTP or production wiring.
use super::*;

struct Saved<'a> {
    model: &'a crate::model::Model,
    source: &'a Value,
    qwen: bool,
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
        self.model
            .research_call(prompt, context, schema, 3072, remaining)
            .await
    }
}

#[tokio::test]
#[ignore = "explicit saved-source Qwen/model pipeline; no public HTTP; frozen input/output directory required"]
async fn section_saved_source_pipeline() -> Result<()> {
    let path = std::path::PathBuf::from(std::env::var("SECTION_PIPELINE_INPUT")?);
    let directory = std::path::PathBuf::from(std::env::var("SECTION_PIPELINE_OUTPUT")?);
    // Never overwrite an attempted model trial.
    std::fs::create_dir(&directory)?;
    let fixture: Value = serde_json::from_slice(&std::fs::read(path)?)?;
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
