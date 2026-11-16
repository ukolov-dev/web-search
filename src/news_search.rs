//! Bounded news discovery shares the existing four-read budget with article verification.
use crate::{
    net,
    policy::{Candidate, Case},
    rescue, search,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    future::Future,
    path::Path,
    time::{Duration, Instant},
};

pub struct Prepared {
    pub candidates: Vec<Candidate>,
    pub documents: Vec<Value>,
    pub cached: HashMap<String, Value>,
    pub trace: Value,
    pub reads: usize,
}

impl Prepared {
    pub fn evidence_candidates(&self) -> Vec<Candidate> {
        self.candidates
            .iter()
            .filter(|c| {
                self.cached.contains_key(&c.id)
                    && self
                        .documents
                        .iter()
                        .any(|d| d["id"] == c.id && d["news"]["date_check"]["eligible"] == true)
            })
            .cloned()
            .collect()
    }

    pub fn read_cached(&self, candidate: &Candidate, query: &str) -> (Value, Option<rescue::Page>) {
        let record = self.documents.iter().find(|d|d["id"] == candidate.id).cloned().unwrap_or_else(||json!({"id":candidate.id,"url":candidate.url,"error":"news article not prefetched"}));
        let page = self.cached.get(&candidate.id).and_then(|doc| {
            rescue::page_from_document(
                candidate,
                record["final_url"].as_str().unwrap_or(&candidate.url),
                doc,
                query,
            )
            .ok()
        });
        (record, page)
    }

    pub fn enforce(
        &self,
        classification: &mut crate::policy::Classifications,
        report: &mut rescue::Report,
    ) {
        for record in &self.documents {
            if let Some(error) = record["error"].as_str() {
                report.errors.push(format!(
                    "{}: {error}",
                    record["id"].as_str().unwrap_or("news")
                ));
            }
        }
        for c in &self.candidates {
            let allowed = self
                .documents
                .iter()
                .find(|d| d["id"] == c.id)
                .is_some_and(|d| {
                    d["news"]["date_check"]["eligible"] == true && d["error"].is_null()
                });
            if !allowed {
                if classification.kinds.get(&c.id) == Some(&0) {
                    classification.kinds.insert(c.id.clone(), 1);
                    classification.evidence.remove(&c.id);
                }
                report.proofs.remove(&c.id);
                report.rejected.insert(
                    c.id.clone(),
                    "news: a read article with an eligible publication date is required".into(),
                );
            }
        }
        report.reads = self.reads;
        report.documents = self.documents.clone();
    }
}

pub async fn prepare(
    case: &Case,
    candidates: &[Candidate],
    rank: &HashMap<String, usize>,
    blocked: &mut HashMap<String, String>,
    started: Instant,
    output: Option<&Path>,
) -> Prepared {
    prepare_with_fetch(
        case,
        candidates,
        rank,
        blocked,
        started,
        output,
        |url, blocks, parent| async move {
            let mut visited = Vec::new();
            #[cfg(test)]
            if let Some(cache) = std::env::var_os("WEB_SEARCH_RAW_CACHE") {
                let result = crate::reliability_benchmark::cached_fetch(&url, Path::new(&cache))
                    .await
                    .map(|v| v.0);
                return (result, visited);
            }
            let result = net::fetch_guarded_with_ttl(&url, crate::cache::NEWS_TTL, |target| {
                check_hop(target, &blocks, parent.as_deref())?;
                visited.push(target.host_str().unwrap_or("").to_owned());
                Ok(())
            })
            .await;
            (result, visited)
        },
    )
    .await
}

