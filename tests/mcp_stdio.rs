use anyhow::{Context, Result};
use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, CallToolResult, ClientConfig, ProtocolVersion},
    service::RunningService,
    transport::TokioChildProcess,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{process::Command, time::timeout};

fn binary() -> PathBuf {
    std::env::var_os("WEB_SEARCH_MCP_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_web-search")))
}

async fn client() -> Result<RunningService<RoleClient, ClientConfig>> {
    let mut command = Command::new(binary());
    command.arg("mcp");
    let transport = TokioChildProcess::new(command)?;
    Ok(timeout(
        Duration::from_secs(10),
        ClientConfig::default().serve(transport),
    )
    .await??)
}

async fn call(
    client: &RunningService<RoleClient, ClientConfig>,
    tool: &str,
    args: Value,
) -> Result<CallToolResult> {
    Ok(timeout(
        Duration::from_secs(210),
        client.call_tool(
            CallToolRequestParams::new(tool.to_owned())
                .with_arguments(args.as_object().unwrap().clone()),
        ),
    )
    .await??)
}

#[tokio::test]
async fn evidence_memory_is_startup_only_and_needs_no_key_for_other_tools() -> Result<()> {
    let mut command = Command::new(binary());
    command.args(["mcp", "--evidence-memory"]);
    let client = timeout(
        Duration::from_secs(10),
        ClientConfig::default().serve(TokioChildProcess::new(command)?),
    )
    .await??;
    assert_eq!(client.list_all_tools().await?.len(), 5);
    let value = call(&client, "ranked_search", json!({"query":"Rust Pin"}))
        .await?
        .structured_content
        .context("structured")?;
    assert_eq!(value["error"]["code"], "model_unconfigured");
    let rejected = call(
        &client,
        "ranked_search",
        json!({"query":"Rust Pin","evidence_memory":true}),
    )
    .await?;
    assert_eq!(rejected.is_error, Some(true));
    assert!(serde_json::to_string(&rejected)?.contains("unknown field"));
    let rejected = call(
        &client,
        "ranked_search",
        json!({"query":"Rust Pin","reranker_settings":"/tmp/untrusted-settings"}),
    )
    .await?;
    assert_eq!(rejected.is_error, Some(true));
    assert!(serde_json::to_string(&rejected)?.contains("unknown field"));
    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn cache_startup_preserves_validation_and_sticky_sources() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let db = temp.path().join("cache.sqlite");
    let state = temp.path().join("sources.json");
    std::fs::write(
        &state,
        r#"{"blocked":{"naver":"source_challenge: saved refusal"},"document_blocks":{"example.org":"source_blocked: saved refusal"}}"#,
    )?;
    for _ in 0..2 {
        let mut command = Command::new(binary());
        command
            .arg("mcp")
            .arg("--cache-db")
            .arg(&db)
            .arg("--source-state")
            .arg(&state);
        let client = timeout(
            Duration::from_secs(10),
            ClientConfig::default().serve(TokioChildProcess::new(command)?),
        )
        .await??;
        assert_eq!(client.list_all_tools().await?.len(), 5);
        let blocked = call(&client, "search", json!({"engine":"naver","query":"Rust"}))
            .await?
            .structured_content
            .context("structured")?;
        assert_eq!(blocked["error"]["code"], "source_disabled");
        let blocked = call(&client, "extract", json!({"urls":["https://example.org/"]}))
            .await?
            .structured_content
            .context("structured")?;
        assert!(
            blocked["documents"][0]["error"]
                .as_str()
                .unwrap()
                .contains("document_host_disabled")
        );
        let private = call(&client, "extract", json!({"urls":["http://127.0.0.1/"]})).await;
        assert!(private.unwrap_err().to_string().contains("unsafe_url"));
        client.cancel().await?;
    }
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    let raw = "https://origin.example/redirect";
    let page = json!({"url":"https://blocked.example/article","status":200,"html":"<p>cached public body</p>","hops":[raw,"https://blocked.example/article"],"cache":{"request_url":raw,"fetched_at":at,"checked_at":at,"expires_at":at+300,"etag":null,"last_modified":null,"allowed":true,"hit":false}});
    let bytes = serde_json::to_vec(&page)?;
    let connection = rusqlite::Connection::open(&db)?;
    connection.execute(
        "INSERT INTO entries VALUES('html-v1',?1,?1,?2,?3,?4,?4,?5,?4,?6)",
        rusqlite::params![
            raw,
            bytes,
            format!("{:x}", Sha256::digest(&bytes)),
            at,
            at + 300,
            bytes.len() as i64
        ],
    )?;
    drop(connection);
    std::fs::write(
        &state,
        r#"{"blocked":{},"document_blocks":{"blocked.example":"saved refusal"}}"#,
    )?;
    let mut command = Command::new(binary());
    command
        .arg("mcp")
        .arg("--cache-db")
        .arg(&db)
        .arg("--source-state")
        .arg(&state);
    let client = timeout(
        Duration::from_secs(10),
        ClientConfig::default().serve(TokioChildProcess::new(command)?),
    )
    .await??;
    let rejected = call(&client, "extract", json!({"urls":[raw]}))
        .await?
        .structured_content
        .context("structured")?;
    assert!(
        rejected["documents"][0]["error"]
            .as_str()
            .unwrap()
            .contains("host_disabled: saved refusal"),
        "must reject cached final host before DNS, no network to .example"
    );
    client.cancel().await?;
    let output = std::process::Command::new(binary())
        .arg("cache-clear")
        .arg("--cache-db")
        .arg(&db)
        .arg("--all")
        .output()?;
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout)?["removed"],
        1
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit OpenRouter plus local Qwen reranked MCP smoke; one live query"]
async fn live_qwen_reranked_search_over_stdio() -> Result<()> {
    let key = std::env::var_os("WEB_SEARCH_OPENROUTER_KEY_FILE").context("key file")?;
    let local = std::env::var_os("WEB_SEARCH_OMLX_SETTINGS").context("local settings")?;
    let mut command = Command::new(binary());
    command
        .args([
            "mcp",
            "--evidence-memory",
            "--model",
            "deepseek/deepseek-v4.1-flash-1017",
            "--openrouter-key-file",
        ])
        .arg(key)
        .arg("--reranker-settings")
        .arg(local);
    if let Ok(proxy) = std::env::var("WEB_SEARCH_OPENROUTER_PROXY") {
        command.args(["--openrouter-proxy", &proxy]);
    }
    let client = timeout(
        Duration::from_secs(10),
        ClientConfig::default().serve(TokioChildProcess::new(command)?),
    )
    .await??;
    let response = call(
        &client,
        "ranked_search",
        json!({"query":"SQLite foreign_keys pragma transaction no op","max_results":10}),
    )
    .await?;
    let value = response.structured_content.context("structured")?;
    let path = PathBuf::from("runs/reranker-2026-10-30/release-mcp-smoke.json");
    std::fs::write(path, serde_json::to_vec_pretty(&value)?)?;
    assert_eq!(value["page_verification"]["evidence_mode"], "coverage");
    let reranker = &value["page_verification"]["passage_reranker"];
    assert_eq!(reranker["model"], "Qwen3-Reranker-0.6B-4bit");
    let calls = reranker["calls"].as_array().context("reranker calls")?;
    assert!(!calls.is_empty() && calls.len() <= 4);
    assert!(calls.iter().all(|c| c["status"] == "ok"));
    client.cancel().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "explicit live audited v9 pipeline, OpenRouter + local Qwen; known healthy MDN domain control"]
