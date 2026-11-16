#[cfg(test)]
mod benchmark;
mod browser;
mod cache;
#[cfg(test)]
mod coverage_replay;
mod discover;
mod evidence;
#[cfg(test)]
mod evidence_probe;
#[cfg(test)]
mod firecrawl_benchmark;
mod mcp;
mod model;
mod net;
mod news;
mod news_search;
mod policy;
#[cfg(test)]
mod reliability_benchmark;
mod reranker;
#[cfg(test)]
mod reranker_benchmark;
mod rescue;
mod research;
mod search;

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(about = "Stage-1 feasibility probes; not the production research API")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Bounded two-round research with audited claims; explicit live permission required.
    Research {
        question: String,
        #[arg(long, value_enum, default_value_t = research::Language::Auto)]
        language: research::Language,
        #[arg(long, value_enum, default_value_t = research::CacheMode::Prefer)]
        cache_mode: research::CacheMode,
        #[arg(long, required_unless_present = "openrouter_key_file")]
        settings: Option<PathBuf>,
        #[arg(long, conflicts_with = "settings")]
        openrouter_key_file: Option<PathBuf>,
        #[arg(long, requires = "openrouter_key_file")]
        openrouter_proxy: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        reranker_settings: Option<PathBuf>,
        #[arg(long)]
        source_state: Option<PathBuf>,
        #[arg(long)]
        cache_db: Option<PathBuf>,
        #[arg(long)]
        capture_dir: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        live: bool,
    },
    /// Run the local MCP server over stdin/stdout (no listening socket).
    Mcp {
        #[arg(long)]
        omlx_settings: Option<PathBuf>,
        #[arg(long, conflicts_with = "omlx_settings")]
        openrouter_key_file: Option<PathBuf>,
        #[arg(long, requires = "openrouter_key_file")]
        openrouter_proxy: Option<String>,
        #[arg(long)]
        model: Option<String>,
        /// Experimental request evidence memory; legacy remains default pending qualification.
        #[arg(long)]
        evidence_memory: bool,
        /// Local Qwen3 reranker settings; requires evidence-memory.
        #[arg(long, requires = "evidence_memory")]
        reranker_settings: Option<PathBuf>,
        /// Explicit diagnostic capture of public sources and model calls into a new directory.
        #[arg(long)]
        capture_dir: Option<PathBuf>,
        /// Preserve source refusals across process restarts; trusted local diagnostic state.
        #[arg(long)]
        source_state: Option<PathBuf>,
        /// Opt-in persistent cache of public HTML, extracted text and local reranker scores.
        #[arg(long)]
        cache_db: Option<PathBuf>,
        /// Revalidate source bodies even if the local TTL has not expired.
        #[arg(long, requires = "cache_db")]
        cache_refresh: bool,
    },
    /// Clear a trusted local cache. URL cleanup also clears dependent reranker scores.
    CacheClear {
        #[arg(long)]
        cache_db: PathBuf,
        #[arg(long, conflicts_with = "all", required_unless_present = "all")]
        url: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Frozen B17 retrieval/classification suite; requires explicit live permission flag.
    RankedSuite {
        #[arg(long, required_unless_present = "openrouter_key_file")]
        settings: Option<PathBuf>,
        #[arg(long, conflicts_with = "settings")]
        openrouter_key_file: Option<PathBuf>,
        #[arg(long, requires = "openrouter_key_file")]
        openrouter_proxy: Option<String>,
        #[arg(long)]
        model: Option<String>,
        /// Experimental request evidence memory; legacy remains default pending qualification.
        #[arg(long)]
        evidence_memory: bool,
        /// Local Qwen3 reranker settings; requires evidence-memory.
        #[arg(long, requires = "evidence_memory")]
        reranker_settings: Option<PathBuf>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        live: bool,
    },
    /// Ranked search with up to four page reads. Known calibration queries use frozen B17 forms; others use only the original.
    RankedSearch {
        query: String,
        #[arg(long, required_unless_present = "openrouter_key_file")]
        settings: Option<PathBuf>,
        #[arg(long, conflicts_with = "settings")]
        openrouter_key_file: Option<PathBuf>,
        #[arg(long, requires = "openrouter_key_file")]
        openrouter_proxy: Option<String>,
        #[arg(long)]
        model: Option<String>,
        /// Experimental request evidence memory; legacy remains default pending qualification.
        #[arg(long)]
        evidence_memory: bool,
        /// Local Qwen3 reranker settings; requires evidence-memory.
        #[arg(long, requires = "evidence_memory")]
        reranker_settings: Option<PathBuf>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        live: bool,
    },
    Search {
        engine: String,
        query: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Discover same-origin navigation links from one public HTML page; no recursive fetch.
    Discover {
        url: String,
        query: String,
        #[arg(long, default_value_t = 20)]
        max_links: usize,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Extract {
        url: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    ExtractFile {
        html: PathBuf,
        url: String,
        #[arg(long)]
        output: PathBuf,
    },
    SearchSuite {
        #[arg(long, default_value = "runs/search")]
        output: PathBuf,
    },
    ExtractSuite {
        #[arg(long, default_value = "runs/extract")]
        output: PathBuf,
    },
    Omlx {
        #[arg(long)]
        settings: PathBuf,
        #[arg(long)]
        model: String,
        #[arg(long, default_value = "runs/omlx.json")]
        output: PathBuf,
    },
    StackCheck,
    Browser {
        #[arg(long)]
        executable: Option<PathBuf>,
        #[arg(long, default_value = "runs/browser.json")]
        output: PathBuf,
    },
}

fn save(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn extract(html: String, url: &str) -> Result<Value> {
    extract_cached(html, url, false)
}

fn extract_cached(html: String, url: &str, cacheable: bool) -> Result<Value> {
    let key = cache::key(&json!(["extract-v2", url, cache::hash(html.as_bytes())]));
    if cacheable
        && let Some(value) = cache::get("extract-v2", &key, false)
        && value["text"]
            .as_str()
            .is_some_and(|text| value["text_hash"] == cache::hash(text.as_bytes()))
    {
        return Ok(value);
    }
    cache::extraction();
    let value = extract_uncached(html, url)?;
    let at = cache::now();
    if cacheable {
        cache::put(
            "extract-v2",
            &key,
            url,
            &value,
            at,
            at,
            at + cache::DOCUMENT_TTL,
        );
    }
    Ok(value)
}

fn extract_uncached(html: String, url: &str) -> Result<Value> {
    let news = news::metadata(&html, url);
    let source = dom_query::Document::from(html.as_str());
    ensure!(
        source.select("*").length() <= 50_000,
        "extraction_failed: DOM element limit"
    );
    let code_scripts = source.select("react-app[app-name='code-view'] script[type='application/json'][data-target='react-app.embeddedData']");
    let code_blob = (|| -> Option<String> {
        if code_scripts.length() != 1 {
            return None;
        }
        let data = code_scripts.text();
        if data.len() > net::MAX_HTML {
            return None;
        }
        let value: Value = serde_json::from_str(&data).ok()?;
        let payload = &value["payload"];
        let route = &payload["codeViewBlobLayoutRoute"];
        let blob = &route["blob"];
        let path = route["path"].as_str()?;
        if path.is_empty()
            || payload["codeViewLayoutRoute"]["path"].as_str()? != path
            || !reqwest::Url::parse(url)
                .ok()?
                .path()
                .ends_with(&format!("/{path}"))
            || blob["truncated"] != false
            || blob["large"] != false
            || blob["viewable"] != true
            || blob["headerInfo"]["isGitLfs"] != false
            || blob["headerInfo"]["mode"] != "file"
        {
            return None;
        }
        let lines = payload["codeViewBlobLayoutRoute.StyledBlob"]["rawLines"].as_array()?;
        let count: usize = blob["headerInfo"]["lineInfo"]["truncatedLoc"]
            .as_str()?
            .parse()
            .ok()?;
        if lines.is_empty() || lines.len() > 100_000 || lines.len() != count {
            return None;
        }
        let lines = lines
            .iter()
            .map(Value::as_str)
            .collect::<Option<Vec<_>>>()?;
        if lines.iter().any(|line| line.contains(['\n', '\r'])) {
            return None;
        }
        let text = lines.join("\n");
        if text.trim().is_empty() {
            return None;
        }
        // File contents are text, so code resembling markup must survive the existing DOM pruning.
        Some(format!(
            "<pre>{}</pre>",
            text.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        ))
    })();
    let code_unavailable = code_scripts.length() > 0 && code_blob.is_none();
    let reference_type = source.select("head meta[name=keywords][content$=' class']");
    let reference_title = source.select("div.header > h2.title");
    let reference_body = reference_type.length() == 1
        && reference_title.length() == 1
        && reference_type
            .attr("content")
            .zip(reference_title.attr("title"))
            .is_some_and(|(kind, title)| {
                kind.strip_suffix(" class")
                    .and_then(|name| name.rsplit('.').next())
                    == title.strip_prefix("Class ").filter(|name| !name.is_empty())
            })
        && source
            .select("div.contentContainer > div.description")
            .length()
            == 1
        && source.select("div.contentContainer > div.summary").length() == 1
        && source.select("div.contentContainer > div.details").length() == 1
        && source
            .select("div.contentContainer > div.details a[name='method.detail']")
            .length()
            == 1;
    let declared_rfc = research::rfc_url(url).is_some_and(|n| {
        source.select("body").length() == 1
            && source.select("meta[name='rfc.number']").length() == 1
            && source
                .select("meta[name='rfc.number']")
                .attr("content")
                .as_deref()
                == Some(n.to_string().as_str())
            && source.select("meta[name='generator']").length() == 1
            && source
                .select("meta[name='generator']")
                .attr("content")
                .is_some_and(|v| v.starts_with("xml2rfc "))
            && source.select("h1#rfcnum").length() == 1
            && source.select("h1#rfcnum").text().trim() == format!("RFC {n}")
            && source.select("h1#title").length() == 1
            && !source.select("h1#title").text().trim().is_empty()
            && source
                .select("head title")
                .text()
                .starts_with(&format!("RFC {n}:"))
    });
    let scholarly = source.select("meta[name=citation_title]").length() > 0;
    let selected = [
        ("main, [role=main]", "semantic_main", "main"),
        ("article", "semantic_article", "article"),
        ("[data-pagefind-body]", "declared_body", "declared_body"),
        (
            "div.contentContainer",
            "declared_reference_body",
            "api_reference",
        ),
        (
            "#article-body, .article-body, [role=doc-body]",
            "scholarly_body",
            "article_body",
        ),
        (
            ".abstract, [role=doc-abstract]",
            "scholarly_abstract",
            "abstract",
        ),
    ]
    .into_iter()
    .find(|(selector, _, scope)| {
        let count = source.select(selector).length();
        if *scope == "api_reference" {
            reference_body && count == 1
        } else if *scope == "article_body" {
            scholarly && count == 1
        } else if *scope == "abstract" {
            scholarly && (1..=8).contains(&count)
        } else {
            count == 1
        }
    });
    let (content, title, method, scope) = if let Some(content) = code_blob {
        (
            content,
            source.select("head title").text().trim().to_owned(),
            "declared_code_blob",
            "file_source",
        )
    } else if declared_rfc {
        (
            source.select("body").html().to_string(),
            source.select("head title").text().trim().to_owned(),
            "declared_rfc_body",
            "rfc_body",
        )
    } else if let Some(article) = news::main_article_scope(&source, url)
        .filter(|scope| news["kind"] == "article" && scope.is("[itemscope]"))
    {
        (
            article.html().to_string(),
            source.select("head title").text().trim().to_owned(),
            "owned_article",
            "article",
        )
    } else if let Some((selector, method, scope)) = selected {
        (
            source
                .select(selector)
                .iter()
                .map(|node| node.html().to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            source.select("head title").text().trim().to_owned(),
            method,
            scope,
        )
    } else {
        let config = dom_smoothie::Config {
            max_elements_to_parse: 50_000,
            ..Default::default()
        };
        let mut reader = dom_smoothie::Readability::new(html, Some(url), Some(config))?;
        let article = reader.parse()?;
        (
            article.content.to_string(),
            article.title,
            "readability",
            "inferred_article",
        )
    };
    let doc = dom_query::Document::from(content);
    doc.select(
        "script,style,noscript,template,nav,aside,footer,button,[hidden],[aria-hidden=true],.sr-only",
    )
    .remove();
    if method == "owned_article" {
        doc.select("article article").remove();
        for nested in doc.select("article [itemscope][itemtype]").iter() {
            if nested.attr("itemtype").is_some_and(|types| {
                types.split_whitespace().any(|t| {
                    matches!(
                        t.rsplit('/').next(),
                        Some("Article" | "NewsArticle" | "BlogPosting")
                    )
                })
            }) {
                nested.remove();
            }
        }
    }
    // Serialize the selected subtree once; nested p/li and table cells must not duplicate or lose text.
    let original = doc.formatted_text().to_string();
    ensure!(!original.trim().is_empty(), "extraction_failed: no text");
    let truncated = original.chars().count() > 100_000;
    let text: String = original.chars().take(100_000).collect();
    let blocks = text.split("\n\n").filter(|b| !b.trim().is_empty()).count();
    let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
    let mut warnings: Vec<&str> = if scope == "abstract" {
        vec!["abstract_only: full article text is not present in the selected landing-page scope"]
    } else {
        vec![]
    };
    if code_unavailable {
        warnings.push("declared_code_unavailable: malformed, ambiguous or incomplete payload; fallback text only");
    }
    let fulltext_url = source
        .select("meta[name=citation_fulltext_html_url]")
        .attr("content")
        .and_then(|v| {
            let base = reqwest::Url::parse(url).ok()?;
            let raw = if v.starts_with("www.") {
                format!("{}://{v}", base.scheme())
            } else {
                base.join(&v).ok()?.to_string()
            };
            net::validate_url(&raw).ok().map(|u| u.to_string())
        });
    Ok(
        json!({"title":title,"page_title":source.select("head title").text().trim(),"text":text,"text_hash":hash,"chars":text.chars().count(),"blocks":blocks,"truncated":truncated,
        "extraction_method":method,"content_scope":scope,"coverage":"not_verified","warnings":warnings,"available_fulltext_url":fulltext_url,"news":news,
        "library_text_prefix":original.chars().take(600).collect::<String>()}),
    )
}

async fn search_probe(engine: &str, query: &str, output: Option<&Path>) -> Result<Value> {
    let cache_before = cache::counters();
    let site = search::site_filter(query)?;
    let url = search::request_url(engine, query)?;
    let start = Instant::now();
    let page = net::fetch_with_ttl(url.as_str(), cache::SEARCH_TTL).await?;
    let parsed = if page.status == 200 {
        search::parse(engine, &page.html, &page.url)
            .and_then(|results| search::filter_site(site.as_ref(), results))
    } else {
        Err(anyhow::anyhow!("HTTP {}", page.status))
    };
    let mut value = match parsed {
        Ok(filtered) => {
            page.store().await;
            let status = if filtered.discarded > 0 {
                "partial"
            } else if filtered.results.is_empty() {
                "empty"
            } else {
                "ok"
            };
            let mut warnings: Vec<&str> = if filtered.discarded > 0 {
                vec!["Results outside the site filter were discarded"]
            } else {
                vec![]
            };
            if engine == "naver" {
                warnings.push("Naver title/snippet both use the last organic anchor text per URL (possibly description/date); first URL order is preserved. Not an independently bounded snippet or verified headline");
            }
            json!({"engine":engine,"query":query,"status":status,"http_status":page.status,"results":filtered.results,"discarded_results":filtered.discarded,"site_filter":site.as_ref().map(|u|u.as_str()),"warnings":warnings,"elapsed_ms":start.elapsed().as_millis(),"hops":page.hops})
        }
        Err(e) => {
            let message = e.to_string();
            let code = search::error_code(&message);
            json!({"engine":engine,"query":query,"status":"error","http_status":page.status,"results":[],"error":message,"error_code":code,"elapsed_ms":start.elapsed().as_millis(),"hops":page.hops})
        }
    };
    if cache::enabled() {
        value["cache"] = cache::diagnostics(cache_before);
    }
    if let Some(path) = output {
        save(path, &value)?;
        std::fs::write(path.with_extension("html"), page.html)?;
    }
    Ok(value)
}

async fn extract_probe(url: &str, output: Option<&Path>) -> Result<Value> {
    let cache_before = cache::counters();
    let start = Instant::now();
    let page = net::fetch(url).await?;
    ensure!(page.status == 200, "HTTP {}", page.status);
    if let Some(path) = output {
        std::fs::write(path.with_extension("html"), &page.html)?;
    }
    let source_url = page.url.clone();
    let accepted = page.clone();
    let final_url = page.url;
    let hops = page.hops;
    let mut result = tokio::task::spawn_blocking(move || {
        extract_cached(page.html, &source_url, page.cache.allowed)
    })
    .await??;
    let source_cache = accepted.store().await;
    if cache::enabled() {
        result["source_cache"] = serde_json::to_value(source_cache)?;
    }
    result["url"] = json!(url);
    result["final_url"] = json!(final_url);
    result["hops"] = json!(hops);
    result["elapsed_ms"] = json!(start.elapsed().as_millis());
    if cache::enabled() {
        result["cache"] = cache::diagnostics(cache_before);
    }
    if let Some(path) = output {
        save(path, &result)?;
    }
    Ok(result)
}

async fn omlx(settings: &Path, model: &str, output: &Path) -> Result<()> {
    let settings: Value = serde_json::from_slice(&std::fs::read(settings)?)?;
    let host = settings["server"]["host"].as_str().unwrap_or("127.0.0.1");
    ensure!(
        matches!(host, "127.0.0.1" | "localhost" | "::1"),
        "oMLX probe accepts loopback settings only"
    );
    let port = settings["server"]["port"].as_u64().unwrap_or(8000);
    ensure!((1..=65535).contains(&port), "invalid local port");
    let base = format!("http://127.0.0.1:{port}/v1");
    let key = settings["auth"]["api_key"]
        .as_str()
        .context("configured oMLX key is missing")?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(120))
        .build()?;
    let list: Value = client
        .get(format!("{base}/models"))
        .bearer_auth(key)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    ensure!(
        list["data"]
            .as_array()
            .is_some_and(|a| a.iter().any(|m| m["id"] == model)),
        "requested local model not found"
    );
    let payload = json!({"model":model,"temperature":0,"max_tokens":256,"messages":[{"role":"user","content":"Call the search tool once with query exactly rust ownership. Do not answer from memory."}],"tools":[{"type":"function","function":{"name":"search","description":"Search public HTML pages","parameters":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}}}],"tool_choice":"auto"});
    let start = Instant::now();
    let response = client
        .post(format!("{base}/chat/completions"))
        .bearer_auth(key)
        .json(&payload)
        .send()
        .await?;
    let http_status = response.status().as_u16();
    let body: Value = response.json().await?;
    let message = &body["choices"][0]["message"];
    let calls = message["tool_calls"].as_array();
    let valid = calls.is_some_and(|calls| {
        calls.len() == 1
            && calls[0]["function"]["name"] == "search"
            && calls[0]["function"]["arguments"]
                .as_str()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .is_some_and(|v| v == json!({"query":"rust ownership"}))
    });
    let result = json!({"model":model,"http_status":http_status,"elapsed_ms":start.elapsed().as_millis(),"native_tool_call_valid":valid,"tool_calls":message["tool_calls"],"content":message["content"],"usage":body["usage"],"error_type":body["error"]["type"]});
    save(output, &result)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Research {
            question,
            language,
            cache_mode,
            settings,
            openrouter_key_file,
            openrouter_proxy,
            model,
            reranker_settings,
            source_state,
            cache_db,
            capture_dir,
            output,
            live,
        } => {
            ensure!(live, "research requires --live; no requests made");
            let input = research::Input {
                question,
                language,
                cache_mode,
            };
            input.validate()?;
            if let Some(path) = cache_db {
                cache::configure(&path, false)?;
            }
            let model = crate::model::Model::configured(
                settings.as_deref(),
                openrouter_key_file.as_deref(),
                model.as_deref(),
                openrouter_proxy.as_deref(),
            )?
            .context("model provider required")?
            .with_reranker(reranker_settings.as_deref(), true)?;
            let value =
                mcp::research_once(input, std::sync::Arc::new(model), capture_dir, source_state)
                    .await?;
            if let Some(path) = output {
                save(&path, &value)?;
            }
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Command::Mcp {
            omlx_settings,
            openrouter_key_file,
            openrouter_proxy,
            model,
            evidence_memory,
            reranker_settings,
            capture_dir,
            source_state,
            cache_db,
            cache_refresh,
        } => {
            if let Some(path) = cache_db {
                cache::configure(&path, cache_refresh)?;
            }
            let local = crate::model::Model::configured(
                omlx_settings.as_deref(),
                openrouter_key_file.as_deref(),
                model.as_deref(),
                openrouter_proxy.as_deref(),
            )?
            .map(|m| m.with_reranker(reranker_settings.as_deref(), evidence_memory))
            .transpose()?
            .map(std::sync::Arc::new);
            ensure!(
                reranker_settings.is_none() || local.is_some(),
                "reranker requires a classifier provider"
            );
            mcp::serve(local, evidence_memory, capture_dir, source_state).await?;
        }
        Command::CacheClear {
            cache_db,
            url,
            all: _,
        } => {
            let url = url
                .map(|raw| {
                    net::validate_url(&raw).map(|mut u| {
                        u.set_fragment(None);
                        u.to_string()
                    })
                })
                .transpose()?;
            let removed = cache::Cache::open(&cache_db)?.clear(url.as_deref())?;
            println!("{}", json!({"removed":removed}));
        }
        Command::RankedSuite {
            settings,
            openrouter_key_file,
            openrouter_proxy,
            model,
            evidence_memory,
            reranker_settings,
            output,
            live,
        } => {
            ensure!(live, "ranked-suite requires --live; no requests made");
            policy::suite(
                &crate::model::Model::configured(
                    settings.as_deref(),
                    openrouter_key_file.as_deref(),
                    model.as_deref(),
                    openrouter_proxy.as_deref(),
                )?
                .context("model provider required")?
                .with_reranker(reranker_settings.as_deref(), evidence_memory)?,
                &output,
                evidence_memory,
            )
            .await?;
        }
        Command::RankedSearch {
            query,
            settings,
            openrouter_key_file,
            openrouter_proxy,
            model,
            evidence_memory,
            reranker_settings,
            output,
            live,
        } => {
            let case = policy::case_for(&query)?;
            ensure!(live, "ranked-search requires --live; no requests made");
            std::fs::create_dir(&output)?;
            let model = crate::model::Model::configured(
                settings.as_deref(),
                openrouter_key_file.as_deref(),
                model.as_deref(),
                openrouter_proxy.as_deref(),
            )?
            .context("model provider required")?
            .with_reranker(reranker_settings.as_deref(), evidence_memory)?;
            model.check().await?;
            let result = policy::Session::new()
                .run(
                    &case,
                    &model,
                    &mut std::collections::HashMap::new(),
                    &mut std::collections::HashMap::new(),
                    Some(&output),
                    evidence_memory,
                )
                .await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&policy::public_result(result, 10))?
            );
        }
        Command::Discover {
            url,
            query,
            max_links,
            output,
        } => println!(
            "{}",
            serde_json::to_string_pretty(
                &discover::probe(&url, &query, max_links, output.as_deref()).await?
            )?
        ),
        Command::Search {
            engine,
            query,
            output,
        } => println!(
            "{}",
            serde_json::to_string_pretty(&search_probe(&engine, &query, output.as_deref()).await?)?
        ),
        Command::Extract { url, output } => {
            let mut value = extract_probe(&url, output.as_deref()).await?;
            value.as_object_mut().unwrap().remove("text");
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Command::ExtractFile { html, url, output } => {
            net::validate_url(&url)?;
            let value = extract(std::fs::read_to_string(html)?, &url)?;
            save(&output, &value)?;
            println!(
                "{}",
                json!({"title":value["title"],"library_text_prefix":value["library_text_prefix"]})
            );
        }
        Command::SearchSuite { output } => {
            let cases: Vec<Value> =
                serde_json::from_str(include_str!("../feasibility/search-cases.json"))?;
            let mut results = Vec::new();
            let mut blocked = std::collections::HashSet::new();
            for case in cases {
                for engine in ["ddg", "bing"] {
                    if blocked.contains(engine) {
                        continue;
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    let path =
                        output.join(format!("{}-{engine}.json", case["id"].as_str().unwrap()));
                    let result = match search_probe(
                        engine,
                        case["query"].as_str().unwrap(),
                        Some(&path),
                    )
                    .await
                    {
                        Ok(v) => v,
                        Err(e) => {
                            let message = e.to_string();
                            json!({"engine":engine,"case":case["id"],"status":"error","error":message,"error_code":search::error_code(&message)})
                        }
                    };
                    if result["error_code"]
                        .as_str()
                        .is_some_and(search::blocks_source)
                    {
                        blocked.insert(engine);
                    }
                    println!(
                        "{} {engine}: {} ({} results)",
                        case["id"],
                        result["status"],
                        result["results"].as_array().map_or(0, Vec::len)
                    );
                    results.push(result);
                    save(&output.join("summary.json"), &json!(results))?;
                }
            }
        }
        Command::ExtractSuite { output } => {
            let cases: Vec<Value> =
                serde_json::from_str(include_str!("../feasibility/extract-cases.json"))?;
            let mut results = Vec::new();
            for case in cases {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let path = output.join(format!("{}.json", case["id"].as_str().unwrap()));
                let result = match extract_probe(case["url"].as_str().unwrap(), Some(&path)).await {
                    Ok(mut v) => {
                        v["marker_present"] =
                            json!(case["markers"].as_array().unwrap().iter().all(|m| {
                                v["text"]
                                    .as_str()
                                    .unwrap()
                                    .to_lowercase()
                                    .contains(&m.as_str().unwrap().to_lowercase())
                            }));
                        v.as_object_mut().unwrap().remove("text");
                        v
                    }
                    Err(e) => json!({"url":case["url"],"error":e.to_string()}),
                };
                println!(
                    "{} marker={} error={}",
                    case["id"], result["marker_present"], result["error"]
                );
                results.push(result);
                save(&output.join("summary.json"), &json!(results))?;
            }
        }
        Command::Omlx {
            settings,
            model,
            output,
        } => omlx(&settings, &model, &output).await?,
        Command::StackCheck => {
            let db = rusqlite::Connection::open_in_memory()?;
            let sqlite: String = db.query_row("select sqlite_version()", [], |r| r.get(0))?;
            let _: axum::Router = axum::Router::new();
            let _ = utoipa::openapi::OpenApi::new(
                utoipa::openapi::Info::new("probe", "0.1"),
                utoipa::openapi::Paths::new(),
            );
            println!(
                "{}",
                json!({"sqlite":sqlite,"api_crates":"compiled; production handlers not implemented","browser":"compiled; runtime not checked"})
            );
        }
        Command::Browser { executable, output } => {
            let result = browser::probe(executable.as_deref()).await?;
            save(&output, &result)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_news_article_body_excludes_sibling_and_nested_news_teasers() {
        let html = include_str!("../tests/fixtures/extract/news-main-article-teasers.html");
        let doc = extract(
            html.into(),
            "https://nnovgorod.bezformata.com/listnews/privolzhe-glavnie/165483239/",
        )
        .unwrap();
        let text = doc["text"].as_str().unwrap();
        assert!(text.contains("в Городце"));
        for marker in [
            "NESTED_CITY_TEASER",
            "NESTED_ARTICLE_TEASER",
            "SIBLING_CITY_TEASER",
        ] {
            assert!(
                !text.contains(marker),
                "related news must not become own evidence: {marker}"
            );
        }
        assert_eq!(doc["news"]["published_date"], "2026-11-03");
        assert_eq!(doc["content_scope"], "article");
        assert_eq!(doc["extraction_method"], "owned_article");
    }

    #[tokio::test]
    #[ignore = "four public Bing requests for the documented locale hypothesis"]
    async fn live_bing_locale_research() -> Result<()> {
        let fixture: Value =
            serde_json::from_str(include_str!("../feasibility/locale-research-cases.json"))?;
        assert_eq!(fixture["cases"].as_array().unwrap().len(), 2);
        assert_eq!(fixture["max_serp_requests"], 4);
        let output = PathBuf::from("runs/research-locale-2026-10-28");
        std::fs::create_dir(&output)?;
        let mut records = Vec::new();
        let mut blocked = false;
        for case in fixture["cases"].as_array().unwrap() {
            for variant in ["q_only", "q_and_setlang"] {
                if blocked {
                    break;
                }
                let mut url = reqwest::Url::parse("https://www.bing.com/search")?;
                url.query_pairs_mut()
                    .append_pair("q", case["query"].as_str().unwrap());
                if variant == "q_and_setlang" {
                    url.query_pairs_mut()
                        .append_pair("setlang", case["setlang"].as_str().unwrap());
                }
                let value = match net::fetch(url.as_str()).await {
                    Ok(page) => {
                        std::fs::write(
                            output.join(format!("{}-{variant}.html", case["id"].as_str().unwrap())),
                            &page.html,
                        )?;
                        let parsed = if page.status == 200 {
                            search::parse("bing", &page.html, &page.url)
                        } else {
                            Err(anyhow::anyhow!("HTTP {}", page.status))
                        };
                        match parsed {
                            Ok(results) => {
                                json!({"case":case,"variant":variant,"http_status":page.status,"hops":page.hops,"results":results})
                            }
                            Err(error) => {
                                json!({"case":case,"variant":variant,"http_status":page.status,"error":error.to_string(),"error_code":search::error_code(&error.to_string())})
                            }
                        }
                    }
                    Err(error) => {
                        json!({"case":case,"variant":variant,"error":error.to_string(),"error_code":search::error_code(&error.to_string())})
                    }
                };
                blocked = value["error_code"]
                    .as_str()
                    .is_some_and(search::blocks_source);
                println!(
                    "{} {variant}: {} results; error={}",
                    case["id"],
                    value["results"].as_array().map_or(0, Vec::len),
                    value["error"]
                );
                records.push(value);
                save(
                    &output.join("results.json"),
                    &json!({"fixture":fixture,"records":records,"interpretation":"development probe, not acceptance; raw unfiltered SERPs"}),
                )?;
            }
        }
        assert!(records.len() <= 4);
        Ok(())
    }

    #[tokio::test]
    #[ignore = "explicit public HTML diagnostics; no retries"]
    async fn live_reliability_diagnostics() -> Result<()> {
        let output = PathBuf::from("runs/reliability-2026-10-28");
        std::fs::create_dir_all(&output)?;
        let mut report = Vec::new();
        for (id, base, parameter, query) in [
            (
                "bing-site",
                "https://www.bing.com/search",
                "q",
                "site:doc.rust-lang.org ownership",
            ),
            (
                "bing-plain",
                "https://www.bing.com/search",
                "q",
                "Rust ownership borrowing documentation",
            ),
            (
                "mwmbl",
                "https://mwmbl.org/",
                "q",
                "Rust ownership borrowing documentation",
            ),
            (
                "marginalia",
                "https://search.marginalia.nu/search",
                "query",
                "Rust ownership borrowing documentation",
            ),
            (
                "wiby",
                "https://wiby.me/",
                "q",
                "Rust ownership borrowing documentation",
            ),
        ] {
            let mut url = reqwest::Url::parse(base)?;
            url.query_pairs_mut().append_pair(parameter, query);
            let result = match net::fetch(url.as_str()).await {
                Ok(page) => {
                    std::fs::write(output.join(format!("{id}.html")), &page.html)?;
                    let doc = dom_query::Document::from(page.html);
                    json!({"id":id,"query":query,"status":page.status,"final_url":page.url,"hops":page.hops,"title":doc.select("title").text().to_string(),"echoed_query":doc.select("input[name=q],input[name=query]").attr("value").map(|x|x.to_string()),"body_prefix":doc.select("body").text().chars().take(1400).collect::<String>()})
                }
                Err(error) => json!({"id":id,"query":query,"error":error.to_string()}),
            };
            println!("{}", serde_json::to_string(&result)?);
            report.push(result);
            save(&output.join("diagnostics.json"), &json!(report))?;
        }
        Ok(())
    }

    #[test]
    #[ignore = "replays local raw HTML saved by explicit diagnostics"]
    fn replay_reliability_snapshots() -> Result<()> {
        let output = PathBuf::from("runs/reliability-2026-10-28");
        let mwmbl = search::parse(
            "mwmbl",
            &std::fs::read_to_string(output.join("mwmbl.html"))?,
            "https://mwmbl.org/search",
        )?;
        assert_eq!(mwmbl.len(), 3);
        assert_eq!(mwmbl[0].url, "https://webreference.com/rust/ownership/");
        let bing = search::parse(
            "bing",
            &std::fs::read_to_string(output.join("bing-site.html"))?,
            "https://www.bing.com/search",
        )?;
        let count = bing.len();
        let site = search::site_filter("site:doc.rust-lang.org ownership")?;
        let mismatch = search::filter_site(site.as_ref(), bing)
            .unwrap_err()
            .to_string();
        assert_eq!(search::error_code(&mismatch), "source_query_mismatch");
        let challenge = search::parse(
            "bing",
            &std::fs::read_to_string(output.join("marginalia.html"))?,
            "https://marginalia-search.com",
        )
        .unwrap_err()
        .to_string();
        assert_eq!(search::error_code(&challenge), "source_challenge");
        save(
            &output.join("replay.json"),
            &json!({"mwmbl_results":mwmbl,"bing_original_results":count,"bing_guard_error":mismatch,"marginalia_guard_error":challenge}),
        )?;
        Ok(())
    }
    // Experiment only: no product caller uses this question-aware selection.
    pub(super) fn section_experiment(html: &str, url: &str, query: &str) -> Result<Value> {
        let baseline = extract(html.into(), url)?;
        if baseline["extraction_method"] != "declared_rfc_body" {
            return Ok(baseline);
        }
        let source = dom_query::Document::from(html);
        source
            .select("nav,aside,footer,[hidden],[aria-hidden=true]")
            .remove();
        let terms: std::collections::HashSet<_> = query
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() > 2)
            .map(str::to_owned)
            .collect();
        let mut candidates = Vec::new();
        for node in source.select("section[id^='section-']").iter() {
            let headings = node.select("h2, h3, h4, h5, h6");
            if headings.length() != 1 {
                continue;
            }
            let heading = headings.text().to_lowercase();
            let score = heading
                .split(|c: char| !c.is_alphanumeric())
                .filter(|t| t.len() > 2 && terms.contains(*t))
                .count();
            if score > 0 {
                candidates.push((
                    score,
                    node.attr("id").unwrap().to_string(),
                    node.html().to_string(),
                ));
            }
        }
        candidates.sort_by_key(|c| std::cmp::Reverse(c.0));
        let Some(best) = candidates.first() else {
            return Ok(baseline);
        };
        if candidates.get(1).is_some_and(|next| next.0 == best.0) {
            return Ok(baseline);
        }
        let selected_html = format!(
            "<html><head>{}</head><body>{}{}{}</body></html>",
            source.select("head").html(),
            source.select("h1#rfcnum").html(),
            source.select("h1#title").html(),
            best.2
        );
        let mut selected = extract(selected_html, url)?;
        selected["content_scope"] = json!("experimental_selected_rfc_section");
        selected["selected_section"] = json!(best.1);
        selected["offset_scope"] =
            json!("Unicode positions in selected extracted text, not full source body");
        Ok(selected)
    }

    pub(super) fn section_closure_experiment(
        html: &str,
        url: &str,
        section_id: &str,
        follow: bool,
    ) -> Result<Value> {
        let full = extract(html.into(), url)?;
        if full["extraction_method"] != "declared_rfc_body" {
            return Ok(full);
        }
        let doc = dom_query::Document::from(html);
        doc.select("nav,aside,footer,[hidden],[aria-hidden=true]")
            .remove();
        let matches: Vec<_> = doc
            .select("section[id]")
            .iter()
            .filter(|n| n.attr("id").as_deref() == Some(section_id))
            .collect();
        if matches.len() != 1
            || !section_id.starts_with("section-")
            || matches[0].select("h2,h3,h4,h5,h6").length() != 1
        {
            return Ok(full);
        }
        let target = &matches[0];
        let mut selected = vec![(section_id.to_owned(), target.html().to_string())];
        let mut unresolved = Vec::new();
        if follow {
            for anchor in target.select("a[href]").iter() {
                let href = anchor.attr("href").unwrap().to_string();
                let Some(id) = href.strip_prefix('#') else {
                    unresolved.push(href);
                    continue;
                };
                let nodes: Vec<_> = doc
                    .select("[id]")
                    .iter()
                    .filter(|n| n.attr("id").as_deref() == Some(id))
                    .collect();
                if nodes.len() != 1 {
                    unresolved.push(href);
                    continue;
                }
                let node = nodes[0].nodes()[0];
                let section = if node.has_name("section") {
                    Some(node)
                } else {
                    let nested = dom_query::Selection::from(node).select("section[id]");
                    if nested.length() == 1 {
                        Some(nested.nodes()[0])
                    } else if nested.length() > 1 {
                        None
                    } else {
                        node.ancestors_it(None).find(|n| n.has_name("section"))
                    }
                };
                let Some(section) = section else {
                    unresolved.push(href);
                    continue;
                };
                let Some(id) = section.attr("id") else {
                    unresolved.push(href);
                    continue;
                };
                if selected.iter().any(|s| s.0 == id.as_ref()) {
                    continue;
                }
                if !id.starts_with("section-")
                    || dom_query::Selection::from(section)
                        .select("h2,h3,h4,h5,h6")
                        .length()
                        != 1
                {
                    unresolved.push(href);
                    continue;
                }
                selected.push((id.to_string(), section.html().to_string()));
            }
        }
        let selected_html = format!(
            "<html><head>{}</head><body>{}{}{}</body></html>",
            doc.select("head").html(),
            doc.select("h1#rfcnum").html(),
            doc.select("h1#title").html(),
            selected
                .iter()
                .map(|s| s.1.as_str())
                .collect::<Vec<_>>()
                .join("")
        );
        let mut result = extract(selected_html, url)?;
        if result["truncated"] == true {
            return Ok(full);
        }
        result["content_scope"] = json!("experimental_rfc_section_closure");
        result["selected_sections"] = json!(selected.iter().map(|s| &s.0).collect::<Vec<_>>());
        result["unresolved_references"] = json!(unresolved);
        result["offset_scope"] =
            json!("Unicode in this selected extracted text, never full-body offsets");
        Ok(result)
    }

    #[test]
    fn section_closure_keeps_referenced_exception_not_only_see_link() -> Result<()> {
        for (html, url, section, needle) in [
            (
                include_str!("../tests/fixtures/extract/rfc9111-section-closure.html"),
                "https://www.rfc-editor.org/rfc/rfc9111.html",
                "section-5.2.2.5",
                "SHOULD ignore the no-store directive",
            ),
            (
                include_str!("../tests/fixtures/extract/rfc9112-section-closure.html"),
                "https://www.rfc-editor.org/rfc/rfc9112.html",
                "section-6.2",
                "A server MAY reject a request that contains both",
            ),
        ] {
            let baseline = section_closure_experiment(html, url, section, false)?;
            let candidate = section_closure_experiment(html, url, section, true)?;
            assert!(!baseline["text"].as_str().unwrap().contains(needle));
            assert!(
                candidate["text"].as_str().unwrap().contains(needle),
                "{}",
                candidate["text"]
            );
            assert_eq!(candidate["selected_sections"].as_array().unwrap().len(), 2);
            assert_ne!(candidate["text_hash"], baseline["text_hash"]);
            assert_eq!(candidate["truncated"], false);
            assert!(
                section_closure_experiment(html, url, "missing", true)?
                    .get("selected_sections")
                    .is_none()
            );
            assert!(
                section_closure_experiment(
                    html,
                    "https://example.org/rfc/rfc9111.html",
                    section,
                    true
                )?
                .get("selected_sections")
                .is_none()
            );
            let oversized = html.replace(
                "SHOULD</span> ignore",
                &format!("SHOULD</span> {} ignore", "padding ".repeat(14_000)),
            );
            if oversized.len() > html.len() {
                assert!(
                    section_closure_experiment(&oversized, url, section, true)?
                        .get("selected_sections")
                        .is_none()
                );
            }
            let toc = html.replace("<body>", &format!("<body><nav><section id='{section}'><h3>TOC decoy</h3><p>Untrusted sidebar.</p></section></nav>"));
            assert_eq!(
                section_closure_experiment(&toc, url, section, true)?["text"],
                candidate["text"]
            );
            let duplicate = html.replace(
                "</body>",
                &format!(
                    "<section id='{section}'><h3>Duplicate</h3><p>Wrong.</p></section></body>"
                ),
            );
            assert!(
                section_closure_experiment(&duplicate, url, section, true)?
                    .get("selected_sections")
                    .is_none()
            );
        }
        Ok(())
    }

    #[test]
    fn section_experiment_recovers_late_norm_without_changing_prefix_contract() {
        let fixture = include_str!("../tests/fixtures/extract/rfc9110-late-204-section.html");
        let html = fixture.replace(
            "<!-- LARGE PREFIX -->",
            &format!(
                "<section id='section-1'><h2>1. Introduction</h2><p>{}</p></section>",
                "Introduction text. ".repeat(7000)
            ),
        );
        let url = "https://www.rfc-editor.org/rfc/rfc9110.html";
        let baseline = extract(html.clone(), url).unwrap();
        assert_eq!(baseline["chars"], 100_000);
        assert!(
            !baseline["text"]
                .as_str()
                .unwrap()
                .contains("cannot contain content or trailers")
        );
        let candidate = section_experiment(&html, url, "RFC 9110 204 content trailers").unwrap();
        assert!(
            candidate["text"]
                .as_str()
                .unwrap()
                .contains("cannot contain content or trailers")
        );
        assert_eq!(candidate["selected_section"], "section-15.3.5");
        assert_eq!(candidate["truncated"], false);
        assert_ne!(candidate["text_hash"], baseline["text_hash"]);
        assert!(candidate["text"].as_str().unwrap().starts_with("RFC 9110"));
        for bad in [
            html.replace("content=\"9110\"", "content=\"9111\""),
            html.replace("<h1 id=\"rfcnum\">RFC 9110", "<h1 id=\"rfcnum\">RFC 9111"),
        ] {
            assert!(
                section_experiment(&bad, url, "204")
                    .unwrap()
                    .get("selected_section")
                    .is_none()
            );
        }
        assert!(
            section_experiment(&html, "https://example.org/rfc/rfc9110.html", "204")
                .unwrap()
                .get("selected_section")
                .is_none()
        );
        assert!(
            section_experiment(&html, url, "unmatchedword")
                .unwrap()
                .get("selected_section")
                .is_none()
        );
        let ambiguous = html.replace("</body>","<section id='section-999'><h4>999. 204 No Content</h4><p>Unrelated duplicate heading.</p></section></body>");
        assert!(
            section_experiment(&ambiguous, url, "204 content")
                .unwrap()
                .get("selected_section")
                .is_none()
        );
        let other = html.replace("9110", "9999").replace("204", "777");
        let selected = section_experiment(
            &other,
            "https://www.rfc-editor.org/rfc/rfc9999.html",
            "777 content",
        )
        .unwrap();
        assert_eq!(selected["selected_section"], "section-15.3.5");
        let no_store = include_str!("../tests/fixtures/extract/rfc9111-declared-body.html");
        assert!(
            section_experiment(
                no_store,
                "https://www.rfc-editor.org/rfc/rfc9111.html",
                "no store"
            )
            .unwrap()["text"]
                .as_str()
                .unwrap()
                .contains("immediate request")
        );
    }

    #[test]
    fn extraction_keeps_declared_rfc_body_and_requires_consistent_identity() {
        let html = include_str!("../tests/fixtures/extract/rfc9111-declared-body.html");
        let url = "https://www.rfc-editor.org/rfc/rfc9111.html";
        let value = extract(html.into(), url).unwrap();
        assert_eq!(value["extraction_method"], "declared_rfc_body");
        assert!(
            value["text"]
                .as_str()
                .unwrap()
                .contains("immediate request")
        );
        assert!(
            value["text"]
                .as_str()
                .unwrap()
                .contains("both private and shared caches")
        );
        assert_eq!(value["truncated"], false);
        for (candidate, target) in [
            (html.to_owned(), "https://example.org/rfc/rfc9111.html"),
            (
                html.to_owned(),
                "https://www.rfc-editor.org/rfc/rfc9112.html",
            ),
            (html.replace("content=\"9111\"", "content=\"9112\""), url),
            (
                html.replace("<h1 id=\"rfcnum\">RFC 9111", "<h1 id=\"rfcnum\">RFC 9112"),
                url,
            ),
            (html.replace("<title>RFC 9111:", "<title>RFC 9112:"), url),
            (html.replace("xml2rfc 3.12.10", "unknown generator"), url),
            (
                html.replace("<meta content=\"9111\" name=\"rfc.number\">", ""),
                url,
            ),
        ] {
            assert_ne!(
                extract(candidate, target).unwrap()["extraction_method"],
                "declared_rfc_body"
            );
        }
        let long = html.replace(
            "<h1 id=\"title\">",
            &format!("<p>{}</p><h1 id=\"title\">", "x".repeat(100_100)),
        );
        let limited = extract(long, url).unwrap();
        assert_eq!(limited["chars"], 100_000);
        assert_eq!(limited["truncated"], true);
    }

    #[test]
    fn extraction_semantic_text_is_not_duplicated_and_title_is_scoped() {
        let html = "<html><head><title>Document title</title></head><body><nav>OUTSIDE</nav><main><button>Copy</button><h1>Guide</h1><ul><li><p>UNIQUE_PARAGRAPH</p></li></ul><div>Direct div text</div><pre>code_line_one\ncode_line_two</pre><svg><title>Icon title</title></svg></main></body></html>";
        let value = extract(html.into(), "https://example.org/guide").unwrap();
        assert_eq!(value["title"], "Document title");
        assert_eq!(value["page_title"], "Document title");
        let text = value["text"].as_str().unwrap();
        assert_eq!(text.matches("UNIQUE_PARAGRAPH").count(), 1);
        assert!(text.contains("Direct div text") && text.contains("code_line_two"));
        assert!(!text.contains("OUTSIDE") && !text.contains("Copy"));
        assert_eq!(value["coverage"], "not_verified");
    }

    #[test]
    fn extraction_regression_prefers_scholarly_body_over_abstract() {
        let value = extract(
            include_str!("../tests/fixtures/extract/jcar-full.html").into(),
            "https://www.jcar.or.kr/articles/xml/bXDR/",
        )
        .unwrap();
        assert_eq!(value["content_scope"], "article_body");
        assert!(
            value["text"]
                .as_str()
                .unwrap()
                .contains("대부분의 건설 프로젝트")
        );
        assert!(value["chars"].as_u64().unwrap() > 5_000);
    }

    #[test]
    fn extraction_regression_keeps_documentation_sections() {
        let value = extract(
            include_str!("../tests/fixtures/extract/n8n.html").into(),
            "https://docs.n8n.io/deploy/host-n8n/configure-n8n/security/enable-ssrf-protection",
        )
        .unwrap();
        let text = value["text"].as_str().unwrap();
        for expected in [
            "N8N_SSRF_PROTECTION_ENABLED=true",
            "DNS rebinding",
            "fc00::/7",
        ] {
            assert!(
                text.contains(expected),
                "missing documentation section: {expected}"
            );
        }
    }

    #[test]
    fn extraction_regression_keeps_reference_options() {
        let value = extract(
            include_str!("../tests/fixtures/extract/git-clean.html").into(),
            "https://git-scm.com/docs/git-clean",
        )
        .unwrap();
        let text = value["text"].as_str().unwrap();
        for expected in [
            "SYNOPSIS",
            "OPTIONS",
            "--dry-run",
            "-x",
            "-X",
            "Don’t actually remove anything",
            "Don’t use the standard ignore rules",
            "Remove only files ignored by Git",
            "Interactive mode",
            "CONFIGURATION",
        ] {
            assert!(
                text.contains(expected),
                "missing reference section: {expected}"
            );
        }
        assert!(!text.contains("Changes in the git-clean manual"));
        assert!(!text.contains("Localized versions"));
        assert!(!text.contains("Patches, suggestions, and comments"));
        assert_eq!(value["content_scope"], "declared_body");
    }

    #[test]
    fn extraction_regression_keeps_declared_reference_body() {
        let value = extract(
            include_str!("../tests/fixtures/extract/java-concurrenthashmap-api.html").into(),
            "https://docs.oracle.com/javase/8/docs/api/java/util/concurrent/ConcurrentHashMap.html",
        )
        .unwrap();
        let text = value["text"].as_str().unwrap();
        let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
        for expected in [
            "The entire method invocation is performed atomically",
            "the function is applied at most once per key",
            "enters it into this map unless null",
            "or null if the computed value is null",
        ] {
            assert!(
                normalized.contains(expected),
                "missing API detail: {expected}"
            );
        }
        assert!(!text.contains("Skip navigation links") && !text.contains("All Classes"));
        assert_eq!(value["content_scope"], "api_reference");
        assert_eq!(value["coverage"], "not_verified");
        assert_eq!(value["truncated"], false);
        assert_eq!(
            value["text_hash"],
            format!("{:x}", Sha256::digest(text.as_bytes()))
        );
    }

    #[test]
    fn extraction_reference_body_requires_consistent_declarations() {
        let fixture = include_str!("../tests/fixtures/extract/java-concurrenthashmap-api.html");
        for html in [
            fixture.replace("java.util.concurrent.ConcurrentHashMap class", "other.Type class"),
            fixture.replace("title=\"Class ConcurrentHashMap\"", "title=\"Class Other\""),
            fixture.replace("class=\"description\"", "class=\"example\""),
            fixture.replace("class=\"summary\"", "class=\"example\""),
            fixture.replace("class=\"details\"", "class=\"example\""),
            fixture.replace("name=\"method.detail\"", "name=\"example.detail\""),
            fixture.replace("</body>", "<div class=\"contentContainer\">Second body</div></body>"),
            fixture.replace("</body>", "<div class=\"header\"><h2 class=\"title\" title=\"Class ConcurrentHashMap\">Second header</h2></div></body>"),
        ] {
            let value = extract(html, "https://example.org/reference").unwrap();
            assert_ne!(value["content_scope"], "api_reference");
            assert_eq!(value["coverage"], "not_verified");
        }
        let with_navigation = fixture.replacen(
            "<div class=\"description\">",
            "<div class=\"description\"><nav>REFERENCE_NAVIGATION</nav><script>REFERENCE_SCRIPT</script>",
            1,
        );
        let value = extract(with_navigation, "https://example.org/reference").unwrap();
        let text = value["text"].as_str().unwrap();
        assert_eq!(value["content_scope"], "api_reference");
        assert!(!text.contains("REFERENCE_NAVIGATION") && !text.contains("REFERENCE_SCRIPT"));
        let oversized = fixture.replacen(
            "<div class=\"contentContainer\">",
            &format!(
                "<div class=\"contentContainer\"><p>{}</p>",
                "界".repeat(100_010)
            ),
            1,
        );
        let value = extract(oversized, "https://example.org/reference").unwrap();
        let text = value["text"].as_str().unwrap();
        assert_eq!(value["content_scope"], "api_reference");
        assert_eq!(value["chars"], 100_000);
        assert_eq!(value["truncated"], true);
        assert_eq!(
            value["text_hash"],
            format!("{:x}", Sha256::digest(text.as_bytes()))
        );
    }

    #[test]
    fn extraction_regression_keeps_declared_code_blob() {
        let value = extract(
            include_str!("../tests/fixtures/extract/cpython-dataclasses-blob.html").into(),
            "https://github.com/python/cpython/blob/main/Doc/library/dataclasses.rst?plain=1",
        )
        .unwrap();
        let text = value["text"].as_str().unwrap();
        for expected in [
            ".. function:: asdict(obj, *, dict_factory=dict)",
            "dataclasses, dicts,",
            "lists, and tuples are recursed into.  Other objects are copied",
            "with :func:`copy.deepcopy`.",
            "assert asdict(c) == {'mylist': [{'x': 0, 'y': 0}, {'x': 10, 'y': 4}]}",
        ] {
            assert!(
                text.contains(expected),
                "missing declared source: {expected}"
            );
        }
        assert!(!text.contains("Repository navigation"));
        assert!(!text.contains("Skip to content"));
        assert!(!text.contains("codeViewFileTreeLayoutRoute"));
        assert_eq!(value["content_scope"], "file_source");
        assert_eq!(value["coverage"], "not_verified");
        assert_eq!(
            value["text_hash"],
            format!("{:x}", Sha256::digest(text.as_bytes()))
        );
    }

    #[test]
    fn extraction_declared_code_rejects_incomplete_or_ambiguous_payloads() {
        let url = "https://example.org/repo/blob/main/doc.rst";
        let payload = json!({"payload":{
            "codeViewLayoutRoute":{"path":"doc.rst"},
            "codeViewBlobLayoutRoute":{"path":"doc.rst","blob":{"truncated":false,"large":false,"viewable":true,"headerInfo":{"mode":"file","isGitLfs":false,"lineInfo":{"truncatedLoc":"2"}}}},
            "codeViewBlobLayoutRoute.StyledBlob":{"rawLines":["<script>source_code()</script>","x < y && text &lt;"]}
        }});
        let script = |p: &str| {
            format!(
                "<script type='application/json' data-target='react-app.embeddedData'>{p}</script>"
            )
        };
        let page = |scripts: &str| {
            format!(
                "<html><head><title>Source file</title></head><body><nav>OUTSIDE_NAVIGATION</nav><main><p>FALLBACK_WRAPPER</p></main><react-app app-name='code-view'>{scripts}</react-app></body></html>"
            )
        };
        // HTML-shaped code must remain literal text rather than being parsed and pruned as executable markup.
        let data = payload
            .to_string()
            .replace("<script>", "\\u003cscript\\u003e")
            .replace("</script>", "\\u003c/script\\u003e");
        let good = extract(page(&script(&data)), url).unwrap();
        assert!(
            good["text"]
                .as_str()
                .unwrap()
                .contains("<script>source_code()</script>")
        );
        assert!(
            good["text"]
                .as_str()
                .unwrap()
                .contains("x < y && text &lt;")
        );
        assert_eq!(good["content_scope"], "file_source");
        assert!(!good["text"].as_str().unwrap().contains("FALLBACK_WRAPPER"));
        let mut plain_payload = payload.clone();
        plain_payload["payload"]["codeViewBlobLayoutRoute.StyledBlob"]["rawLines"] =
            json!(["SOURCE_SENTINEL", "second source line"]);
        let mut invalid = vec!["{not JSON}".to_owned()];
        for (pointer, value) in [
            (
                "/payload/codeViewBlobLayoutRoute/blob/truncated",
                json!(true),
            ),
            ("/payload/codeViewBlobLayoutRoute/blob/large", json!(true)),
            (
                "/payload/codeViewBlobLayoutRoute/blob/viewable",
                json!(false),
            ),
            (
                "/payload/codeViewBlobLayoutRoute/blob/headerInfo/isGitLfs",
                json!(true),
            ),
            (
                "/payload/codeViewBlobLayoutRoute/blob/headerInfo/lineInfo/truncatedLoc",
                json!("3"),
            ),
            (
                "/payload/codeViewBlobLayoutRoute.StyledBlob/rawLines",
                json!(["SOURCE_SENTINEL", 7]),
            ),
            ("/payload/codeViewBlobLayoutRoute/path", json!("other.rst")),
            ("/payload/codeViewLayoutRoute/path", json!("other.rst")),
        ] {
            let mut v = plain_payload.clone();
            *v.pointer_mut(pointer).unwrap() = value;
            invalid.push(v.to_string());
        }
        let mut pages = invalid.iter().map(|v| page(&script(v))).collect::<Vec<_>>();
        pages.push(page(&format!("{}{}", script(&data), script(&data))));
        for html in pages {
            let value = extract(html, url).unwrap();
            assert_eq!(value["content_scope"], "main");
            assert!(value["text"].as_str().unwrap().contains("FALLBACK_WRAPPER"));
            assert!(!value["text"].as_str().unwrap().contains("source_code()"));
            assert_eq!(value["coverage"], "not_verified");
            assert!(
                value["warnings"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|w| w.as_str().unwrap().starts_with("declared_code_unavailable"))
            );
        }
        let no_code=extract("<html><main>Article contents</main><react-app app-name='issues'><script type='application/json' data-target='react-app.embeddedData'>{}</script></react-app></html>".into(),url).unwrap();
        assert!(no_code["warnings"].as_array().unwrap().is_empty());
        let mut long_source = plain_payload;
        long_source["payload"]["codeViewBlobLayoutRoute.StyledBlob"]["rawLines"] =
            json!(["界".repeat(100_010)]);
        long_source["payload"]["codeViewBlobLayoutRoute"]["blob"]["headerInfo"]["lineInfo"]["truncatedLoc"] =
            json!("1");
        let capped = extract(page(&script(&long_source.to_string())), url).unwrap();
        assert_eq!(capped["chars"], 100_000);
        assert_eq!(capped["truncated"], true);
        assert_eq!(
            capped["text_hash"],
            format!(
                "{:x}",
                Sha256::digest(capped["text"].as_str().unwrap().as_bytes())
            )
        );
    }

    #[test]
    fn extraction_regression_keeps_abstract_instead_of_only_references() {
        let value = extract(
            include_str!("../tests/fixtures/extract/jcar.html").into(),
            "https://www.jcar.or.kr/articles/article/bXDR/",
        )
        .unwrap();
        assert!(
            value["text"]
                .as_str()
                .unwrap()
                .contains("Efficient planning of earthworks")
        );
    }

    #[test]
    fn extraction_preserves_main_text_and_excludes_navigation() {
        let paragraph="Основной материал о работе экскаватора: техника перемещает грунт и выполняет рабочий цикл. ".repeat(15);
        let html = format!(
            "<html><head><title>Проверка извлечения</title></head><body><nav>SECRET_NAVIGATION_MARKER</nav><article><h1>Рабочий цикл</h1><p>{paragraph}</p><p>{paragraph}</p></article></body></html>"
        );
        let value = extract(html, "https://example.com/article").unwrap();
        assert!(value["text"].as_str().unwrap().contains("экскаватора"));
        assert!(
            !value["text"]
                .as_str()
                .unwrap()
                .contains("SECRET_NAVIGATION_MARKER")
        );
    }
}

#[cfg(test)]
mod section_experiment_capture {
    use super::*;

    #[test]
    #[ignore = "explicit offline captured F02 HTML experiment; no HTTP or models"]
    fn captured_f02_section_before_cap() -> Result<()> {
        let input = std::env::var("SECTION_EXPERIMENT_HTML")?;
        let output = std::path::PathBuf::from(std::env::var("SECTION_EXPERIMENT_OUTPUT")?);
        std::fs::create_dir(&output)?;
        let html = std::fs::read_to_string(input)?;
        let url = "https://www.rfc-editor.org/rfc/rfc9110.html";
        let baseline = extract(html.clone(), url)?;
        let candidate =
            crate::tests::section_experiment(&html, url, "RFC 9110 204 content trailers")?;
        save(&output.join("baseline.json"), &baseline)?;
        save(&output.join("candidate.json"), &candidate)?;
        assert_eq!(baseline["chars"], 100_000);
        assert!(
            !baseline["text"]
                .as_str()
                .unwrap()
                .contains("A 204 response is terminated")
        );
        assert!(
            candidate["text"]
                .as_str()
                .unwrap()
                .contains("A 204 response is terminated")
        );
        assert!(
            candidate["text"]
                .as_str()
                .unwrap()
                .contains("cannot contain content or trailers")
        );
        Ok(())
    }
}

#[cfg(test)]
mod section_exception_capture {
    use super::*;
    #[test]
    #[ignore = "explicit two captured RFC same-HTML extraction arms; no HTTP/models"]
    fn captured_section_exception_pairs() -> Result<()> {
        let manifest: Value = serde_json::from_str(&std::fs::read_to_string(std::env::var(
            "SECTION_EXCEPTION_MANIFEST",
        )?)?)?;
        let out = std::path::PathBuf::from(std::env::var("SECTION_EXCEPTION_OUTPUT")?);
        std::fs::create_dir(&out)?;
        for case in manifest.as_array().unwrap() {
            let html = std::fs::read_to_string(case["path"].as_str().unwrap())?;
            ensure!(
                cache::hash(html.as_bytes()) == case["html_sha256"],
                "frozen HTML mismatch"
            );
            let url = case["url"].as_str().unwrap();
            let section = case["needed_sections"][0].as_str().unwrap();
            for (arm, follow) in [("baseline", false), ("candidate", true)] {
                let extracted =
                    crate::tests::section_closure_experiment(&html, url, section, follow)?;
                save(
                    &out.join(format!("{}-{arm}.json", case["id"].as_str().unwrap())),
                    &json!({"id":case["id"],"url":url,"title":extracted["title"],"question":case["question"],"html_sha256":case["html_sha256"],"extracted":extracted}),
                )?;
                if follow {
                    assert!(
                        case["needed_sections"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .all(|id| extracted["selected_sections"]
                                .as_array()
                                .unwrap()
                                .contains(id))
                    );
                } else {
                    assert_eq!(extracted["selected_sections"].as_array().unwrap().len(), 1);
                }
            }
        }
        Ok(())
    }
}