async fn prepare_with_fetch<F, Fut>(
    case: &Case,
    candidates: &[Candidate],
    rank: &HashMap<String, usize>,
    blocked: &mut HashMap<String, String>,
    started: Instant,
    output: Option<&Path>,
    mut fetch: F,
) -> Prepared
where
    F: FnMut(String, HashMap<String, String>, Option<String>) -> Fut,
    Fut: Future<Output = (Result<net::Page>, Vec<String>)>,
{
    let mut window = crate::news::window(&case.query, &crate::news::current_date());
    window["as_of_timestamp"] = json!(crate::news::current_timestamp());
    let mut state = Prepared {
        candidates: candidates.to_vec(),
        documents: vec![],
        cached: HashMap::new(),
        trace: json!({"enabled":true,"window":window,"links":[],"read_limit":4,"same_origin":true}),
        reads: 0,
    };
    let site = match search::site_filter(&case.query) {
        Ok(v) => v,
        Err(_) => return state,
    };
    let mut roots = candidates.to_vec();
    roots.sort_by_key(|c| {
        let index = crate::policy::canonical(&c.url, true)
            .ok()
            .and_then(|u| rank.get(&u).copied())
            .unwrap_or(usize::MAX);
        let feed_path = reqwest::Url::parse(&c.url).is_ok_and(|u| {
            u.path().trim_matches('/').is_empty()
                || matches!(u.path().trim_matches('/'), "news" | "novosti")
        });
        (!feed_path, index)
    });
    let mut queue: VecDeque<_> = roots.into_iter().map(|c| (c, None::<String>)).collect();
    let mut seen = HashSet::new();
    let mut opened_feed = false;
    while let Some((mut candidate, parent)) = queue.pop_front() {
        if state.reads >= 4 || started.elapsed() >= Duration::from_secs(180) {
            break;
        }
        let Ok(url) = net::validate_url(&candidate.url) else {
            continue;
        };
        let host = url.host_str().unwrap().to_owned();
        let key = crate::policy::canonical(&candidate.url, false)
            .unwrap_or_else(|_| candidate.url.clone());
        if !seen.insert(key) {
            continue;
        }
        let card = search::SearchResult {
            title: candidate.title.clone(),
            snippet: candidate.snippet.clone(),
            url: candidate.url.clone(),
            position: 1,
            result_source: None,
        };
        if search::filter_site(site.as_ref(), vec![card]).is_err()
            || blocked.contains_key(&host)
            || crate::evidence::news_app_candidate(
                &case.query,
                &candidate.title,
                &candidate.snippet,
                &candidate.url,
            )
        {
            continue;
        }
        state.reads += 1;
        let mut record = json!({"id":candidate.id,"url":candidate.url,"parent_url":parent});
        let read: Result<()> = async {
            let remaining = Duration::from_secs(180).saturating_sub(started.elapsed());
            let (fetched, visited) = tokio::time::timeout(remaining, fetch(candidate.url.clone(), blocked.clone(), parent.clone())).await.context("deadline_exceeded")?;
            record["attempted_hosts"] = json!(visited);
            let page = fetched?;
            record["http_status"] = json!(page.status);
            record["final_url"] = json!(page.url);
            record["hops"] = json!(page.hops);
            record["html_hash"] = json!(format!("{:x}", Sha256::digest(page.html.as_bytes())));
            seen.insert(crate::policy::canonical(&page.url, false)?);
            match page.status {
                403 => anyhow::bail!("source_blocked: HTTP 403"),
                429 => anyhow::bail!("rate_limited: HTTP 429"),
                202 => anyhow::bail!("source_challenge: HTTP 202"),
                _ => ensure!(page.status == 200, "HTTP {}", page.status),
            }
            search::navigation_guard(&page.html)?;
            let final_url = net::validate_url(&page.url)?;
            if let Some(parent) = &parent {ensure!(net::validate_url(parent)?.origin() == final_url.origin(), "news cross-origin redirect");}
            let final_card = search::SearchResult {title:String::new(), snippet:String::new(), url:page.url.clone(), position:1, result_source:None};
            search::filter_site(site.as_ref(), vec![final_card]).context("site_redirect_mismatch")?;
            let metadata = crate::news::metadata(&page.html, &page.url);
            if crate::cache::enabled() { record["source_cache"] = serde_json::to_value(&page.cache)?; }
            let date_check = crate::news::check_date(&metadata, &window);
            record["news"] = json!({"metadata":metadata,"date_check":date_check,"parent_url":parent});
            if metadata["kind"] == "feed" && parent.is_none() && !opened_feed {
                opened_feed = true;
                let links = crate::news::links(&page.html, &page.url, &case.query, 3)?;
                for link in links.into_iter().rev() {
                    let child_url = link["url"].as_str().context("news link URL")?.to_owned();
                    if state.candidates.iter().any(|c|c.url==child_url) {
                        let existing = state.candidates.iter().find(|c|c.url==child_url).unwrap().clone();
                        state.trace["links"].as_array_mut().unwrap().push(json!({"parent_url":page.url,"id":existing.id,"url":child_url,"position":link["position"],"label":link["label"]}));
                        queue.push_front((existing, Some(page.url.clone())));
                        continue;
                    }
                    let child = Candidate {id:format!("C{:03}",state.candidates.len()+1),url:child_url.clone(),title:link["title"].as_str().or_else(||link["label"].as_str()).unwrap_or("").chars().take(500).collect(),snippet:String::new(),document_http_status:None,fetch_method:"news_feed_link".into()};
                    state.trace["links"].as_array_mut().unwrap().push(json!({"parent_url":page.url,"id":child.id,"url":child_url,"position":link["position"],"label":child.title}));
                    state.candidates.push(child.clone());
                    queue.push_front((child, Some(page.url.clone())));
                }
            }
            let accepted = page.clone();
            let html = page.html;
            if let Some(out) = output {
                std::fs::write(out.join(format!("{}-{}-news.html",case.id,candidate.id)),&html)?;
            }
            let final_url = page.url;
            let extraction_url = final_url.clone();
            let cacheable = accepted.cache.allowed;
            let mut doc = tokio::task::spawn_blocking(move ||crate::extract_cached(html,&extraction_url,cacheable)).await??;
            let source_cache = accepted.store().await;
            if crate::cache::enabled() { record["source_cache"] = serde_json::to_value(source_cache)?; }
            doc["news"]["request_window"] = window.clone();
            record["text_hash"] = doc["text_hash"].clone();
            record["content_scope"] = doc["content_scope"].clone();
            record["truncated"] = doc["truncated"].clone();
            if metadata["kind"] == "article" {
                candidate.document_http_status = Some(200);
                candidate.fetch_method = "news_http_extract".into();
                candidate.title = doc["title"].as_str().filter(|s|!s.is_empty()).or_else(||doc["page_title"].as_str()).unwrap_or(&candidate.title).chars().take(500).collect();
                let excerpt = crate::policy::excerpt(doc["text"].as_str().unwrap_or(""),&case.query,1100);
                candidate.snippet = excerpt;
                if let Some(c) = state.candidates.iter_mut().find(|c|c.id==candidate.id) {*c=candidate.clone();}
                state.cached.insert(candidate.id.clone(),doc.clone());
            }
            if let Some(out) = output {
                crate::save(&out.join(format!("{}-{}-news-document.json",case.id,candidate.id)), &doc)?;
            }
            Ok(())
        }.await;
        if let Err(error) = read {
            let error = error.to_string();
            if search::blocks_source(search::error_code(&error)) {
                blocked.insert(host, error.clone());
                if let Some(final_host) = record["final_url"]
                    .as_str()
                    .and_then(|s| reqwest::Url::parse(s).ok())
                    .and_then(|u| u.host_str().map(str::to_owned))
                {
                    blocked.insert(final_host, error.clone());
                }
                for host in record["attempted_hosts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    blocked.insert(host.to_owned(), error.clone());
                }
            }
            record["error"] = json!(error);
        }
        state.documents.push(record);
    }
    state.trace["network_reads"] = json!(state.reads);
    state.trace["article_pages"] = json!(state.cached.len());
    state.trace["documents"] = json!(state.documents);
    state
}