async fn live_audited_reranker_pipeline() -> Result<()> {
    let mut command = Command::new(binary());
    command
        .args([
            "mcp",
            "--evidence-memory",
            "--model",
            "deepseek/deepseek-v4.1-flash-1017",
            "--openrouter-key-file",
        ])
        .arg(std::env::var_os("WEB_SEARCH_OPENROUTER_KEY_FILE").context("key")?)
        .arg("--reranker-settings")
        .arg(std::env::var_os("WEB_SEARCH_OMLX_SETTINGS").context("local settings")?);
    if let Ok(proxy) = std::env::var("WEB_SEARCH_OPENROUTER_PROXY") {
        command.args(["--openrouter-proxy", &proxy]);
    }
    let client = timeout(
        Duration::from_secs(10),
        ClientConfig::default().serve(TokioChildProcess::new(command)?),
    )
    .await??;
    let value=call(&client,"ranked_search",json!({"query":"site:developer.mozilla.org HTTP 202 Accepted processing completed guarantee","max_results":10})).await?.structured_content.context("structured")?;
    std::fs::write(
        PathBuf::from(
            std::env::var_os("WEB_SEARCH_EXPERIMENT_ROOT").context("new smoke output root")?,
        )
        .join("release-mcp.json"),
        serde_json::to_vec_pretty(&value)?,
    )?;
    assert_eq!(value["classifier_valid"], true);
    let memory = &value["page_verification"]["evidence_memory"];
    assert_eq!(memory["version"], "request-evidence-v14-news-anchors");
    assert!(value["budget"]["classification_calls"].as_u64().unwrap() <= 3);
    assert!(value["budget"]["reranker_calls"].as_u64().unwrap() > 0);
    assert!(
        memory["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["audit_supported"].is_boolean())
    );
    for r in value["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["classification"] == "direct")
    {
        assert!(
            memory["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["id"] == r["id"] && e["audit_supported"] == true)
        );
        assert!(
            r["requirement_evidence"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["context_only"] != true)
                .all(|p| p["quote"].as_str().unwrap().chars().count() <= 240)
        );
    }
    client.cancel().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "one explicit public n8n HTML read through MCP; no model"]
async fn live_extract_semantic_documentation() -> Result<()> {
    let client = client().await?;
    let response = call(&client,"extract",json!({"urls":["https://docs.n8n.io/deploy/host-n8n/configure-n8n/security/enable-ssrf-protection"]})).await?;
    let value = response
        .structured_content
        .context("structured extraction")?;
    let out = PathBuf::from("runs/relevance-extract-2026-10-29/mcp-extract.json");
    std::fs::create_dir_all(out.parent().unwrap())?;
    std::fs::write(&out, serde_json::to_vec_pretty(&value)?)?;
    assert_eq!(value["status"], "ok");
    let doc = &value["documents"][0];
    assert_eq!(doc["content_scope"], "main");
    assert_eq!(doc["coverage"], "not_verified");
    let text = doc["text"].as_str().context("text")?;
    assert!(text.contains("N8N_SSRF_PROTECTION_ENABLED=true") && text.contains("fc00::/7"));
    client.cancel().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "explicit live Seznam/Naver + local model; requires WEB_SEARCH_OMLX_SETTINGS"]
async fn live_ranked_search_over_stdio() -> Result<()> {
    let settings = std::env::var_os("WEB_SEARCH_OMLX_SETTINGS").context("settings required")?;
    let mut command = Command::new(binary());
    command.args(["mcp", "--omlx-settings"]).arg(settings);
    let client = timeout(
        Duration::from_secs(10),
        ClientConfig::default().serve(TokioChildProcess::new(command)?),
    )
    .await??;
    let response = call(
        &client,
        "ranked_search",
        json!({"query":"CSS box-sizing border-box content-box padding border difference","max_results":3}),
    )
    .await?;
    let value = response
        .structured_content
        .context("structured ranked result")?;
    let path = PathBuf::from("runs/recall-2026-10-29/mcp-ranked-smoke.json");
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, serde_json::to_vec_pretty(&value)?)?;
    assert_ne!(value["status"], "error");
    assert_eq!(value["forms"].as_array().unwrap().len(), 1);
    assert_eq!(value["budget"]["logical_search_calls"], 2);
    assert_eq!(value["classifier_valid"], true);
    assert!(value["budget"]["document_reads"].as_u64().unwrap() <= 4);
    assert!(value["budget"]["classification_calls"].as_u64().unwrap() <= 3);
    for result in value["results"].as_array().unwrap() {
        if let Some(proofs) = result["requirement_evidence"].as_array() {
            assert_eq!(result["classification"], "direct");
            assert_eq!(result["document_http_status"], 200);
            assert_eq!(result["fetch_method"], "http_extract");
            for proof in proofs {
                assert_eq!(proof["text_hash"].as_str().unwrap().len(), 64);
                assert!((16..=1200).contains(&proof["quote"].as_str().unwrap().chars().count()));
            }
        }
    }
    assert!(value["results"].as_array().unwrap().len() <= 3);
    for private in [
        "candidates",
        "sources",
        "model_calls",
        "selected_before_site",
    ] {
        assert!(value.get(private).is_none());
    }
    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn stdio_discovery_and_validation() -> Result<()> {
    let mut client = client().await?;
    let info = client.peer_info().context("missing server info")?;
    let implementation = info
        .server_info
        .as_ref()
        .context("missing implementation")?;
    assert_eq!(implementation.name, "web-search");
    assert_eq!(implementation.version, env!("CARGO_PKG_VERSION"));
    let tools = client.list_all_tools().await?;
    let mut names: Vec<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    names.sort();
    assert_eq!(
        names,
        ["discover", "extract", "ranked_search", "research", "search"]
    );
    let schema = serde_json::to_string(
        &tools
            .iter()
            .find(|tool| tool.name == "search")
            .unwrap()
            .input_schema,
    )?;
    assert!(schema.contains("\"seznam\"") && schema.contains("\"naver\""));
    for tool in &tools {
        assert_eq!(
            tool.input_schema.get("additionalProperties"),
            Some(&json!(false))
        );
        assert_eq!(
            tool.annotations.as_ref().unwrap().read_only_hint,
            Some(true)
        );
    }
    // All rejected before network access, including a mixed batch with a public URL.
    for (tool, args) in [
        ("research", json!({"question":" "})),
        ("research", json!({"question":"x".repeat(2001)})),
        ("research", json!({"question":"site:127.0.0.1 probe"})),
        (
            "research",
            json!({"question":"probe","model":"other/provider"}),
        ),
        (
            "research",
            json!({"question":"probe","max_generations":999}),
        ),
        (
            "research",
            json!({"question":"probe","cache_mode":"unsafe"}),
        ),
        ("ranked_search", json!({"query":" ","max_results":3})),
        ("ranked_search", json!({"query":"rust","max_results":0})),
        (
            "ranked_search",
            json!({"query":"rust","settings":"/etc/passwd"}),
        ),
        ("ranked_search", json!({"query":"site:localhost rust"})),
        ("discover", json!({"url":"http://127.1/", "query":"wal"})),
        (
            "discover",
            json!({"url":"https://example.com/", "query":" "}),
        ),
        (
            "discover",
            json!({"url":"https://example.com/", "query":"wal", "max_links":0}),
        ),
        (
            "discover",
            json!({"url":"https://example.com/", "query":"wal", "max_links":51}),
        ),
        (
            "discover",
            json!({"url":"https://example.com/", "query":"я".repeat(2001)}),
        ),
        (
            "discover",
            json!({"url":"https://example.com/", "query":"wal", "settings":"bad"}),
        ),
        ("search", json!({"query":" ", "engine":"bing"})),
        ("search", json!({"query":"я".repeat(2001), "engine":"bing"})),
        (
            "search",
            json!({"query":"rust", "engine":"bing", "max_results":0}),
        ),
        (
            "search",
            json!({"query":"rust", "engine":"bing", "max_results":11}),
        ),
        ("search", json!({"query":"rust", "engine":"unknown"})),
        (
            "search",
            json!({"query":"rust", "engine":"bing", "shell":"echo bad"}),
        ),
        ("search", json!({"query":"rust"})),
        (
            "search",
            json!({"query":"site:localhost ownership", "engine":"bing"}),
        ),
        (
            "search",
            json!({"query":"site:a.example OR site:b.example rust", "engine":"bing"}),
        ),
        (
            "search",
            json!({"query":"-site:a.example rust", "engine":"bing"}),
        ),
        ("extract", json!({"urls":[]})),
        ("extract", json!({"urls":vec!["https://example.com/";9]})),
        (
            "extract",
            json!({"urls":["https://example.com/", "http://127.1/"]}),
        ),
        ("extract", json!({"urls":["file:///etc/passwd"]})),
        ("extract", json!({"urls":["http://[::1]/"]})),
        (
            "extract",
            json!({"urls":["https://user:secret@example.com/"]}),
        ),
        (
            "extract",
            json!({"urls":[format!("https://example.com/{}", "a".repeat(4096))]}),
        ),
        (
            "extract",
            json!({"urls":["https://example.com/"], "settings":"/etc/passwd"}),
        ),
        ("research", json!({"question":"rust"})),
    ] {
        match timeout(Duration::from_secs(5), call(&client, tool, args.clone())).await? {
            Ok(result) => assert_eq!(result.is_error, Some(true), "accepted {tool}: {args}"),
            Err(error) => assert!(
                matches!(
                    error.downcast_ref::<rmcp::ServiceError>(),
                    Some(rmcp::ServiceError::McpError(_))
                ),
                "unexpected failure: {error}"
            ),
        }
    }
    // A bad request must not kill the session.
    let unconfigured = call(&client, "ranked_search", json!({"query":"rust ownership"})).await?;
    assert_eq!(
        unconfigured.structured_content.unwrap()["error"]["code"],
        "model_unconfigured"
    );
    let unconfigured = call(
        &client,
        "research",
        json!({"question":"Does this have source support?","language":"en","cache_mode":"refresh"}),
    )
    .await?;
    assert_eq!(
        unconfigured.structured_content.unwrap()["error"]["code"],
        "model_unconfigured"
    );
    assert_eq!(client.list_all_tools().await?.len(), 5);
    client.close().await?;
    Ok(())
}

#[tokio::test]
async fn stdio_legacy_initialize() -> Result<()> {
    let mut command = Command::new(binary());
    command.arg("mcp");
    let transport = TokioChildProcess::new(command)?;
    let mut client = timeout(
        Duration::from_secs(10),
        ClientConfig::default()
            .with_protocol_version(ProtocolVersion::V_2025_11_25)
            .serve(transport),
    )
    .await??;
    let info = client.peer_info().context("missing initialize response")?;
    assert_eq!(info.protocol_version, ProtocolVersion::V_2025_11_25);
    assert!(info.capabilities.tools.is_some());
    assert_eq!(client.list_all_tools().await?.len(), 5);
    client.close().await?;
    Ok(())
}

#[tokio::test]
async fn stdio_eof_exits_cleanly() -> Result<()> {
    let mut child = Command::new(binary())
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    drop(child.stdin.take());
    let output = timeout(Duration::from_secs(5), child.wait_with_output()).await??;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "non-protocol stdout on startup/EOF"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit live docs navigation: one seed and at most two extracted pages"]
async fn live_docs_navigation() -> Result<()> {
    let fixture: Value =
        serde_json::from_str(include_str!("../feasibility/docs-evidence-cases.json"))?;
    let case = &fixture["navigation"];
    let output = PathBuf::from("runs/docs-evidence-2026-10-28");
    std::fs::create_dir(&output)?;
    let mut client = client().await?;
    let args = json!({"url":case["seed_url"],"query":case["query"],"max_links":case["max_links"]});
    let discovery = call(&client, "discover", args.clone()).await?;
    let value = discovery
        .structured_content
        .context("missing discovery result")?;
    let mut trace = vec![json!({"tool":"discover","arguments":args,"result":value})];
    std::fs::write(
        output.join("trace.json"),
        serde_json::to_vec_pretty(&trace)?,
    )?;
    assert_ne!(discovery.is_error, Some(true), "{value}");
    let links = value["links"].as_array().context("missing links")?;
    let mut urls = Vec::new();
    for term in case["selection_label_terms"].as_array().unwrap() {
        if let Some(link) = links.iter().find(|link| {
            link["label"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains(term.as_str().unwrap())
        }) {
            let url = link["url"].as_str().unwrap().to_owned();
            if !urls.contains(&url) {
                urls.push(url);
            }
        }
    }
    assert!(
        !urls.is_empty(),
        "no matching navigation labels; no URLs guessed"
    );
    assert!(urls.len() <= 2 && urls.len() < case["max_document_loads"].as_u64().unwrap() as usize);
    let args = json!({"urls":urls});
    let extracted = call(&client, "extract", args.clone()).await?;
    let documents = extracted
        .structured_content
        .context("missing extraction result")?;
    trace.push(json!({"tool":"extract","arguments":args,"result":documents}));
    let binary_hash = format!("{:x}", Sha256::digest(std::fs::read(binary())?));
    std::fs::write(
        output.join("trace.json"),
        serde_json::to_vec_pretty(&trace)?,
    )?;
    std::fs::write(
        output.join("execution.json"),
        serde_json::to_vec_pretty(&json!({
            "binary_sha256":binary_hash,"fixture":fixture,"document_loads":1+urls.len(),
            "selection":"First real returned label matching each fixture term; no deep URL supplied or guessed",
            "interpretation":"MCP navigation/extraction diagnostic, not semantic sufficiency or general search acceptance"
        }))?,
    )?;
    assert_ne!(extracted.is_error, Some(true), "{documents}");
    client.close().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "20 live Bing requests; run explicitly, stop on blocking"]
async fn live_bing_quality_capture() -> Result<()> {
    capture_quality("bing").await
}

#[tokio::test]
#[ignore = "20 live Mwmbl requests; run explicitly, stop on blocking"]
async fn live_mwmbl_quality_capture() -> Result<()> {
    capture_quality("mwmbl").await
}

#[tokio::test]
#[ignore = "two remaining Mwmbl cases after correcting the local source-label guard"]
async fn live_mwmbl_remaining_cases() -> Result<()> {
    let cases: Vec<Value> = serde_json::from_str(include_str!("../feasibility/search-cases.json"))?;
    let mut client = client().await?;
    let mut records = Vec::new();
    for case in cases
        .into_iter()
        .filter(|case| matches!(case["id"].as_str(), Some("en09" | "en10")))
    {
        let result = call(
            &client,
            "search",
            json!({"engine":"mwmbl","query":case["query"],"max_results":10}),
        )
        .await?;
        let value = result
            .structured_content
            .context("missing structuredContent")?;
        let blocked = [
            "source_challenge",
            "source_blocked",
            "rate_limited",
            "source_policy_changed",
            "captcha",
        ]
        .contains(&value["error_code"].as_str().unwrap_or(""));
        println!(
            "{} {} {}",
            case["id"],
            value["status"],
            value["results"].as_array().map_or(0, Vec::len)
        );
        records.push(json!({"case":case,"result":value,"is_error":result.is_error == Some(true)}));
        if blocked {
            break;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    client.close().await?;
    let report = json!({"reason":"first run stopped at a local source-label guard, not a remote block; no repeat of successful cases", "binary_sha256":format!("{:x}",Sha256::digest(std::fs::read(binary())?)),"cases":records});
    std::fs::write(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("runs/reliability-2026-10-28/mwmbl-continuation.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}

async fn capture_quality(engine: &str) -> Result<()> {
    let cases: Vec<Value> = serde_json::from_str(include_str!("../feasibility/search-cases.json"))?;
    assert_eq!(cases.len(), 20);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("runs/quality-{engine}-{stamp}"));
    std::fs::create_dir(&output)?;
    let mut client = client().await?;
    let mut report = json!({
        "started_unix_seconds":stamp, "engine":engine, "transport":"stdio", "max_results":10,
        "case_set_sha256":format!("{:x}", Sha256::digest(include_bytes!("../feasibility/search-cases.json"))),
        "tested_binary_sha256":format!("{:x}", Sha256::digest(std::fs::read(binary())?)),
        "server_info":client.peer_info(), "cases":[], "complete":false,
        "quality_pass":null, "rubric":"doc/search-quality-rubric-v1.md",
        "excluded_source":{"ddg":"earlier HTTP 202; no new request"}
    });
    let mut blocked = false;
    for case in cases {
        let record = if blocked {
            json!({"case":case, "skipped":"source blocked earlier in this run"})
        } else {
            let result = call(
                &client,
                "search",
                json!({"engine":engine, "query":case["query"], "max_results":10}),
            )
            .await?;
            let value = result
                .structured_content
                .context("missing search structuredContent")?;
            let code = value["error_code"].as_str().unwrap_or("");
            blocked = [
                "captcha",
                "source_challenge",
                "source_blocked",
                "rate_limited",
                "source_policy_changed",
            ]
            .contains(&code);
            let record =
                json!({"case":case, "is_error":result.is_error == Some(true), "result":value});
            std::fs::write(
                output.join(format!("{}.json", case["id"].as_str().unwrap())),
                serde_json::to_vec_pretty(&record)?,
            )?;
            record
        };
        println!(
            "{}: status={} results={} skipped={}",
            case["id"],
            record["result"]["status"],
            record["result"]["results"].as_array().map_or(0, Vec::len),
            record["skipped"]
        );
        report["cases"].as_array_mut().unwrap().push(record);
        std::fs::write(
            output.join("capture.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        if !blocked {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
    client.close().await?;
    report["complete"] = json!(true);
    std::fs::write(
        output.join("capture.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("quality capture: {}", output.display());
    Ok(())
}

#[tokio::test]
#[ignore = "live public HTTP/search requests; run explicitly"]
async fn live_search_and_extract_over_stdio() -> Result<()> {
    let mut client = client().await?;
    let mut report = json!({
        "date":"2026-10-28", "transport":"stdio", "client":"rmcp 3.8.0",
        "server_info":client.peer_info(), "search_relevance_qualified":false,
        "tools":client.list_all_tools().await?,
        "tested_binary_sha256":format!("{:x}", Sha256::digest(std::fs::read(binary())?)),
    });
    let extracted = call(
        &client,
        "extract",
        json!({"urls":[
            "https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html",
            "https://doc.rust-lang.org/book/this-page-does-not-exist-mcp-smoke.html"
        ]}),
    )
    .await?;
    assert_ne!(extracted.is_error, Some(true));
    let extracted = extracted
        .structured_content
        .context("missing extract structuredContent")?;
    assert_eq!(extracted["status"], "partial");
    let document = &extracted["documents"][0];
    assert_eq!(document["status"], "ok");
    let text = document["text"]
        .as_str()
        .context("missing extracted text")?;
    assert!(text.contains("Ownership"));
    assert_eq!(
        document["text_hash"],
        format!("{:x}", Sha256::digest(text.as_bytes()))
    );
    assert!(
        document["final_url"]
            .as_str()
            .unwrap()
            .starts_with("https://doc.rust-lang.org/")
    );
    assert_eq!(extracted["documents"][1]["status"], "error");
    report["extract"] = extracted;
    let failed = call(
        &client,
        "extract",
        json!({"urls":["https://doc.rust-lang.org/book/this-page-does-not-exist-mcp-smoke.html"]}),
    )
    .await?;
    assert_eq!(failed.is_error, Some(true));
    let failed = failed
        .structured_content
        .context("missing failed extraction")?;
    assert_eq!(failed["status"], "error");
    assert_eq!(failed["documents"][0]["status"], "error");
    report["extract_all_failed"] = failed;
    // One request per engine. An upstream error is an expected tool failure, not MCP success.
    for engine in ["bing", "ddg"] {
        let result = call(&client, "search", json!({"query":"Rust ownership borrowing documentation", "engine":engine, "max_results":3})).await?;
        let failed = result.is_error == Some(true);
        let value = result
            .structured_content
            .context("missing search structuredContent")?;
        assert_eq!(failed, value["status"] == "error");
        if !failed {
            assert!(
                value["results"]
                    .as_array()
                    .context("missing results")?
                    .len()
                    <= 3
            );
            assert_eq!(value["untrusted_content"], true);
        }
        report[engine] = json!({"is_error":failed, "result":value});
        let message = value["error"].as_str().unwrap_or("");
        if ["captcha", "HTTP 202", "rate_limited", "source_blocked"]
            .iter()
            .any(|marker| message.contains(marker))
        {
            let disabled =
                call(&client, "search", json!({"query":"Rust", "engine":engine})).await?;
            assert_eq!(disabled.is_error, Some(true));
            assert_eq!(
                disabled.structured_content.as_ref().unwrap()["error"]["code"],
                "source_disabled"
            );
            report[engine]["repeat_after_block"] = json!("source_disabled; no network retry");
        }
    }
    client.close().await?;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runs/mcp-2026-10-28/live.json");
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, serde_json::to_vec_pretty(&report)?)?;
    println!("live MCP report: {}", path.display());
    Ok(())
}

#[tokio::test]
async fn openrouter_startup_discovers_tools_without_using_the_key() -> Result<()> {
    let key = tempfile::NamedTempFile::new()?;
    std::fs::write(key.path(), "unused-test-placeholder")?;
    let mut command = Command::new(binary());
    command
        .args(["mcp", "--openrouter-key-file"])
        .arg(key.path())
        .args(["--model", "test/model"]);
    let client = timeout(
        Duration::from_secs(10),
        ClientConfig::default().serve(TokioChildProcess::new(command)?),
    )
    .await??;
    assert_eq!(client.list_all_tools().await?.len(), 5);
    client.cancel().await?;
    let status = Command::new(binary())
        .args([
            "mcp",
            "--omlx-settings",
            "unused",
            "--openrouter-key-file",
            "unused",
            "--model",
            "test/model",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;
    assert!(!status.success());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit OpenRouter + Seznam/Naver + up to4 MDN reads; requires key/proxy startup env"]
async fn live_openrouter_ranked_over_stdio() -> Result<()> {
    let key = std::env::var_os("WEB_SEARCH_OPENROUTER_KEY_FILE").context("key path required")?;
    let proxy = std::env::var("WEB_SEARCH_OPENROUTER_PROXY")
        .context("existing proxy required for this probe")?;
    let out = std::env::var_os("WEB_SEARCH_MCP_LIVE_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("runs/openrouter-access-2026-10-30/mcp-live.json"));
    anyhow::ensure!(!out.exists(), "preserve previous capture");
    let mut command = Command::new(binary());
    command
        .args(["mcp", "--openrouter-key-file"])
        .arg(key)
        .args([
            "--evidence-memory",
            "--model",
            "deepseek/deepseek-v4.1-flash-1017",
            "--openrouter-proxy",
            &proxy,
        ]);
    let client = timeout(
        Duration::from_secs(10),
        ClientConfig::default().serve(TokioChildProcess::new(command)?),
    )
    .await??;
    let response=call(&client,"ranked_search",json!({"query":"site:developer.mozilla.org CSS box-sizing border-box content-box padding border difference","max_results":3})).await?;
    let value = response
        .structured_content
        .context("structured output required")?;
    std::fs::write(&out, serde_json::to_vec_pretty(&value)?)?;
    assert_eq!(value["model_provider"], "openrouter");
    assert_eq!(value["model"], "deepseek/deepseek-v4.1-flash-1017");
    assert!(value["classifier_valid"].is_boolean());
    let results = value["results"].as_array().context("results")?;
    assert!(!results.is_empty() && results.len() <= 3);
    for result in results {
        let url = reqwest::Url::parse(result["url"].as_str().unwrap())?;
        assert!(
            url.host_str().is_some_and(
                |h| h == "developer.mozilla.org" || h.ends_with(".developer.mozilla.org")
            )
        );
    }
    assert!(value["budget"]["classification_calls"].as_u64().unwrap() <= 3);
    assert!(value["budget"]["document_reads"].as_u64().unwrap() <= 4);
    if value["classifier_valid"] == true {
        let memory = &value["page_verification"]["evidence_memory"];
        assert_eq!(memory["version"], "request-evidence-v2");
        assert!(memory["entries"].as_array().unwrap().len() <= 10);
        for e in memory["entries"].as_array().unwrap() {
            for p in e["passages"].as_array().unwrap() {
                assert!(p.get("text").is_none());
            }
        }
        for r in results {
            if r["classification"] == "direct" {
                assert!(
                    r["requirement_evidence"]
                        .as_array()
                        .is_some_and(|p| !p.is_empty())
                );
            }
        }
    }
    for private in ["model_calls", "sources", "candidates"] {
        assert!(value.get(private).is_none());
    }
    client.cancel().await?;
    Ok(())
}