fn check_hop(
    target: &reqwest::Url,
    blocked: &HashMap<String, String>,
    parent: Option<&str>,
) -> Result<()> {
    if let Some(error) = target.host_str().and_then(|h| blocked.get(h)) {
        anyhow::bail!("host_disabled: {error}");
    }
    if let Some(parent) = parent {
        ensure!(
            net::validate_url(parent)?.origin() == target.origin(),
            "news cross-origin redirect"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    #[ignore = "explicit cached old2 plus optional saved negative, local Qwen and max2 OpenRouter generations; no search or target HTTP"]
    async fn old2_cached_date_qwen_coverage_audit_replay() -> Result<()> {
        let workspace = std::path::PathBuf::from(
            std::env::var("WEB_SEARCH_NEWS_DATE_REPLAY_WORKSPACE")
                .unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").into()),
        );
        let output = std::path::PathBuf::from(std::env::var("WEB_SEARCH_NEWS_DATE_REPLAY_ROOT")?);
        std::fs::create_dir(&output)?;
        let input_path = std::env::var_os("WEB_SEARCH_NEWS_DATE_REPLAY_INPUT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| workspace.join("runs/news-dates-2026-11-03/old2-inputs.json"));
        let input_bytes = std::fs::read(input_path)?;
        let input: Value = serde_json::from_slice(&input_bytes)?;
        ensure!(
            input["schema"] == "news-dates-old2-source-input-v1",
            "old2 schema"
        );
        let case = Case {
            id: input["case_id"].as_str().context("old2 case")?.into(),
            query: input["query"].as_str().context("old2 query")?.into(),
            forms: vec![],
        };
        ensure!(
            matches!(
                case.query.as_str(),
                "новости Нижнего Новгорода" | "последние новости Челябинска"
            ),
            "bounded saved development query"
        );
        let window = input["window"].clone();
        ensure!(
            window["status"] == "valid" && window["as_of"] == "2026-11-03",
            "trusted old clock"
        );
        let mut snapshots = HashMap::new();
        let mut candidates = Vec::new();
        for row in input["sources"].as_array().context("old2 sources")? {
            let url = row["url"].as_str().context("source URL")?;
            let html = std::fs::read_to_string(
                workspace.join(row["html_path"].as_str().context("HTML path")?),
            )?;
            ensure!(
                format!("{:x}", Sha256::digest(html.as_bytes())) == row["html_sha256"],
                "preserved HTML hash"
            );
            let final_url = row["final_url"].as_str().context("final URL")?;
            ensure!(
                url == final_url
                    && matches!(
                        net::validate_url(url)?.host_str(),
                        Some("nnovgorod.bezformata.com" | "74.ru")
                    ),
                "old2 URL binding"
            );
            ensure!(row["http_status"] == 200, "preserved HTTP status");
            let hops: Vec<String> = serde_json::from_value(row["hops"].clone())?;
            ensure!(hops == [url], "old2 no-redirect snapshot");
            snapshots.insert(
                url.to_owned(),
                net::Page {
                    url: final_url.into(),
                    status: 200,
                    html,
                    hops,
                    cache: Default::default(),
                },
            );
            candidates.push(Candidate {
                id: row["id"].as_str().context("source ID")?.into(),
                url: url.into(),
                title: row["title"].as_str().context("source title")?.into(),
                snippet: String::new(),
                document_http_status: None,
                fetch_method: "saved-original-stage-input".into(),
            });
        }
        ensure!(
            (2..=3).contains(&snapshots.len()) && candidates.len() == snapshots.len(),
            "bounded saved development sources"
        );
        let rank: HashMap<String, usize> = candidates
            .iter()
            .enumerate()
            .map(|(i, c)| Ok((crate::policy::canonical(&c.url, true)?, i)))
            .collect::<Result<_>>()?;
        let started = Instant::now();
        let mut state = prepare_with_fetch(
            &case,
            &candidates,
            &rank,
            &mut HashMap::new(),
            started,
            Some(&output),
            move |url, _, _| {
                let result = snapshots
                    .get(&url)
                    .map(|p| net::Page {
                        url: p.url.clone(),
                        status: p.status,
                        html: p.html.clone(),
                        hops: p.hops.clone(),
                        cache: Default::default(),
                    })
                    .context("uncaptured URL: target network forbidden");
                async move { (result, vec![]) }
            },
        )
        .await;
        // Only the test clock is restored; metadata, extraction and admission stay production code.
        state.trace["window"] = window.clone();
        for record in &mut state.documents {
            record["news"]["date_check"] =
                crate::news::check_date(&record["news"]["metadata"], &window);
        }
        for doc in state.cached.values_mut() {
            doc["news"]["request_window"] = window.clone();
        }
        let articles = state.evidence_candidates();
        crate::save(
            &output.join("admission.json"),
            &json!({"input_sha256":format!("{:x}",Sha256::digest(&input_bytes)),"query":case.query,"trusted_request_window":window,"documents":state.documents,"admitted":articles,"fresh_target_http_requests":0,"search_calls":0,"replayed_document_reads":state.reads}),
        )?;
        let mut labels = crate::policy::Classifications::default();
        labels
            .kinds
            .extend(state.candidates.iter().map(|c| (c.id.clone(), 1)));
        let mut report = rescue::Report::default();
        if !articles.is_empty() {
            let model = crate::model::Model::from_openrouter(
                Path::new(&std::env::var("WEB_SEARCH_OPENROUTER_KEY_FILE")?),
                "deepseek/deepseek-v4.1-flash-1017",
                std::env::var("WEB_SEARCH_OPENROUTER_PROXY").ok().as_deref(),
            )?
            .with_reranker(
                Some(Path::new(&std::env::var("WEB_SEARCH_OMLX_SETTINGS")?)),
                true,
            )?;
            model.check().await?;
            model
                .reranker
                .as_ref()
                .context("local Qwen required")?
                .check()
                .await?;
            report = crate::evidence::run_with_reader(
                rescue::Input {
                    case: &case,
                    candidates: &articles,
                    rank: &rank,
                    started,
                    output: Some(&output),
                },
                &mut labels,
                &model,
                &mut HashMap::new(),
                2,
                |c| {
                    let result = state.read_cached(&c, &case.query);
                    async move { result }
                },
            )
            .await;
        }
        state.enforce(&mut labels, &mut report);
        crate::save(
            &output.join("result.json"),
            &json!({"scope":"cached source admission and evidence-stage diagnostic; not full search or historical model replay","input_sha256":format!("{:x}",Sha256::digest(&input_bytes)),"evidence_version":crate::evidence::VERSION,"query":case.query,"trusted_request_window":window,"initial_classifier_bypass":true,"initial_labels":"ambiguous only; no synthetic positive evidence","fresh_target_http_requests":0,"search_calls":0,"admitted_count":articles.len(),"replayed_document_reads":state.reads,"final_labels":labels.kinds,"proofs":report.proofs,"rejected":report.rejected,"errors":report.errors,"memory":report.memory,"model_calls":report.calls,"reranker_calls":report.reranker_calls,"elapsed_ms":started.elapsed().as_millis()}),
        )?;
        ensure!(
            report.calls.len() <= 2
                && report.reranker_calls.len() <= candidates.len()
                && state.reads == candidates.len(),
            "old2 bounded stage calls"
        );
        ensure!(
            report.reranker_calls.len() == articles.len()
                && report.reranker_calls.iter().all(|c| c["status"] == "ok"),
            "every admitted source needs real successful local Qwen; see saved result"
        );
        Ok(())
    }

    fn card(url: &str) -> Candidate {
        Candidate {
            id: "C001".into(),
            url: url.into(),
            title: "Новости города".into(),
            snippet: "Свежая городская лента".into(),
            document_http_status: None,
            fetch_method: "unknown".into(),
        }
    }

    #[tokio::test]
    async fn feed_and_articles_share_four_reads_and_cached_body_provenance() {
        let today = crate::news::current_date();
        let expected_date = today.clone();
        let case = Case {
            id: "test".into(),
            query: "новости города сегодня".into(),
            forms: vec![],
        };
        let mut root = card("https://example.org/");
        root.id = "C002".into();
        let channel = card("https://video.example/channel/collection-news/");
        let mut existing = card("https://example.org/news/city-event-one/");
        existing.id = "C003".into();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let seen = calls.clone();
        let state = prepare_with_fetch(&case,&[channel,root,existing],&HashMap::new(),&mut HashMap::new(),Instant::now(),None,move |url,_,_| {
            seen.lock().unwrap().push(url.clone());
            let html = if url == "https://example.org/" {
                r#"<h1>Новости города</h1><a href="http://127.0.0.1/private">городская новость недопустима</a><a href="https://other.example/news/foreign-event/">городская новость с другого origin</a><a href="/pravila-ispolzovaniya">Длинные правила использования новостей города</a><a href="/news/city-event-one/">Первое конкретное городское событие сегодня</a><a href="/news/city-event-two/">Второе конкретное городское событие сегодня</a><a href="/news/city-event-three/">Третье конкретное городское событие сегодня</a><a href="/news/city-event-four/">Четвёртое городское событие сверх бюджета</a>"#.into()
            } else {
                format!(r#"<head><title>Конкретная городская новость</title><meta property="og:type" content="article"><meta property="article:published_time" content="{today}T00:00:00+03:00"></head><article><h1>Конкретная городская новость</h1><p>Сегодня в городе состоялось конкретное событие с участием жителей. Подробности опубликовала городская администрация.</p></article>"#)
            };
            async move {(Ok(net::Page{url:url.clone(),status:200,html,hops:vec![url],cache:Default::default()}),vec!["example.org".into()])}
        }).await;
        assert_eq!(state.reads, 4);
        assert_eq!(state.cached.len(), 3);
        assert_eq!(state.trace["links"].as_array().unwrap().len(), 3);
        assert_eq!(state.evidence_candidates().len(), 3);
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 4);
        assert!(calls.iter().all(|u| u.starts_with("https://example.org/")));
        assert!(!calls.iter().any(|u| u.contains("four")));
        for candidate in state.evidence_candidates() {
            assert!(
                !candidate.snippet.contains('…'),
                "no invented metadata/body quote joins"
            );
            let (record, page) = state.read_cached(&candidate, &case.query);
            assert_eq!(record["news"]["date_check"]["eligible"], true);
            assert_eq!(record["parent_url"], "https://example.org/");
            assert_eq!(page.unwrap().publication["published_date"], expected_date);
        }
    }

    #[tokio::test]
    async fn redirected_block_is_remembered_and_read_errors_are_visible() {
        let case = Case {
            id: "test".into(),
            query: "новости города".into(),
            forms: vec![],
        };
        let a = card("https://origin.example/start");
        let mut b = card("https://blocked.example/city-event/");
        b.id = "C002".into();
        let calls = Arc::new(Mutex::new(0));
        let counter = calls.clone();
        let mut blocked = HashMap::new();
        let state = prepare_with_fetch(
            &case,
            &[a, b],
            &HashMap::new(),
            &mut blocked,
            Instant::now(),
            None,
            move |url, _, _| {
                *counter.lock().unwrap() += 1;
                async move {
                    (
                        Ok(net::Page {
                            url: "https://blocked.example/city-event/".into(),
                            status: 403,
                            html: "denied".into(),
                            hops: vec![url],
                            cache: Default::default(),
                        }),
                        vec!["origin.example".into(), "blocked.example".into()],
                    )
                }
            },
        )
        .await;
        assert_eq!(*calls.lock().unwrap(), 1);
        assert!(blocked.contains_key("blocked.example"));
        let mut report = rescue::Report::default();
        let mut classification = crate::policy::Classifications::default();
        state.enforce(&mut classification, &mut report);
        assert_eq!(report.reads, 1);
        assert!(report.errors[0].contains("source_blocked"));
    }

    #[tokio::test]
    async fn blocked_and_foreign_redirects_are_rejected_before_dns_or_http() {
        let blocks = HashMap::from([("blocked.example".into(), "source_blocked".into())]);
        let rejected = net::fetch_guarded("https://blocked.example/news/", |u| {
            check_hop(u, &blocks, None)
        })
        .await
        .err()
        .unwrap();
        assert!(rejected.to_string().starts_with("host_disabled"));
        let target = net::validate_url("https://other.example/news/").unwrap();
        assert!(check_hop(&target, &HashMap::new(), Some("https://origin.example/")).is_err());
        let same = net::validate_url("https://origin.example/news/").unwrap();
        assert!(check_hop(&same, &HashMap::new(), Some("https://origin.example/")).is_ok());
    }

    #[test]
    fn feed_navigation_is_filtered_before_its_result_limit() {
        let menu = (0..80).map(|i|format!("<a href='/theme/long-city-topic-{i}/'>Новости города длинная тема номер {i}</a>")).collect::<String>();
        let html = format!(
            "{menu}<a href='/news/a-concrete-city-event/'>Произошло конкретное событие с участием жителей города</a>"
        );
        let links = crate::news::links(&html, "https://example.org/", "новости города", 3).unwrap();
        assert_eq!(links.len(), 1);
        assert!(links[0]["url"].as_str().unwrap().contains("/news/"));
    }

    #[test]
    fn numeric_article_urls_and_city_case_endings_survive_legal_navigation() {
        let html = r#"<a href='/cookie-policy/'>Политика использования cookies на городском сайте</a><a href='/usloviya-ispolzovaniya-i-politiku-konfidencialnosti/'>Условия использования и политика конфиденциальности</a><a href='/news/a-national-news-story/'>Новость про события в другом регионе страны</a><a href='/text/transport/2026/11/03/76741111/'>В Воронеже изменилось движение городских автобусов</a>"#;
        let links =
            crate::news::links(html, "https://example.org/", "новости Воронежа", 3).unwrap();
        assert_eq!(links.len(), 2);
        assert!(links[0]["url"].as_str().unwrap().contains("76741111"));
    }

    #[test]
    fn unknown_dates_feeds_and_unread_cards_cannot_keep_direct() {
        let candidates = vec![card("https://example.org/")];
        let state = Prepared {
            candidates,
            documents: vec![],
            cached: HashMap::new(),
            trace: Value::Null,
            reads: 0,
        };
        let mut labels = crate::policy::Classifications::default();
        labels.kinds.insert("C001".into(), 0);
        labels
            .evidence
            .insert("C001".into(), "promising news source".into());
        let mut report = rescue::Report::default();
        report
            .proofs
            .insert("C001".into(), json!(["accepted by model"]));
        state.enforce(&mut labels, &mut report);
        assert_eq!(labels.kinds["C001"], 1);
        assert!(report.proofs.is_empty());
        assert!(labels.evidence.is_empty());
    }
}
