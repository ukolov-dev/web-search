use super::*;

fn old_case(id: &str) -> Value {
    serde_json::from_str::<Value>(include_str!(
        "../tests/fixtures/research/old-luna-gates.json"
    ))
    .unwrap()[id]
        .clone()
}

fn old_session<'a>(input: &'a Input, fixture: &Value) -> Session<'a, Fake> {
    let mut session = Session::new(input, Fake::new(), None);
    let context = &fixture["context"];
    session.plan.facets = serde_json::from_value(context["facets"].clone()).unwrap();
    session.missing = (0..session.plan.facets.len()).collect();
    session.chunks = context["chunks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| Chunk {
            id: c["id"].as_str().unwrap().into(),
            source_id: c["source_id"].as_str().unwrap().into(),
            document_version: c["document_version"].as_str().unwrap().into(),
            field: "text",
            start: c["start"].as_u64().unwrap() as usize,
            end: c["end"].as_u64().unwrap() as usize,
            text: c["text"].as_str().unwrap().into(),
        })
        .collect();
    session.sources = context["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| Source {
            id: s["id"].as_str().unwrap().into(),
            url: s["url_preview"].as_str().unwrap().into(),
            document_version: session
                .chunks
                .iter()
                .find(|c| c.source_id == s["id"])
                .map(|c| c.document_version.clone())
                .unwrap_or_default(),
            text_hash: String::new(),
            title: s["title"].as_str().unwrap().into(),
            publication: s["publication"].clone(),
            eligible: s["eligible"] == true,
            eligibility: "read_body".into(),
            parent_url: None,
            cache: Value::Null,
            extraction: Value::Null,
            rfc: None,
        })
        .collect();
    session
}

#[test]
fn old_r01_secondary_should_is_withheld_even_after_supported_audit() -> Result<()> {
    let fixture = old_case("R01");
    let input: Input = serde_json::from_value(json!({"question":fixture["context"]["question"]}))?;
    let mut session = old_session(&input, &fixture);
    let visible = session.chunks.iter().map(|c| c.id.clone()).collect();
    let mut draft: Draft = serde_json::from_value(fixture["draft"].clone())?;
    assert_eq!(fixture["audit"]["checks"][0]["verdict"], "supported");
    session.prune_invalid_claims(&mut draft, &visible)?;
    assert!(
        draft.claims.iter().all(|c| !c.text.contains("SHOULD")),
        "an exact secondary quote cannot establish the RFC modality"
    );
    Ok(())
}

#[test]
fn old_r03_keeps_primary_after_bad_additional_quote() -> Result<()> {
    let fixture = old_case("R03");
    let input = input();
    let mut session = old_session(&input, &fixture);
    let visible = session.chunks.iter().map(|c| c.id.clone()).collect();
    let mut draft: Draft = serde_json::from_value(fixture["draft"].clone())?;
    draft.claims.truncate(2);
    session.prune_invalid_claims(&mut draft, &visible)?;
    assert_eq!(draft.claims.len(), 2);
    assert!(
        draft
            .claims
            .iter()
            .all(|c| c.evidence.len() == 1 && c.evidence[0].chunk_id.starts_with("S1P"))
    );
    assert!(!session.audited, "pruning is not a semantic verdict");
    Ok(())
}

#[test]
fn old_r03_array_example_reaches_semantic_audit() -> Result<()> {
    let fixture = old_case("R03");
    let input = input();
    let session = old_session(&input, &fixture);
    let visible = session.chunks.iter().map(|c| c.id.clone()).collect();
    let mut draft: Draft = serde_json::from_value(fixture["draft"].clone())?;
    draft.claims = vec![draft.claims.pop().unwrap()];
    assert!(draft.claims[0].text.contains("[4]"));
    session.validate_draft(&draft, &visible)?;
    Ok(())
}

#[test]
fn rfc_primary_role_requires_final_document_url_and_own_identity() {
    let mut document = Document {
        url: "https://www.rfc-editor.org/rfc/rfc6585.html".into(),
        text: "Request for Comments: 6585\n\nRFC 6585\n\nA server MAY send a header.".into(),
        meta: json!({"title":"RFC 6585 - Additional HTTP Status Codes"}),
        links: vec![],
        cache: Value::Null,
        extraction: Value::Null,
    };
    assert!(primary_rfc(&document).is_some());
    for url in [
        "https://www.rfc-editor.org.evil.example/rfc/rfc6585.html",
        "https://evil.example/rfc/rfc6585.html",
        "https://www.rfc-editor.org/info/rfc6585",
        "https://user@www.rfc-editor.org/rfc/rfc6585.html",
        "https://www.rfc-editor.org/rfc/rfc6455.html",
    ] {
        document.url = url.into();
        assert!(primary_rfc(&document).is_none(), "{url}");
    }
    document.url = "https://datatracker.ietf.org/doc/html/rfc6585/".into();
    assert!(primary_rfc(&document).is_some());
    document.meta = json!({"title":"A blog about RFC 6455"});
    assert!(primary_rfc(&document).is_none());
    document.meta = json!({"title":"RFC 6585"});
    document.text = "Request for Comments: 65850\n\nA server SHOULD include Retry-After.".into();
    assert!(
        primary_rfc(&document).is_none(),
        "own identity must match a whole RFC number"
    );
    document.meta = json!({"title":"","heading":""});
    document.text = "Request for Comments: 6585\n\nA server MAY send a header.".into();
    assert!(
        primary_rfc(&document).is_some(),
        "legacy HTML own numbered header does not require a title tag"
    );
    document.meta = json!({"title":"RFC 6455"});
    assert!(
        primary_rfc(&document).is_none(),
        "contradictory own metadata is not ignored"
    );
    document.meta = json!({});
    document.text = "A blog about RFC 6585 says a server SHOULD send a header.".into();
    assert!(
        primary_rfc(&document).is_none(),
        "a mere body mention is not an own document header"
    );
    document.meta = json!({"title":"A blog about RFC 6585"});
    assert!(
        primary_rfc(&document).is_none(),
        "matching prose mentions in title and body are not own headers"
    );
    document.meta = json!({"title":"RFC 6585"});
    document.text = "Stream: IETF\nRFC 6585\n\nThe server MAY send a header.".into();
    assert!(
        primary_rfc(&document).is_some(),
        "a modern own RFC heading binds with its matching metadata"
    );
}

#[tokio::test]
async fn rfc_info_candidate_uses_own_body_without_widening_site_scope() -> Result<()> {
    for restricted in [false, true] {
        let question = if restricted {
            "RFC 6585 site:www.rfc-editor.org/info/rfc6585"
        } else {
            "RFC 6585 Retry-After MAY MUST"
        };
        let input: Input = serde_json::from_value(json!({"question":question}))?;
        let mut fake = Fake::new();
        fake.both_sources = false;
        fake.urls = Some(if restricted {
            vec!["https://www.rfc-editor.org/info/rfc6585/".into()]
        } else {
            vec![
                "https://www.rfc-editor.org/info/rfc7234/".into(),
                "https://www.rfc-editor.org/info/rfc6585/".into(),
            ]
        });
        for n in [6585, 7234] {
            for url in [
                format!("https://www.rfc-editor.org/info/rfc{n}/"),
                format!("https://www.rfc-editor.org/rfc/rfc{n}.html"),
            ] {
                fake.docs.insert(
                    url.clone(),
                    Document {
                        url,
                        text: format!(
                            "Request for Comments: {n}\n\nRFC {n}\n\nThe server MAY send a header."
                        ),
                        meta: json!({"title":format!("RFC {n}")}),
                        links: vec![],
                        cache: Value::Null,
                        extraction: Value::Null,
                    },
                );
            }
        }
        let mut session = Session::new(&input, fake, None);
        session.search(question.into()).await?;
        session.read_round().await?;
        assert_eq!(
            session.sources[0].url,
            if restricted {
                "https://www.rfc-editor.org/info/rfc6585/"
            } else {
                "https://www.rfc-editor.org/rfc/rfc6585.html"
            }
        );
        assert_eq!(session.sources[0].rfc.is_some(), !restricted);
        assert!(session.budget.counts.documents <= 2);
        let (context, _) = session.context(&[])?;
        assert_eq!(
            serde_json::from_str::<Value>(&context)?["facet_indices"],
            json!([])
        );
    }
    assert_eq!(
        rfc_body_candidate("https://datatracker.ietf.org/doc/rfc6585/"),
        Some("https://datatracker.ietf.org/doc/html/rfc6585".into())
    );
    assert!(rfc_body_candidate("https://www.rfc-editor.org.evil.example/info/rfc6585/").is_none());
    assert!(rfc_body_candidate("https://www.rfc-editor.org/info/rfc6585/extra").is_none());
    Ok(())
}

#[tokio::test]
async fn rfc_binding_keeps_the_entire_subject_modal_and_negation() -> Result<()> {
    let input: Input =
        serde_json::from_value(json!({"question":"RFC 6585 SHOULD or MAY Retry-After?"}))?;
    let sentence = "The response representations SHOULD include details explaining the condition, and MAY include a Retry-After header indicating how long to wait before making a new request.";
    let mut fake = Fake::new();
    let d = fake.docs.get_mut("https://source1.example/doc").unwrap();
    d.url = "https://www.rfc-editor.org/rfc/rfc6585.html".into();
    d.text = format!(
        "RFC 6585\n\n{sentence}\n\nA server MUST NOT mask any frames that it sends to the client.\n\nA client MUST NOT negotiate TLS 1.2."
    );
    d.meta = json!({"title":"RFC 6585"});
    let mut session = Session::new(&input, fake, None);
    session.plan.facets = vec!["norm".into()];
    session.queue.push_back(Target {
        url: "https://source1.example/doc".into(),
        parent: None,
    });
    session.read_round().await?;
    let visible = session.chunks.iter().map(|c| c.id.clone()).collect();
    let make = |text: String, q: &str| -> Draft {
        serde_json::from_value(json!({"claims":[{"text":text,"kind":"fact","rationale":"","facets":[0],"evidence":[{"chunk_id":"S1P1","quote":q}]}],"next":{"facet":0,"query":""}})).unwrap()
    };
    let valid = make(format!("RFC 6585: {sentence}"), sentence);
    session.validate_draft(&valid, &visible)?;
    for bad in [
        make("RFC 6585 says servers SHOULD include Retry-After.".into(), sentence),
        make("RFC 6585: MAY include a Retry-After header indicating how long to wait before making a new request.".into(), "MAY include a Retry-After header indicating how long to wait before making a new request."),
        make(format!("RFC 6455: {sentence}"), sentence),
        make("RFC 6585: A server MUST mask any frames that it sends to the client.".into(), "A server MUST NOT mask any frames that it sends to the client."),
        make("RFC 6585: A client MUST NOT negotiate TLS 1.".into(), "A client MUST NOT negotiate TLS 1."),
    ] {
        assert!(session.apply_audit(bad, serde_json::from_value(audit(1,&[],true))?, &visible, &visible).is_err(), "supported judge cannot bypass the binding gate");
    }
    let mut no_version = valid;
    session.chunks[0].document_version = "other-version".into();
    session.prune_invalid_claims(&mut no_version, &visible)?;
    assert!(no_version.claims.is_empty());
    Ok(())
}

#[test]
fn numeric_arrays_and_indices_do_not_allow_links_or_fabricated_source_refs() {
    for text in [
        "Результат [1, 2, 3, 4] — [4].",
        "a[0] is 1.",
        "Empty array []",
        "vec![1, 2]",
        "Returns [at, len); leaves [0, at).",
    ] {
        assert!(safe_claim_text(text), "{text}");
    }
    for text in [
        "[label](https://example.com)",
        "[1](/relative)",
        "![label](relative)",
        "[label][ref]",
        "[1]: /ref",
        "[S1]",
        "[1](javascript:alert(1))",
        "[at, len)(https://example.com)",
        "[at, len)](/relative)",
        "[at, len, extra)",
        "[label)",
        "[a/b, len)",
        "[[at, len)",
        "<a href='/'>x</a>",
        "<https://example.com>",
    ] {
        assert!(!safe_claim_text(text), "{text}");
    }
}

#[tokio::test]
async fn non_rfc_must_question_still_accepts_a_rust_witness() -> Result<()> {
    let input: Input =
        serde_json::from_value(json!({"question":"In Rust, MUST Vec::truncate reduce capacity?"}))?;
    let mut fake = Fake::new();
    let q = "Note that this method has no effect on the allocated capacity of the vector.";
    fake.docs
        .get_mut("https://source1.example/doc")
        .unwrap()
        .text = q.into();
    let mut session = Session::new(&input, fake, None);
    session.plan.facets = vec!["capacity".into()];
    session.queue.push_back(Target {
        url: "https://source1.example/doc".into(),
        parent: None,
    });
    session.read_round().await?;
    let visible = session.chunks.iter().map(|c| c.id.clone()).collect();
    let draft: Draft = serde_json::from_value(
        json!({"claims":[{"text":"truncate does not change the allocated capacity.","kind":"fact","rationale":"","facets":[0],"evidence":[{"chunk_id":"S1P1","quote":q}]}],"next":{"facet":0,"query":""}}),
    )?;
    session.validate_draft(&draft, &visible)?;
    Ok(())
}

#[test]
fn audit_can_use_additional_chunks_visible_after_pruning() -> Result<()> {
    let fixture = old_case("R03");
    let input = input();
    let mut session = old_session(&input, &fixture);
    session.plan.facets = vec!["condition".into()];
    for (i, c) in session.chunks.iter_mut().enumerate() {
        c.text = format!("Condition {i}. {}", "x".repeat(1180));
    }
    for s in &mut session.sources {
        s.url = format!("https://example.com/{}", "x".repeat(980));
        s.title = "x".repeat(120);
    }
    let mut claims = vec![];
    for i in 0..4 {
        claims.push(Claim {
            text: format!("Claim {i} {}", "x".repeat(220)),
            kind: Kind::Fact,
            rationale: String::new(),
            facets: vec![0],
            evidence: session.chunks[i * 2..i * 2 + 2]
                .iter()
                .map(|c| Quote {
                    selected_start: None,
                    chunk_id: c.id.clone(),
                    quote: c.text[..360].into(),
                })
                .collect(),
        });
    }
    let (_, before) = session.context(&claims)?;
    claims.truncate(1);
    let (_, after) = session.context(&claims)?;
    assert!(before.is_subset(&after) && before.len() < after.len());
    let new = session
        .chunks
        .iter()
        .find(|c| after.contains(&c.id) && !before.contains(&c.id) && c.source_id != "S1")
        .unwrap();
    let mut check = audit(1, &[], false);
    check["conflicts"] = json!([{"facet":0,"left":claims[0].evidence[0],"right":{"chunk_id":new.id,"quote":new.text[..50]},"reason":"A newly visible opposing original chunk."}]);
    let draft = Draft {
        claims,
        next: FollowUp::default(),
    };
    assert!(
        !session
            .apply_audit(draft, serde_json::from_value(check)?, &after, &after)?
            .0
    );
    assert!(session.claims.is_empty());
    Ok(())
}

#[tokio::test]
async fn pruning_does_not_remove_opposing_chunks_or_conflict_quarantine() -> Result<()> {
    let input = input();
    let mut session = Session::new(&input, Fake::new(), None);
    session.plan.facets = vec!["condition".into()];
    for i in 1..=2 {
        session.queue.push_back(Target {
            url: format!("https://source{i}.example/doc"),
            parent: None,
        });
    }
    session.read_round().await?;
    let visible = session.chunks.iter().map(|c| c.id.clone()).collect();
    let mut draft: Draft = serde_json::from_value(
        json!({"claims":[claim("Условие подтверждено.",0,&[1,2])],"next":{"facet":0,"query":""}}),
    )?;
    draft.claims[0].evidence[1].quote = "A fabricated additional citation.".into();
    session.prune_invalid_claims(&mut draft, &visible)?;
    assert_eq!(draft.claims[0].evidence.len(), 1);
    let (_, audit_visible) = session.context(&draft.claims)?;
    assert_eq!(visible, audit_visible);
    let mut check = audit(1, &[], true);
    check["conflicts"] = json!([{"facet":0,"left":quote(1),"right":quote(2),"reason":"An actual unresolved contradiction in the test."}]);
    assert!(
        !session
            .apply_audit(
                draft,
                serde_json::from_value(check)?,
                &audit_visible,
                &audit_visible
            )?
            .0
    );
    assert!(session.claims.is_empty());
    assert_eq!(session.missing, vec![0]);
    Ok(())
}

#[tokio::test]
async fn zero_valid_quotes_or_bad_structure_remain_withheld() -> Result<()> {
    let fixture = old_case("R03");
    let input = input();
    for mutation in 0..4 {
        let mut session = old_session(&input, &fixture);
        let visible = session.chunks.iter().map(|c| c.id.clone()).collect();
        let mut draft: Draft = serde_json::from_value(fixture["draft"].clone())?;
        draft.claims.truncate(1);
        match mutation {
            0 => {
                for q in &mut draft.claims[0].evidence {
                    q.chunk_id = "hidden".into()
                }
            }
            1 => {
                for s in &mut session.sources {
                    s.eligible = false
                }
            }
            2 => draft.claims[0].facets = vec![99],
            _ => {
                let q = draft.claims[0].evidence[0].clone();
                draft.claims[0].evidence.push(q)
            }
        }
        session.prune_invalid_claims(&mut draft, &visible)?;
        assert!(draft.claims.is_empty());
    }
    Ok(())
}

#[tokio::test]
#[ignore = "explicit OpenRouter semantic audit of frozen OLD R03, no source HTTP"]
async fn old_r03_live_audit_of_preserved_original_chunks() -> Result<()> {
    let directory = std::path::PathBuf::from(std::env::var("RESEARCH_GATE_REPLAY_DIR")?);
    std::fs::create_dir(&directory)?;
    let fixture = old_case("R03");
    let input: Input =
        serde_json::from_value(json!({"question":fixture["context"]["question"],"language":"ru"}))?;
    let mut session = old_session(&input, &fixture);
    let (before, visible) = session.context(&[])?;
    let mut draft: Draft = serde_json::from_value(fixture["draft"].clone())?;
    session.prune_invalid_claims(&mut draft, &visible)?;
    assert_eq!(draft.claims.len(), 3);
    let (context, audit_visible, conflict_visible) = session.audit_context(&draft.claims)?;
    assert_eq!(visible, audit_visible);
    let path =
        std::path::PathBuf::from(std::env::var("HOME")?).join(".config/web-search/openrouter.key");
    let model = crate::model::Model::from_openrouter(
        &path,
        "deepseek/deepseek-v4.1-flash-1017",
        Some("http://127.0.0.1:10808"),
    )?;
    let call = model
        .research_call(
            &format!("{RULES}\n{AUDIT_PROMPT}"),
            &context,
            serde_json::to_value(schemars::schema_for!(Audit))?,
            3072,
            Duration::from_secs(90),
        )
        .await?;
    crate::save(&directory.join("call.json"), &call)?;
    let audit: Audit = serde_json::from_str(
        call["response"]["choices"][0]["message"]["content"]
            .as_str()
            .context("audit content")?,
    )?;
    let (_, next) = session.apply_audit(draft, audit, &audit_visible, &conflict_visible)?;
    crate::save(
        &directory.join("result.json"),
        &session.result("component_replay", None),
    )?;
    crate::save(
        &directory.join("inputs.json"),
        &json!({"before":serde_json::from_str::<Value>(&before)?,"after":serde_json::from_str::<Value>(&context)?,"next":next}),
    )?;
    assert_eq!(
        session.claims.len(),
        3,
        "the real semantic audit must support the preserved claims"
    );
    Ok(())
}

struct Fake {
    replies: VecDeque<Value>,
    docs: HashMap<String, Document>,
    searches: usize,
    both_sources: bool,
    disabled: bool,
    slow_read: bool,
    urls: Option<Vec<String>>,
}

fn input() -> Input {
    Input {
        question: "Проверь ограничения и исключения протокола 🔎".into(),
        language: Language::Ru,
        cache_mode: CacheMode::Prefer,
    }
}
fn body(i: usize) -> String {
    format!(
        "🔎 Русский исходник {i}. Protocol condition {i} is explicitly documented here. Exception {i} must also be considered."
    )
}
fn quote(i: usize) -> Value {
    json!({"chunk_id":format!("S{i}P1"),"quote":format!("Protocol condition {i} is explicitly documented here.")})
}
fn claim(text: &str, facet: usize, sources: &[usize]) -> Value {
    json!({"text":text,"kind":"fact","rationale":"","facets":[facet],"evidence":sources.iter().map(|&i|quote(i)).collect::<Vec<_>>()})
}
fn audit(n: usize, missing: &[usize], sufficient: bool) -> Value {
    json!({"checks":(0..n).map(|i|json!({"id":claim_id(i),"verdict":"supported","reason":"Entire bounded statement is supported."})).collect::<Vec<_>>(),"conflicts":[],"missing":missing,"sufficient":sufficient,"next":{"facet":1,"query":"protocol exception evidence"}})
}
fn replies() -> VecDeque<Value> {
    VecDeque::from(vec![
        json!({"facets":["Condition","Exception"],"query":"protocol conditions"}),
        json!({"claims":[claim("Первое условие подтверждено.",0,&[1,2])],"next":{"facet":1,"query":"protocol exception evidence"}}),
        audit(1, &[1], false),
        json!({"claims":[claim("Первое условие подтверждено.",0,&[1,2]),claim("Исключение подтверждено отдельно.",1,&[3,4])],"next":{"facet":1,"query":""}}),
        audit(2, &[], true),
    ])
}
impl Fake {
    fn new() -> Self {
        Self {
            replies: replies(),
            docs: (1..=4)
                .map(|i| {
                    let url = format!("https://source{i}.example/doc");
                    (
                        url.clone(),
                        Document {
                            url,
                            text: body(i),
                            meta: json!({"title":format!("Source {i}")}),
                            links: vec![],
                            cache: json!({"hit":true}),
                            extraction: json!({"truncated":false}),
                        },
                    )
                })
                .collect(),
            searches: 0,
            both_sources: true,
            disabled: false,
            slow_read: false,
            urls: None,
        }
    }
}
impl Backend for Fake {
    fn available(&self, engine: &'static str) -> bool {
        !self.disabled && (self.both_sources || engine == "seznam")
    }
    fn has_qwen(&self) -> bool {
        true
    }
    async fn search(&mut self, engine: &'static str, query: &str) -> Result<crate::policy::Source> {
        assert!(self.available(engine));
        let round = if self.both_sources {
            self.searches / 2
        } else {
            self.searches
        };
        self.searches += 1;
        Ok(crate::policy::Source {
            engine: engine.into(),
            query: query.into(),
            results: if let Some(urls) = &self.urls {
                urls.iter()
                    .enumerate()
                    .map(|(i, url)| crate::search::SearchResult {
                        title: format!("Candidate {i}"),
                        url: url.clone(),
                        snippet: "Protocol evidence".into(),
                        position: i + 1,
                        result_source: None,
                    })
                    .collect()
            } else {
                ((round * 2 + 1)..=(round * 2 + 2))
                    .map(|i| crate::search::SearchResult {
                        title: format!("Source {i}"),
                        url: format!("https://source{i}.example/doc"),
                        snippet: "Protocol evidence".into(),
                        position: i,
                        result_source: None,
                    })
                    .collect()
            },
            error: None,
            http_status: Some(200),
            hops: vec![],
            html_hash: None,
            elapsed_ms: 0,
        })
    }
    async fn read(&mut self, target: &Target, _: &str) -> Result<Document> {
        if self.slow_read {
            std::future::pending::<()>().await;
        }
        self.docs
            .remove(&target.url)
            .context("fake missing document")
    }
    async fn rank(
        &mut self,
        _: &str,
        text: &str,
        _: Instant,
    ) -> Result<(Vec<(usize, usize)>, Value)> {
        Ok((vec![(0, text.len())], json!({"call":{"cache_hit":true}})))
    }
    async fn generate(
        &mut self,
        _: &str,
        context: &str,
        _: Value,
        remaining: Duration,
    ) -> Result<Value> {
        assert!(context.chars().count() <= 24_000);
        assert!(!remaining.is_zero());
        let parsed: Value = serde_json::from_str(context)?;
        assert_eq!(
            parsed["facet_indices"],
            json!((0..parsed["facets"].as_array().unwrap().len()).collect::<Vec<_>>()),
            "draft/audit receive the exact zero-based facet IDs, including [0,1]"
        );
        assert!(parsed["budget"]["used"]["generations"].as_u64().unwrap() < 6);
        let value = self.replies.pop_front().context("fake transport failure")?;
        let content = if value == json!("bad_json") {
            "broken JSON".into()
        } else {
            serde_json::to_string(&value)?
        };
        Ok(
            json!({"response":{"choices":[{"finish_reason":"stop","message":{"content":content}}],"usage":{"cost":0.001}},"elapsed_ms":0}),
        )
    }
}

#[tokio::test]
async fn two_rounds_share_all_budgets_and_unicode_provenance() -> Result<()> {
    let input = input();
    let value = Session::new(&input, Fake::new(), None)
        .execute(std::future::pending())
        .await?;
    assert_eq!(value["status"], "ok");
    assert_eq!(value["stop_reason"], "evidence_sufficient");
    assert_eq!(
        value["budget"]["used"],
        json!({"forms":2,"serp":4,"documents":4,"qwen":4,"generations":5})
    );
    assert_eq!(value["query_forms"][1], "protocol exception evidence");
    let answer = value["answer"].as_str().unwrap();
    for claim in value["claims"].as_array().unwrap() {
        let start = claim["answer_start"].as_u64().unwrap() as usize;
        let end = claim["answer_end"].as_u64().unwrap() as usize;
        assert_eq!(
            answer
                .chars()
                .skip(start)
                .take(end - start)
                .collect::<String>(),
            claim["text"].as_str().unwrap()
        );
        for witness in claim["evidence"].as_array().unwrap() {
            let id = witness["source_id"].as_str().unwrap()[1..].parse::<usize>()?;
            let start = witness["start"].as_u64().unwrap() as usize;
            let end = witness["end"].as_u64().unwrap() as usize;
            assert_eq!(
                body(id)
                    .chars()
                    .skip(start)
                    .take(end - start)
                    .collect::<String>(),
                witness["quote"].as_str().unwrap()
            );
            assert_ne!(
                start,
                body(id).find(witness["quote"].as_str().unwrap()).unwrap(),
                "multibyte prefix must distinguish code points from bytes"
            );
            assert_eq!(
                witness["document_version"],
                value["sources"][id - 1]["document_version"]
            );
        }
    }
    assert!(serde_json::to_vec(&value)?.len() < OUTPUT_BYTES);
    assert!(
        !serde_json::to_string(&value)?.contains("Русский исходник"),
        "full bodies stay out of public output"
    );
    Ok(())
}

#[tokio::test]
async fn failed_final_audit_withholds_pending_claims_but_keeps_sources() -> Result<()> {
    let input = input();
    let mut fake = Fake::new();
    fake.replies.pop_back();
    let value = Session::new(&input, fake, None)
        .execute(std::future::pending())
        .await?;
    assert_eq!(value["status"], "partial");
    assert_eq!(value["stop_reason"], "model_error");
    assert_eq!(value["answer"], "");
    assert_eq!(value["sources"].as_array().unwrap().len(), 4);
    assert_eq!(value["model_calls"][4]["cost_known"], false);
    assert_eq!(value["budget"]["used"]["generations"], 5);
    Ok(())
}

#[tokio::test]
async fn unknown_ids_changed_quotes_and_markdown_never_enter_answer() -> Result<()> {
    for mutation in [0, 1, 2, 3] {
        let input = input();
        let mut fake = Fake::new();
        let draft = &mut fake.replies[1];
        match mutation {
            0 => draft["claims"][0]["evidence"][0]["chunk_id"] = json!("S99P1"),
            1 => {
                draft["claims"][0]["evidence"][0]["quote"] =
                    json!("This is fabricated source evidence.")
            }
            2 => draft["claims"][0]["text"] = json!("[Do something](https://untrusted.example)"),
            _ => {
                draft["claims"][0]["kind"] = json!("inference");
                draft["claims"][0]["rationale"] = json!("");
            }
        }
        let value = Session::new(&input, fake, None)
            .execute(std::future::pending())
            .await?;
        assert!(
            value["actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["action"]
                    == if mutation < 2 {
                        "withhold_quote"
                    } else {
                        "withhold_claim"
                    })
        );
        assert!(!value["answer"].as_str().unwrap().contains("Do something"));
        assert!(value["budget"]["used"]["generations"].as_u64().unwrap() <= 6);
    }
    Ok(())
}

#[tokio::test]
async fn witnessed_conflict_quarantines_otherwise_supported_claim() -> Result<()> {
    let input = input();
    let mut fake = Fake::new();
    fake.replies[2]["conflicts"] = json!([{"facet":0,"left":quote(1),"right":quote(2),"reason":"Sources disagree on this condition."}]);
    fake.replies[4]["checks"][0]["verdict"] = json!("supported");
    let value = Session::new(&input, fake, None)
        .execute(std::future::pending())
        .await?;
    assert_eq!(value["status"], "partial");
    assert!(value["missing"].as_array().unwrap().contains(&json!(0)));
    assert!(!value["answer"].as_str().unwrap().contains("Первое условие"));
    assert_eq!(value["conflicts"][0]["left"]["source_id"], "S1");
    assert_eq!(value["claims"].as_array().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn one_shared_schema_repair_and_no_hidden_transport_retry() -> Result<()> {
    let input = input();
    let mut fake = Fake::new();
    fake.replies.push_front(json!("bad_json"));
    let value = Session::new(&input, fake, None)
        .execute(std::future::pending())
        .await?;
    assert_eq!(value["status"], "ok");
    assert_eq!(value["budget"]["used"]["generations"], 6);
    let mut fake = Fake::new();
    fake.replies = VecDeque::from(vec![json!("bad_json"), json!("bad_json")]);
    let value = Session::new(&input, fake, None)
        .execute(std::future::pending())
        .await?;
    assert_eq!(value["status"], "error");
    assert_eq!(value["budget"]["used"]["generations"], 2);
    assert_eq!(value["budget"]["used"]["serp"], 0);
    Ok(())
}

#[tokio::test]
async fn deadline_cancellation_and_refusals_start_no_extra_actions() -> Result<()> {
    let input = input();
    let mut session = Session::new(&input, Fake::new(), None);
    session.budget.started = Instant::now() - Duration::from_secs(SECONDS + 1);
    let value = session.execute(std::future::pending()).await?;
    assert_eq!(value["stop_reason"], "deadline");
    assert_eq!(value["budget"]["used"]["generations"], 0);
    let mut fake = Fake::new();
    fake.slow_read = true;
    let value = Session::new(&input, fake, None)
        .execute(async { tokio::time::sleep(Duration::from_millis(10)).await })
        .await?;
    assert_eq!(value["stop_reason"], "cancelled");
    assert_eq!(value["budget"]["used"]["documents"], 1);
    assert_eq!(value["budget"]["used"]["generations"], 1);
    let mut fake = Fake::new();
    fake.disabled = true;
    let value = Session::new(&input, fake, None)
        .execute(std::future::pending())
        .await?;
    assert_eq!(value["stop_reason"], "sources_unavailable");
    assert_eq!(value["budget"]["used"]["serp"], 0);
    assert_eq!(value["budget"]["used"]["documents"], 0);
    Ok(())
}

#[tokio::test]
async fn news_feed_reads_count_and_invalid_dates_have_no_chunks() -> Result<()> {
    let mut input = input();
    input.question = "Новости города за 5 ноября 2026".into();
    let mut fake = Fake::new();
    fake.docs
        .get_mut("https://source1.example/doc")
        .unwrap()
        .meta = json!({"kind":"feed"});
    fake.docs
        .get_mut("https://source1.example/doc")
        .unwrap()
        .links = vec!["https://source1.example/article".into()];
    fake.docs
        .get_mut("https://source3.example/doc")
        .unwrap()
        .meta = json!({"kind":"article","date_status":"valid","published_date":"2026-11-05","publication_witnesses":[{"source":"jsonld.datePublished","raw":"2026-11-05","date":"2026-11-05"}]});
    let mut article = fake.docs.remove("https://source3.example/doc").unwrap();
    article.url = "https://source1.example/article".into();
    fake.docs.insert(article.url.clone(), article);
    let mut session = Session::new(&input, fake, None);
    session.window = Some(crate::news::window(&input.question, "2026-11-05"));
    session.queue.push_back(Target {
        url: "https://source1.example/doc".into(),
        parent: None,
    });
    session.queue.push_back(Target {
        url: "https://source2.example/doc".into(),
        parent: None,
    });
    session.read_round().await?;
    assert_eq!(session.budget.counts.documents, 2);
    assert!(!session.sources[0].eligible);
    assert!(session.sources[1].eligible);
    assert_eq!(
        session.sources[1].parent_url.as_deref(),
        Some("https://source1.example/doc")
    );
    session.read_round().await?;
    assert_eq!(session.budget.counts.documents, 3);
    assert!(session.chunks.iter().all(|c| c.source_id == "S2"));
    assert_eq!(session.sources[2].eligibility, "not_article");
    Ok(())
}

#[tokio::test]
async fn output_bound_withholds_answer_without_dangling_citations() -> Result<()> {
    let input = input();
    let mut session = Session::new(&input, Fake::new(), None);
    assert_eq!(session.work().await?, "evidence_sufficient");
    for source in &mut session.sources {
        source.url = format!("https://example.com/{}", "ю".repeat(4096));
    }
    let value = session.result("evidence_sufficient", None);
    assert!(serde_json::to_vec(&value)?.len() <= OUTPUT_BYTES - 1024);
    assert_eq!(value["output_limited"], true);
    assert_eq!(value["status"], "partial");
    assert_eq!(value["claims"], json!([]));
    assert_eq!(value["answer"], "");
    Ok(())
}

#[test]
fn input_and_query_trust_boundaries() -> Result<()> {
    assert!(
        serde_json::from_value::<Input>(json!({"question":"q","model":"http://127.0.0.1"}))
            .is_err()
    );
    let mut input = input();
    input.question = "foo site:docs.example/api".into();
    let session = Session::new(&input, Fake::new(), None);
    assert_eq!(
        session.query("foo exceptions")?,
        "foo exceptions site:docs.example/api"
    );
    assert!(session.query("foo site:evil.example").is_err());
    assert!(session.query("foo site:127.0.0.1").is_err());
    assert!(session.query(" ").is_err());
    let unrestricted = Input {
        question: "protocol".into(),
        language: Language::Auto,
        cache_mode: CacheMode::Prefer,
    };
    let session = Session::new(&unrestricted, Fake::new(), None);
    assert_eq!(
        session.query("protocol site:docs.example/api")?,
        "protocol site:docs.example/api"
    );
    assert!(session.query("protocol site:127.0.0.1").is_err());
    Ok(())
}

#[tokio::test]
async fn duplicate_bodies_do_not_satisfy_two_document_sufficiency() -> Result<()> {
    let input = input();
    let mut fake = Fake::new();
    fake.docs
        .get_mut("https://source2.example/doc")
        .unwrap()
        .text = body(1);
    let mut session = Session::new(&input, fake, None);
    session.plan.facets = vec!["Condition".into()];
    for i in 1..=2 {
        session.queue.push_back(Target {
            url: format!("https://source{i}.example/doc"),
            parent: None,
        });
    }
    session.read_round().await?;
    assert_eq!(session.budget.counts.documents, 2);
    assert_eq!(session.sources.len(), 1);
    let draft: Draft = serde_json::from_value(
        json!({"claims":[claim("Подтверждённое условие.",0,&[1])],"next":{"facet":0,"query":""}}),
    )?;
    let audit: Audit = serde_json::from_value(audit(1, &[], true))?;
    let visible = session.chunks.iter().map(|c| c.id.clone()).collect();
    session.validate_draft(&draft, &visible)?;
    assert!(
        !session.apply_audit(draft, audit, &visible, &visible)?.0,
        "a duplicate is not a second document"
    );
    let result = session.result("budget_exhausted", None);
    assert_eq!(result["sufficiency"]["cited_distinct_documents"], 1);
    Ok(())
}

#[tokio::test]
async fn exact_multiline_quotes_keep_original_unicode_offsets() -> Result<()> {
    let input = input();
    let mut fake = Fake::new();
    fake.docs
        .get_mut("https://source1.example/doc")
        .unwrap()
        .text
        .push_str("\nSecond line witness statement.");
    let mut session = Session::new(&input, fake, None);
    session.queue.push_back(Target {
        url: "https://source1.example/doc".into(),
        parent: None,
    });
    session.read_round().await?;
    let visible = session.chunks.iter().map(|c| c.id.clone()).collect();
    let quote = Quote {
        selected_start: None,
        chunk_id: "S1P1".into(),
        quote: "considered.\nSecond line witness statement.".into(),
    };
    let witness = session.quote(&quote, &visible)?;
    assert_eq!(witness["quote"], quote.quote);
    let mut control = quote.clone();
    control.quote.push('\0');
    assert!(session.quote(&control, &visible).is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit localhost Qwen transport fixture; no public network or real credentials"]
async fn live_backend_uses_reranker_instead_of_generative_model() -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let settings = tempfile::NamedTempFile::new()?;
    crate::save(
        settings.path(),
        &json!({"server":{"host":"127.0.0.1","port":listener.local_addr()?.port()},"auth":{"api_key":"test-only-local-key"}}),
    )?;
    let model = crate::model::Model::from_settings(settings.path(), "generative-different-model")?
        .with_reranker(Some(settings.path()), true)?;
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut data = vec![];
        let mut buffer = [0; 1024];
        loop {
            let n = socket.read(&mut buffer).await.unwrap();
            assert!(n > 0);
            data.extend_from_slice(&buffer[..n]);
            if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&data[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap();
                if data.len() >= end + 4 + length {
                    assert!(headers.starts_with("POST /v1/rerank "));
                    let body: Value =
                        serde_json::from_slice(&data[end + 4..end + 4 + length]).unwrap();
                    assert_eq!(body["model"], crate::reranker::MODEL);
                    assert_eq!(body["query"], "protocol");
                    break;
                }
            }
        }
        let body =
            json!({"model":crate::reranker::MODEL,"results":[{"index":0,"relevance_score":0.8}]})
                .to_string();
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    });
    let mut blocked = HashMap::new();
    let mut hosts = HashMap::new();
    let mut url_blocks = HashMap::new();
    let mut live = Live {
        model: &model,
        blocked: &mut blocked,
        document_blocks: &mut hosts,
        capture: None,
        site: None,
        qwen_enabled: true,
        read_sequence: 0,
        serp_sequence: 0,
        observed_read_urls: Vec::new(),
        document_url_blocks: &mut url_blocks,
    };
    let (ranges, trace) = tokio::time::timeout(
        Duration::from_secs(5),
        live.rank("protocol", "Protocol fixture body.", Instant::now()),
    )
    .await??;
    assert_eq!(ranges, vec![(0, 22)]);
    assert_eq!(trace["call"]["model"], crate::reranker::MODEL);
    assert!(live.has_qwen());
    server.await?;
    Ok(())
}

#[test]
fn final_quoted_subject_and_exact_range_replay() -> Result<()> {
    let fixtures: Value =
        serde_json::from_str(include_str!("../tests/fixtures/research/final-losses.json"))?;
    for (id, number) in [("D02", 2606), ("D04", 7519)] {
        let fixture = &fixtures[id];
        let input: Input =
            serde_json::from_value(json!({"question":fixture["context"]["question"]}))?;
        let mut session = old_session(&input, fixture);
        session.sources[0].rfc =
            Some((number, fixture["document"]["text"].as_str().unwrap().into()));
        let visible = session.chunks.iter().map(|c| c.id.clone()).collect();
        let draft: Draft = serde_json::from_value(fixture["draft"].clone())?;
        if id == "D02" {
            session.validate_draft(&draft, &visible)?;
        } else {
            assert!(
                session.validate_draft(&draft, &visible).is_err(),
                "rewritten whitespace remains invalid"
            );
        }
        let mut selected = fixture["draft"].clone();
        for claim in selected["claims"].as_array_mut().unwrap() {
            let text = claim["text"]
                .as_str()
                .unwrap()
                .split_once(": ")
                .unwrap()
                .1
                .to_owned();
            let chunk = session
                .chunks
                .iter()
                .find(|c| c.id == claim["evidence"][0]["chunk_id"])
                .unwrap();
            let (start, end) = sentence_ranges(&chunk.text)
                .into_iter()
                .find(|&(start, end)| {
                    chunk
                        .text
                        .chars()
                        .skip(start)
                        .take(end - start)
                        .collect::<String>()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        == text
                })
                .expect("a complete original sentence must be offered");
            claim["evidence"] = json!([{"chunk_id":chunk.id,"start":start,"end":end}]);
            claim["text"] = json!("A model paraphrase must not define a literal RFC claim.");
        }
        let selected: Draft<QuoteRange> = serde_json::from_value(selected)?;
        let draft = session.select_quotes(selected);
        session.validate_draft(&draft, &visible)?;
        let audit_value = audit(draft.claims.len(), &[], true);
        session.apply_audit(
            draft,
            serde_json::from_value(audit_value)?,
            &visible,
            &visible,
        )?;
        assert_eq!(session.claims.len(), 2);
    }
    Ok(())
}

#[test]
fn range_selection_preserves_unicode_and_rejects_invalid_ranges() {
    assert_eq!(
        sentence_ranges("🔎 Заголовок.\n\nA client MUST NOT negotiate TLS 1.2."),
        vec![(0, 12), (14, 50)]
    );
    let input = input();
    let fixture = old_case("R03");
    let session = old_session(&input, &fixture);
    for (start, end) in [(0, 0), (10, 9), (0, 361), (usize::MAX, usize::MAX)] {
        let draft = session.select_quotes(Draft {
            next: FollowUp::default(),
            claims: vec![Claim {
                text: "x".into(),
                kind: Kind::Fact,
                rationale: String::new(),
                facets: vec![0],
                evidence: vec![QuoteRange {
                    chunk_id: session.chunks[0].id.clone(),
                    start,
                    end,
                }],
            }],
        });
        assert!(draft.claims[0].evidence[0].quote.is_empty());
    }
}

#[tokio::test]
#[ignore = "explicit OpenRouter draft and audit of frozen D02/D04, no source HTTP"]
async fn final_losses_live_range_selection_and_audit() -> Result<()> {
    let directory = std::path::PathBuf::from(std::env::var("RESEARCH_GATE_REPLAY_DIR")?);
    std::fs::create_dir(&directory)?;
    let fixtures: Value =
        serde_json::from_str(include_str!("../tests/fixtures/research/final-losses.json"))?;
    let path =
        std::path::PathBuf::from(std::env::var("HOME")?).join(".config/web-search/openrouter.key");
    let model = crate::model::Model::from_openrouter(
        &path,
        "deepseek/deepseek-v4.1-flash-1017",
        Some("http://127.0.0.1:10808"),
    )?;
    for (id, number) in [("D02", 2606), ("D04", 7519)] {
        let fixture = &fixtures[id];
        let input: Input = serde_json::from_value(
            json!({"question":fixture["context"]["question"],"language":"ru"}),
        )?;
        let mut session = old_session(&input, fixture);
        session.sources[0].rfc =
            Some((number, fixture["document"]["text"].as_str().unwrap().into()));
        let (context, visible) = session.context(&[])?;
        let prompt = format!("{RULES}\n{DRAFT_PROMPT}{RFC_RANGE_PROMPT}");
        let call = model
            .research_call(
                &prompt,
                &context,
                serde_json::to_value(schemars::schema_for!(Draft<QuoteRange>))?,
                3072,
                Duration::from_secs(90),
            )
            .await?;
        crate::save(&directory.join(format!("{id}-draft.json")), &call)?;
        let selected: Draft<QuoteRange> = serde_json::from_str(
            call["response"]["choices"][0]["message"]["content"]
                .as_str()
                .context("draft JSON")?,
        )?;
        let mut draft = session.select_quotes(selected);
        session.prune_invalid_claims(&mut draft, &visible)?;
        assert!(
            !draft.claims.is_empty(),
            "{id}: no admitted original ranges"
        );
        let (context, audit_visible, conflict_visible) = session.audit_context(&draft.claims)?;
        let call = model
            .research_call(
                &format!("{RULES}\n{AUDIT_PROMPT}"),
                &context,
                serde_json::to_value(schemars::schema_for!(Audit))?,
                3072,
                Duration::from_secs(90),
            )
            .await?;
        crate::save(&directory.join(format!("{id}-audit.json")), &call)?;
        let audit: Audit = serde_json::from_str(
            call["response"]["choices"][0]["message"]["content"]
                .as_str()
                .context("audit JSON")?,
        )?;
        session.apply_audit(draft, audit, &audit_visible, &conflict_visible)?;
        crate::save(
            &directory.join(format!("{id}-result.json")),
            &session.result("component_replay", None),
        )?;
        assert!(
            session
                .claims
                .iter()
                .any(|c| c.text.contains(if id == "D02" {
                    "invalid"
                } else {
                    "MUST be before"
                })),
            "{id}: target claim not supported"
        );
    }
    Ok(())
}

#[test]
fn lexical_fallback_keeps_requested_api_contract() -> Result<()> {
    let text = include_str!("../tests/fixtures/research/rust-split-off.txt");
    let question = "Проверь Vec::split_off(2) для [1, 2, 3, 4]: результат, остаток и capacity?";
    let ranges = lexical(text, question)?;
    assert!(ranges.len() <= 3);
    assert!(ranges.iter().any(|&(start, end)| {
        let selected = &text[start..end];
        selected.starts_with("pub fn split_off")
            && selected.contains("with its previous capacity unchanged.")
    }));
    for (start, end) in ranges {
        assert!(text[start..end].chars().count() <= 1200);
    }
    Ok(())
}

#[test]
fn selected_range_keeps_the_requested_repeated_occurrence() -> Result<()> {
    let input = input();
    let fixture = old_case("R03");
    let mut session = old_session(&input, &fixture);
    let text = "🔎 A repeated source sentence. A repeated source sentence.";
    let needle = "A repeated source sentence.";
    session.chunks[0].text = text.into();
    session.chunks[0].start = 0;
    let start = text[..text.rfind(needle).unwrap()].chars().count();
    let draft = session.select_quotes(Draft {
        next: FollowUp::default(),
        claims: vec![Claim {
            text: needle.into(),
            kind: Kind::Fact,
            rationale: String::new(),
            facets: vec![0],
            evidence: vec![QuoteRange {
                chunk_id: session.chunks[0].id.clone(),
                start,
                end: start + needle.chars().count(),
            }],
        }],
    });
    let visible = session.chunks.iter().map(|c| c.id.clone()).collect();
    let witness = session.quote(&draft.claims[0].evidence[0], &visible)?;
    assert_eq!(witness["start"], start);
    assert_eq!(witness["quote"], needle);
    assert!(
        serde_json::to_value(&draft)?["claims"][0]["evidence"][0]
            .get("selected_start")
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn failed_reads_without_new_evidence_keep_audited_claims() -> Result<()> {
    let input = input();
    let mut fake = Fake::new();
    fake.docs.remove("https://source3.example/doc");
    fake.docs.remove("https://source4.example/doc");
    let value = Session::new(&input, fake, None)
        .execute(std::future::pending())
        .await?;
    assert_eq!(value["stop_reason"], "no_new_evidence");
    assert_eq!(value["claims"].as_array().unwrap().len(), 1);
    assert!(!value["answer"].as_str().unwrap().is_empty());
    assert_eq!(value["sources"].as_array().unwrap().len(), 2);
    Ok(())
}

#[test]
fn duplicate_non_rfc_witness_is_withheld_without_losing_the_answer() -> Result<()> {
    let fixture: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/research/duplicate-api-witness.json"
    ))?;
    let input: Input = serde_json::from_value(json!({"question":fixture["context"]["question"]}))?;
    let mut session = old_session(&input, &fixture);
    let visible = session.chunks.iter().map(|c| c.id.clone()).collect();
    let mut draft: Draft = serde_json::from_value(fixture["draft"].clone())?;
    session.prune_invalid_claims(&mut draft, &visible)?;
    assert_eq!(
        draft.claims.len(),
        2,
        "the ambiguous complexity witness must not be bound to the first API"
    );
    assert!(draft.claims[0].text.contains("None"));
    assert!(draft.claims.iter().all(|c| !c.text.contains("O(1)")));
    Ok(())
}

#[test]
fn overlapping_unicode_quote_is_ambiguous_without_an_explicit_range() -> Result<()> {
    let input = input();
    let fixture = old_case("R03");
    let mut session = old_session(&input, &fixture);
    session.chunks[0].text = "я".repeat(13);
    let visible = session.chunks.iter().map(|c| c.id.clone()).collect();
    let mut q = Quote {
        chunk_id: session.chunks[0].id.clone(),
        quote: "я".repeat(12),
        selected_start: None,
    };
    assert!(session.quote(&q, &visible).is_err());
    q.selected_start = Some(1);
    assert!(session.quote(&q, &visible).is_ok());
    Ok(())
}

#[tokio::test]
async fn documentation_queue_preserves_api_container_before_unrelated_results() -> Result<()> {
    // Reduced G07 SERP ordering: release notes, a different API, then the requested reference.
    let urls = vec![
        "https://github.com/Niko1221/Strata/releases".to_owned(),
        "https://www.tensorflow.org/api_docs/python/tf/compat/v1/string_split".to_owned(),
        "https://doc.rust-lang.org/nightly/std/primitive.str.html".to_owned(),
    ];
    for (question, expected) in [
        (
            "По документации Rust str::split_once: что возвращается без delimiter?",
            2,
        ),
        ("Rust str::split_once news", 0),
        ("По документации Rust str::split_once site:github.com", 0),
    ] {
        let mut input = input();
        input.question = question.into();
        let mut backend = Fake::new();
        backend.urls = Some(if question.contains("site:") {
            vec![urls[0].clone()]
        } else {
            urls.clone()
        });
        let mut session = Session::new(&input, backend, None);
        session
            .search(session.query("Rust str::split_once documentation")?)
            .await?;
        assert_eq!(session.queue.front().unwrap().url, urls[expected]);
        assert_eq!(session.budget.counts.forms, 1);
        assert_eq!(session.budget.counts.serp, 2);
    }
    Ok(())
}

#[test]
fn documentation_preference_is_not_an_api_or_authority_verdict() {
    for question in [
        "Rust std::vec::Vec::insert documentation",
        "По документации Rust Vec::insert",
    ] {
        assert!(api_documentation_candidate(
            question,
            "https://doc.rust-lang.org/std/vec/struct.Vec.html"
        ));
        for url in [
            "https://vec.example/docs/other.html",
            "https://rust.example/docs/other.html?api=Vec::insert",
            "https://doc.rust-lang.org/std/collections/struct.VecDeque.html",
            "https://python.example/api/vec/insert.html",
            "https://doc.rust-lang.org/std/vec/struct.Vec.html#method.insert_mut",
        ] {
            assert!(!api_documentation_candidate(question, url), "{url}");
        }
    }
    assert!(!api_documentation_candidate(
        "Rust Vec::insert news",
        "https://doc.rust-lang.org/std/vec/struct.Vec.html"
    ));
}

#[tokio::test]
async fn format_repair_recovers_type_text_without_weakening_witness_gates() -> Result<()> {
    let input = input();
    let mut fake = Fake::new();
    fake.replies = VecDeque::from(vec![
        json!({"facets":["Condition"],"query":"protocol conditions"}),
        json!({"claims":[claim("Returns Option<(&str, &str)> or None.",0,&[1,2])],"next":{"facet":0,"query":""}}),
        json!({"claims":[claim("Returns a pair of string slices or None.",0,&[1,2])],"next":{"facet":0,"query":""}}),
        audit(1, &[], true),
    ]);
    let result = Session::new(&input, fake, None)
        .execute(std::future::pending())
        .await?;
    assert_eq!(result["stop_reason"], "evidence_sufficient");
    assert_eq!(
        result["claims"][0]["text"],
        "Returns a pair of string slices or None."
    );
    assert_eq!(result["budget"]["used"]["generations"], 4);
    Ok(())
}

#[tokio::test]
async fn format_repair_saved_g07_and_shared_allowance() -> Result<()> {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/research/g07-format.json"))?;
    let input: Input = serde_json::from_value(json!({"question":fixture["context"]["question"]}))?;
    for blocked in 0..4 {
        let mut session = old_session(&input, &fixture);
        let original: Draft = serde_json::from_value(fixture["draft"].clone())?;
        let mut repaired: Draft = serde_json::from_value(fixture["draft"].clone())?;
        repaired.claims[0].text =
            "str::split_once возвращает None, если разделитель отсутствует.".into();
        session.backend.replies = VecDeque::from(vec![serde_json::to_value(&repaired)?]);
        match blocked {
            1 => session.repaired = true,
            2 => session.budget.counts.generations = 5,
            3 => session.budget.started = Instant::now() - Duration::from_secs(SECONDS - 1),
            _ => {}
        }
        let (context, visible) = session.context(&[])?;
        let draft = session
            .repair_draft_text(original, &context, &visible)
            .await?;
        assert_eq!(draft.claims.len(), usize::from(blocked == 0));
        if blocked == 0 {
            assert!(session.repaired);
            assert_eq!(
                draft.claims[0].evidence[1].quote,
                r#"assert_eq!("cfg".split_once('='), None);"#
            );
            assert!(
                !session.audited,
                "format repair does not grant semantic acceptance"
            );
            assert_eq!(session.budget.counts.generations, 1);
        }
    }
    Ok(())
}

#[tokio::test]
async fn format_repair_failure_keeps_good_claims_and_never_admits_html() -> Result<()> {
    let fixture = old_case("R03");
    let input = input();
    let mut session = old_session(&input, &fixture);
    let (context, visible) = session.context(&[])?;
    let mut original: Draft = serde_json::from_value(fixture["draft"].clone())?;
    session.prune_invalid_claims(&mut original, &visible)?;
    let good = original.claims[0].clone();
    let mut bad = good.clone();
    bad.text = "Vec<Option<T>>".into();
    original.claims = vec![good.clone(), bad.clone()];
    session.backend.replies = VecDeque::from(vec![
        json!({"claims":[{"text":"<script>alert(1)</script>","kind":"fact","rationale":"","facets":good.facets,"evidence":good.evidence}],"next":{"facet":0,"query":""}}),
    ]);
    let result = session
        .repair_draft_text(original, &context, &visible)
        .await?;
    assert_eq!(result.claims.len(), 1);
    assert_eq!(result.claims[0].text, good.text);
    assert!(session.repaired);
    Ok(())
}

#[test]
fn rfc_complete_clause_before_lowercase_paragraph_keeps_required_fact() -> Result<()> {
    let input: Input =
        serde_json::from_value(json!({"question":"RFC 8259 names SHOULD or MUST be unique?"}))?;
    let clause = "The names within an object SHOULD be unique.";
    let mut session = Session::new(&input, Fake::new(), None);
    session.plan.facets = vec!["required modality".into()];
    let body = format!(
        "RFC 8259\n\nA single comma separates a value from a following\n   name.  {clause}\n\n      object = begin-object [ member *( value-separator member ) ] end-object"
    );
    session.chunks.push(Chunk {
        id: "S1P1".into(),
        source_id: "S1".into(),
        document_version: "v1".into(),
        field: "text",
        start: 0,
        end: body.chars().count(),
        text: body.clone(),
    });
    session.sources.push(Source {
        id: "S1".into(),
        url: "https://www.rfc-editor.org/rfc/rfc8259.html".into(),
        document_version: "v1".into(),
        text_hash: String::new(),
        title: "RFC 8259".into(),
        publication: Value::Null,
        eligible: true,
        eligibility: "read_body".into(),
        parent_url: None,
        cache: Value::Null,
        extraction: Value::Null,
        rfc: Some((8259, body.clone())),
    });
    let visible = HashSet::from(["S1P1".to_owned()]);
    let draft: Draft = serde_json::from_value(
        json!({"claims":[{"text":format!("RFC 8259: {clause}"),"kind":"fact","rationale":"","facets":[0],"evidence":[{"chunk_id":"S1P1","quote":clause}]}],"next":{"facet":0,"query":""}}),
    )?;
    session.validate_draft(&draft, &visible)?;
    let (sufficient, _) = session.apply_audit(
        serde_json::from_value(serde_json::to_value(&draft)?)?,
        serde_json::from_value(audit(1, &[], true))?,
        &visible,
        &visible,
    )?;
    assert!(!sufficient, "one source must not bypass the two-body gate");
    assert!(session.missing.is_empty());
    assert_eq!(session.claims.len(), 1);
    assert_eq!(session.budget.counts.generations, 0);
    for separator in [" ", "\n   "] {
        let inline = body.replace("\n\n      object", &format!("{separator}object"));
        session.sources[0].rfc = Some((8259, inline.clone()));
        session.chunks[0].text = inline;
        assert!(
            session.validate_draft(&draft, &visible).is_err(),
            "lowercase continuation without paragraph boundary remains rejected"
        );
    }
    Ok(())
}

#[test]
fn documentation_language_hint_survives_punctuation() {
    for question in [
        "По документации Rust: Vec::insert вернёт ошибку?",
        "Rust, Vec::insert documentation",
        "По документации /Rust, Vec::insert",
    ] {
        assert!(!api_documentation_candidate(
            question,
            "https://foreign.example/docs/vec/insert"
        ));
        assert!(api_documentation_candidate(
            question,
            "https://doc.rust-lang.org/std/vec/struct.Vec.html"
        ));
    }
}

#[tokio::test]
#[ignore = "explicit frozen F02/F03 model component; no source HTTP; caller serializes model access"]
async fn coverage_live_saved_range_contexts() -> Result<()> {
    let directory = std::path::PathBuf::from(std::env::var("RESEARCH_GATE_REPLAY_DIR")?);
    std::fs::create_dir(&directory)?;
    let corpus = std::path::PathBuf::from(std::env::var("RESEARCH_COVERAGE_CORPUS_DIR")?);
    let arm = std::env::var("RESEARCH_COVERAGE_RANGE_ARM")?;
    ensure!(
        matches!(
            arm.as_str(),
            "baseline" | "candidate" | "coordinates_baseline" | "coordinates_candidate"
        ),
        "unknown component arm"
    );
    let old_suffix = " Use evidence objects {chunk_id,start,end} instead of copying quote text. Select the supplied quote_ranges (chunk-relative Unicode start inclusive/end exclusive); Rust copies the original slice. Never count bytes or rewrite a source range.";
    let suffix = if arm == "baseline" {
        old_suffix
    } else {
        RFC_RANGE_PROMPT
    };
    let fixtures: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/research/coverage-range-contexts.json"
    ))?;
    let key =
        std::path::PathBuf::from(std::env::var("HOME")?).join(".config/web-search/openrouter.key");
    let model = crate::model::Model::from_openrouter(
        &key,
        "deepseek/deepseek-v4.1-flash-1017",
        Some("http://127.0.0.1:10808"),
    )?;
    let mut failed = false;
    for id in ["F02", "F03"] {
        let attempt: Result<()> = async {
            let fixture = &fixtures[id];
            let input: Input = serde_json::from_value(json!({"question":fixture["context"]["question"],"language":"ru"}))?;
            let mut session = old_session(&input, fixture);
            for descriptor in fixture["documents"].as_array().context("documents")? {
                let relative = std::path::Path::new(descriptor["path"].as_str().context("document path")?);
                ensure!(relative.components().all(|c| matches!(c, std::path::Component::Normal(_))), "unsafe fixture path");
                let original: Value = serde_json::from_slice(&std::fs::read(corpus.join(relative))?)?;
                let text = original["text"].as_str().context("original text")?;
                ensure!(crate::cache::hash(text.as_bytes()) == descriptor["text_hash"], "original text hash mismatch");
                let source = session.sources.iter_mut().find(|s| s.id == descriptor["source_id"]).context("fixture source")?;
                ensure!(source.url == original["url"], "source URL mismatch");
                source.text_hash = descriptor["text_hash"].as_str().unwrap().into();
                source.rfc = primary_rfc(&Document {url:source.url.clone(),text:text.into(),meta:original["metadata"].clone(),links:vec![],cache:Value::Null,extraction:Value::Null});
                for chunk in session.chunks.iter().filter(|c| c.source_id == source.id) {
                    ensure!(text.chars().skip(chunk.start).take(chunk.end-chunk.start).collect::<String>() == chunk.text, "original chunk mismatch");
                }
            }
            let context = fixture["context_text"].as_str().context("exact context")?;
            ensure!(serde_json::from_str::<Value>(context)? == fixture["context"], "context mismatch");
            let mut call_context: Value = serde_json::from_str(context)?;
            if arm == "coordinates_candidate" {
                for chunk in call_context["chunks"].as_array_mut().unwrap() {
                    chunk.as_object_mut().unwrap().remove("start");
                    chunk.as_object_mut().unwrap().remove("end");
                }
            }
            let call_context = serde_json::to_string(&call_context)?;
            let visible: HashSet<_> = session.chunks.iter().map(|c| c.id.clone()).collect();
            crate::save(&directory.join(format!("{id}-inputs.json")), &json!({"arm":arm,"prompt":format!("{RULES}\n{DRAFT_PROMPT}{suffix}"),"context":serde_json::from_str::<Value>(&call_context)?,"documents":fixture["documents"]}))?;
            let call = model.research_call(&format!("{RULES}\n{DRAFT_PROMPT}{suffix}"), &call_context, serde_json::to_value(schemars::schema_for!(Draft<QuoteRange>))?, 3072, Duration::from_secs(90)).await?;
            crate::save(&directory.join(format!("{id}-draft.json")), &call)?;
            let selected: Draft<QuoteRange> = serde_json::from_str(call["response"]["choices"][0]["message"]["content"].as_str().context("draft JSON or refusal")?)?;
            let mut draft = session.select_quotes(selected);
            crate::save(&directory.join(format!("{id}-selected.json")), &serde_json::to_value(&draft)?)?;
            session.prune_invalid_claims(&mut draft, &visible)?;
            crate::save(&directory.join(format!("{id}-pruned.json")), &serde_json::to_value(&draft)?)?;
            if !draft.claims.is_empty() && !arm.starts_with("coordinates_") {
                let (audit_context, audit_visible, conflict_visible) = session.audit_context(&draft.claims)?;
                let call = model.research_call(&format!("{RULES}\n{AUDIT_PROMPT}"), &audit_context, serde_json::to_value(schemars::schema_for!(Audit))?, 3072, Duration::from_secs(90)).await?;
                crate::save(&directory.join(format!("{id}-audit.json")), &call)?;
                let audit: Audit = serde_json::from_str(call["response"]["choices"][0]["message"]["content"].as_str().context("audit JSON or refusal")?)?;
                session.apply_audit(draft, audit, &audit_visible, &conflict_visible)?;
            }
            crate::save(&directory.join(format!("{id}-result.json")), &session.result("component_replay", None))?;
            Ok(())
        }.await;
        if let Err(error) = attempt {
            failed = true;
            crate::save(
                &directory.join(format!("{id}-failure.json")),
                &json!({"arm":arm,"error":error.to_string(),"retried":false}),
            )?;
            let message = error.to_string().to_ascii_lowercase();
            if [
                "openrouter http 401",
                "openrouter http 403",
                "openrouter http 429",
            ]
            .iter()
            .any(|reason| message.contains(reason))
            {
                return Err(error);
            }
        }
    }
    ensure!(
        !failed,
        "component attempt failed; inspect saved failure/raw calls; no retry"
    );
    Ok(())
}

fn quote_audit_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../tests/fixtures/research/quote-audit-g09.json"
    ))
    .unwrap()
}

#[test]
fn audit_support_is_bound_to_returned_witness_not_other_example_lines() -> Result<()> {
    let fixture = quote_audit_fixture();
    let input: Input = serde_json::from_value(json!({"question":fixture["context"]["question"]}))?;
    let session = old_session(&input, &fixture);
    let claims: Vec<Claim> = fixture["context"]["claims"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| serde_json::from_value(r["claim"].clone()))
        .collect::<serde_json::Result<_>>()?;
    assert_eq!(fixture["old_audit"]["checks"][1]["verdict"], "supported");
    let (context, visible, _) = session.audit_context(&claims)?;
    let context: Value = serde_json::from_str(&context)?;
    assert!(context.get("chunks").is_none());
    let evidence = &context["claims"][1]["claim"]["evidence"][0];
    assert_eq!(evidence["quote"], "assert_eq!(vec.pop(), Some(3));");
    assert_eq!(evidence["source_id"], "S1");
    assert!(visible.contains("S1P1"));
    assert!(!serde_json::to_string(evidence)?.contains("[1, 2]"));
    assert!(serde_json::to_string(&context["rejection_context"])?.contains("[1, 2]"));
    assert_eq!(context["question"], fixture["context"]["question"]);
    let chunk = session.chunks.iter().find(|c| c.id == "S1P1").unwrap();
    let start = evidence["start"].as_u64().unwrap() as usize - chunk.start;
    assert_eq!(
        chunk
            .text
            .chars()
            .skip(start)
            .take(evidence["quote"].as_str().unwrap().chars().count())
            .collect::<String>(),
        evidence["quote"]
    );
    Ok(())
}

#[test]
fn audit_support_survives_background_trimming_and_keeps_question_premises() -> Result<()> {
    let fixture = quote_audit_fixture();
    let input: Input = serde_json::from_value(
        json!({"question":"For Vec::pop on vec![1,2,3], give result and remaining vector."}),
    )?;
    let mut session = old_session(&input, &fixture);
    let mut claims: Vec<Claim> = fixture["context"]["claims"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| serde_json::from_value(r["claim"].clone()))
        .collect::<serde_json::Result<_>>()?;
    let rule = claims[0].evidence[0].clone();
    claims[1].evidence.push(rule);
    claims[1].kind = Kind::Inference;
    claims[1].rationale =
        "Instantiate the quoted removal rule with the vector explicitly supplied by the question."
            .into();
    // Fill the existing context just below its cap; resolved support metadata then forces trimming.
    let original_len = session.context(&claims)?.0.chars().count();
    let padding = 23_980usize.saturating_sub(original_len);
    session.chunks[0].text.push_str(&"x".repeat(padding));
    session.chunks[0].end += padding;
    let before = session.context(&claims)?.0;
    let before: Value = serde_json::from_str(&before)?;
    assert_eq!(
        before["chunks"].as_array().unwrap().len(),
        session.chunks.len()
    );
    let original = session.chunks[0].text.clone();
    let (context, visible, _) = session.audit_context(&claims)?;
    assert!(context.chars().count() <= 24_000);
    let context: Value = serde_json::from_str(&context)?;
    assert_eq!(context["question"], input.question);
    assert_eq!(
        context["claims"][1]["claim"]["evidence"][0]["quote"],
        "assert_eq!(vec.pop(), Some(3));"
    );
    assert!(visible.contains("S1P1"));
    assert!(context["rejection_context"].as_array().unwrap().len() < session.chunks.len());
    assert_eq!(
        session.chunks[0].text, original,
        "do not synthesize or mutate source chunks"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit single saved G09 audit call per arm; no source HTTP; root serializes"]
async fn quote_bound_live_saved_audit() -> Result<()> {
    let directory = std::path::PathBuf::from(std::env::var("RESEARCH_GATE_REPLAY_DIR")?);
    std::fs::create_dir(&directory)?;
    let arm = std::env::var("RESEARCH_QUOTE_AUDIT_ARM")?;
    ensure!(
        matches!(arm.as_str(), "baseline" | "candidate"),
        "unknown audit arm"
    );
    let fixture = quote_audit_fixture();
    let input: Input = serde_json::from_value(json!({"question":fixture["context"]["question"]}))?;
    let mut session = old_session(&input, &fixture);
    let claims: Vec<Claim> = fixture["context"]["claims"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| serde_json::from_value(r["claim"].clone()))
        .collect::<serde_json::Result<_>>()?;
    let (context, visible, conflict_visible, prompt) = if arm == "baseline" {
        (
            fixture["context_text"].as_str().unwrap().to_owned(),
            session.chunks.iter().map(|c| c.id.clone()).collect(),
            session.chunks.iter().map(|c| c.id.clone()).collect(),
            fixture["old_prompt"].as_str().unwrap().to_owned(),
        )
    } else {
        let (context, visible, conflict_visible) = session.audit_context(&claims)?;
        (
            context,
            visible,
            conflict_visible,
            format!("{RULES}\n{AUDIT_PROMPT}"),
        )
    };
    crate::save(
        &directory.join("inputs.json"),
        &json!({"arm":arm,"prompt":prompt,"context":serde_json::from_str::<Value>(&context)?}),
    )?;
    let key =
        std::path::PathBuf::from(std::env::var("HOME")?).join(".config/web-search/openrouter.key");
    let model = crate::model::Model::from_openrouter(
        &key,
        "deepseek/deepseek-v4.1-flash-1017",
        Some("http://127.0.0.1:10808"),
    )?;
    let call = model
        .research_call(
            &prompt,
            &context,
            serde_json::to_value(schemars::schema_for!(Audit))?,
            3072,
            Duration::from_secs(90),
        )
        .await?;
    crate::save(&directory.join("audit.json"), &call)?;
    let audit: Audit = serde_json::from_str(
        call["response"]["choices"][0]["message"]["content"]
            .as_str()
            .context("audit JSON or refusal; raw call retained; no retry")?,
    )?;
    session.apply_audit(
        Draft {
            claims,
            next: FollowUp::default(),
        },
        audit,
        &visible,
        &conflict_visible,
    )?;
    crate::save(
        &directory.join("result.json"),
        &session.result("component_replay", None),
    )?;
    Ok(())
}

#[test]
fn generative_chunks_omit_document_offsets_but_witnesses_keep_them() -> Result<()> {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/research/coverage-range-contexts.json"
    ))?;
    for id in ["F02", "F03"] {
        let fixture = &fixtures[id];
        let input: Input =
            serde_json::from_value(json!({"question":fixture["context"]["question"]}))?;
        let session = old_session(&input, fixture);
        let (context, visible) = session.context(&[])?;
        let context: Value = serde_json::from_str(&context)?;
        for (original, chunk) in fixture["context"]["chunks"]
            .as_array()
            .unwrap()
            .iter()
            .zip(context["chunks"].as_array().unwrap())
        {
            let mut expected = original.clone();
            assert!(expected.as_object_mut().unwrap().remove("start").is_some());
            assert!(expected.as_object_mut().unwrap().remove("end").is_some());
            assert_eq!(
                &expected, chunk,
                "only document-coordinate fields are omitted"
            );
            let internal = session.chunks.iter().find(|c| c.id == chunk["id"]).unwrap();
            assert_eq!(internal.start, original["start"].as_u64().unwrap() as usize);
            assert_eq!(internal.end, original["end"].as_u64().unwrap() as usize);
            let selected_start = 12;
            let quote: String = internal
                .text
                .chars()
                .skip(selected_start)
                .take(20)
                .collect();
            let witness = session.quote(
                &Quote {
                    chunk_id: internal.id.clone(),
                    quote: quote.clone(),
                    selected_start: Some(selected_start),
                },
                &visible,
            )?;
            assert_eq!(witness["start"], internal.start + selected_start);
            assert_eq!(
                witness["end"],
                internal.start + selected_start + quote.chars().count()
            );
            assert_eq!(witness["document_version"], internal.document_version);
            assert_eq!(witness["quote"], quote);
            assert!(
                witness.get("chunk_id").is_none(),
                "internal audit IDs must not change public witness shape"
            );
        }
    }
    Ok(())
}

#[test]
fn reviewer_bounded_audit_unseen_conflict_quote() -> Result<()> {
    let input = Input {
        question: "q".repeat(2000),
        language: Language::Auto,
        cache_mode: CacheMode::default(),
    };
    input.validate()?;
    let mut session = Session::new(&input, Fake::new(), None);
    session.plan.facets = (0..4)
        .map(|i| format!("Facet{i} {}", "f".repeat(100)))
        .collect();
    for source in 0..4 {
        session.sources.push(Source {
            id: format!("S{source}"),
            url: format!("https://example.org/{source}/{}", "p".repeat(960)),
            document_version: "a".repeat(64),
            text_hash: "b".repeat(64),
            title: "t".repeat(120),
            publication: Value::Null,
            eligible: true,
            eligibility: "read_body".into(),
            parent_url: None,
            cache: Value::Null,
            extraction: Value::Null,
            rfc: None,
        });
        for part in 0..3 {
            let id = format!("S{source}P{part}");
            let mut text = format!("VISIBLE_{id}_{}", "x".repeat(600));
            text.push_str(&format!("HIDDEN_{id}_CONTRADICTION_WITNESS."));
            text.push_str(&"y".repeat(1200 - text.len()));
            session.chunks.push(Chunk {
                id,
                source_id: format!("S{source}"),
                document_version: "a".repeat(64),
                field: "text",
                start: part * 1200,
                end: (part + 1) * 1200,
                text,
            });
        }
    }
    let claims: Vec<Claim> = (0..4)
        .map(|i| Claim {
            text: format!("Claim{i} {}", "c".repeat(225)),
            kind: Kind::Fact,
            rationale: String::new(),
            facets: vec![i],
            evidence: [1, 2]
                .into_iter()
                .map(|part| {
                    let c = &session.chunks[i * 3 + part];
                    Quote {
                        chunk_id: c.id.clone(),
                        quote: c.text.chars().take(360).collect(),
                        selected_start: None,
                    }
                })
                .collect(),
        })
        .collect();
    assert!(
        session
            .chunks
            .iter()
            .all(|c| c.text.chars().count() <= 1200)
    );
    let (context, visible, conflict_visible) = session.audit_context(&claims)?;
    let val: Value = serde_json::from_str(&context)?;
    let shown: HashSet<_> = val["rejection_context"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    let hidden = session
        .chunks
        .iter()
        .find(|c| visible.contains(&c.id) && !shown.contains(c.id.as_str()))
        .context("no bounded required chunk was trimmed")?;
    let quote = format!("HIDDEN_{}_CONTRADICTION_WITNESS.", hidden.id);
    assert!(!context.contains(&quote));
    let left = Quote {
        chunk_id: hidden.id.clone(),
        quote,
        selected_start: None,
    };
    let accepted = session.quote(&left, &visible);
    eprintln!(
        "BOUNDED context={} chunks={} hidden_chunk={} quote_accepted={}",
        context.chars().count(),
        session.chunks.len(),
        hidden.id,
        accepted.is_ok()
    );
    let right_chunk = session
        .chunks
        .iter()
        .find(|c| c.source_id != hidden.source_id && visible.contains(&c.id))
        .unwrap();
    let right = Quote {
        chunk_id: right_chunk.id.clone(),
        quote: right_chunk.text.chars().take(360).collect(),
        selected_start: None,
    };
    let audit = Audit {
        checks: (0..4)
            .map(|i| Check {
                id: claim_id(i),
                verdict: Verdict::Supported,
                reason: "Supported.".into(),
            })
            .collect(),
        conflicts: vec![Conflict {
            facet: 0,
            left,
            right,
            reason: "Conflicting policies.".into(),
        }],
        missing: vec![],
        sufficient: false,
        next: FollowUp::default(),
    };
    let mut shown_audit: Audit = serde_json::from_value(serde_json::to_value(&audit)?)?;
    shown_audit.conflicts[0].left = claims
        .iter()
        .flat_map(|c| &c.evidence)
        .find(|q| q.chunk_id == hidden.id)
        .unwrap()
        .clone();
    let shown_draft = Draft {
        claims: claims.clone(),
        next: FollowUp::default(),
    };
    let applied = session.apply_audit(
        Draft {
            claims,
            next: FollowUp::default(),
        },
        audit,
        &visible,
        &conflict_visible,
    );
    eprintln!(
        "AUDIT conflict from unseen text accepted={}",
        applied.is_ok()
    );
    assert!(
        applied.is_err(),
        "Unseen non-support quote must not become eligible merely because same chunk ID has mandatory support"
    );
    assert!(
        session
            .apply_audit(shown_draft, shown_audit, &visible, &conflict_visible)
            .is_ok(),
        "the exact mandatory support witness remains visible even if its full chunk was trimmed"
    );
    Ok(())
}

#[test]
fn declared_modern_rfc_extraction_keeps_own_body_identity() -> Result<()> {
    let url = "https://www.rfc-editor.org/rfc/rfc9111.html";
    let value = crate::extract(
        include_str!("../tests/fixtures/extract/rfc9111-declared-body.html").into(),
        url,
    )?;
    let doc = Document {
        url: url.into(),
        text: value["text"].as_str().unwrap().into(),
        meta: json!({"title":value["title"]}),
        links: vec![],
        cache: Value::Null,
        extraction: Value::Null,
    };
    assert_eq!(primary_rfc(&doc).map(|p| p.0), Some(9111));
    Ok(())
}

#[test]
fn rfc_sentence_ranges_recognize_only_delimited_pilcrow() -> Result<()> {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/research/coverage-range-contexts.json"
    ))?;
    let chunk = &fixtures["F02"]["context"]["chunks"][0];
    let text = chunk["text"].as_str().unwrap();
    assert_eq!(
        text.chars().skip(643).take(99).collect::<String>(),
        "All 1xx (Informational), 204 (No Content), and 304 (Not Modified) responses do not include content."
    );
    assert!(sentence_ranges(text).contains(&(643, 742)));
    let clause = "The cache MUST NOT store this response.";
    for suffix in ["¶", "¶\n\nNext clause.", "¶ Next clause."] {
        assert!(
            sentence_ranges(&format!("{clause}{suffix}")).contains(&(0, clause.chars().count()))
        );
    }
    assert!(
        !sentence_ranges(&format!("{clause}¶continuation")).contains(&(0, clause.chars().count()))
    );
    assert!(!sentence_ranges("A client MUST NOT negotiate TLS 1.2.").contains(&(0, 33)));
    Ok(())
}

#[tokio::test]
async fn rfc_exact_witness_ends_before_pilcrow_without_changing_source() -> Result<()> {
    let input: Input =
        serde_json::from_value(json!({"question":"RFC 9110: can204 responses have content?"}))?;
    let sentence = "All 1xx (Informational), 204 (No Content), and 304 (Not Modified) responses do not include content.";
    let mut fake = Fake::new();
    let doc = fake.docs.get_mut("https://source1.example/doc").unwrap();
    doc.url = "https://www.rfc-editor.org/rfc/rfc9110.html".into();
    doc.text = format!(
        "RFC 9110\n\n{sentence}¶\n\nAll other responses do include content, although that content might be of zero length.¶"
    );
    doc.meta = json!({"title":"RFC 9110"});
    let mut session = Session::new(&input, fake, None);
    session.plan.facets = vec!["204 content".into()];
    session.queue.push_back(Target {
        url: "https://source1.example/doc".into(),
        parent: None,
    });
    session.read_round().await?;
    let visible = session.chunks.iter().map(|c| c.id.clone()).collect();
    let draft: Draft = serde_json::from_value(
        json!({"claims":[{"text":format!("RFC 9110: {sentence}"),"kind":"fact","rationale":"","facets":[0],"evidence":[{"chunk_id":"S1P1","quote":sentence}]}],"next":{"facet":0,"query":""}}),
    )?;
    session.validate_draft(&draft, &visible)?;
    let witness = session.quote(&draft.claims[0].evidence[0], &visible)?;
    assert_eq!(witness["quote"], sentence);
    let end = witness["end"].as_u64().unwrap() as usize;
    assert_eq!(
        session.sources[0].rfc.as_ref().unwrap().1.chars().nth(end),
        Some('¶')
    );
    let original = session.sources[0].rfc.as_ref().unwrap().1.clone();
    for continuation in [".¶continuation All other", ".¶ continuation All other"] {
        session.sources[0].rfc.as_mut().unwrap().1 =
            original.replace(".¶\n\nAll other", continuation);
        assert!(session.validate_draft(&draft, &visible).is_err());
    }
    Ok(())
}

#[tokio::test]
async fn repeated_followup_query_targets_missing_facet() -> Result<()> {
    let input = input();
    let mut fake = Fake::new();
    fake.replies = VecDeque::from(vec![
        json!({"facets":["Condition","Exception"],"query":"protocol conditions"}),
        json!({"claims":[claim("Первое условие подтверждено.",0,&[1,2])],"next":{"facet":1,"query":"protocol conditions"}}),
        json!({"checks":[{"id":"A1","verdict":"supported","reason":"Supported."}],"conflicts":[],"missing":[1],"sufficient":false,"next":{"facet":1,"query":"protocol conditions"}}),
        json!({"claims":[claim("Первое условие подтверждено.",0,&[1,2]),claim("Исключение подтверждено отдельно.",1,&[3,4])],"next":{"facet":1,"query":""}}),
        audit(2, &[], true),
    ]);
    let mut session = Session::new(&input, fake, None);
    assert_eq!(session.work().await?, "evidence_sufficient");
    assert_eq!(
        session.forms,
        vec!["protocol conditions", "protocol conditions Exception"]
    );
    assert_eq!(session.budget.counts.forms, 2);
    assert_eq!(session.budget.counts.generations, 5);
    Ok(())
}

#[test]
fn missing_followup_preserves_scope_and_available_queue_choice() -> Result<()> {
    for (question, initial, facet) in [
        (
            "Rust str::split_once documentation",
            "Rust str::split_once documentation",
            "absence of delimiter return contract",
        ),
        (
            "Rust Vec::insert documentation",
            "Rust Vec::insert documentation",
            "panic when index exceeds length",
        ),
        (
            "RFC 6585 status429",
            "RFC 6585 status429",
            "requirement to send 429",
        ),
    ] {
        let mut input = input();
        input.question = question.into();
        let mut session = Session::new(&input, Fake::new(), None);
        session.plan.facets = vec![facet.into()];
        session.missing = vec![0];
        session.forms = vec![initial.into()];
        let next = FollowUp {
            facet: 0,
            query: initial.into(),
        };
        assert_eq!(
            session.followup_query(&next)?,
            Some(format!("{initial} {facet}"))
        );
        assert_eq!(session.followup_query(&FollowUp::default())?, None);
    }
    Ok(())
}

#[test]
fn missing_followup_keeps_site_path_and_version() -> Result<()> {
    let mut input = input();
    input.question = "Rust1.90 Vec::insert site:doc.rust-lang.org/1.90.0".into();
    let mut session = Session::new(&input, Fake::new(), None);
    session.plan.facets = vec!["panic when index exceeds length".into()];
    session.missing = vec![0];
    let first = session.query("Rust1.90 Vec::insert")?;
    session.forms = vec![first.clone()];
    let focused = session
        .followup_query(&FollowUp {
            facet: 0,
            query: first,
        })?
        .unwrap();
    assert!(focused.contains("Rust1.90 Vec::insert"));
    assert!(focused.ends_with("site:doc.rust-lang.org/1.90.0"));
    assert!(focused.contains("panic when index exceeds length"));
    Ok(())
}
#[path = "research_inference_experiment.rs"]
mod inference_experiment;

#[path = "research_section_pipeline_experiment.rs"]
mod section_pipeline_experiment;

#[test]
fn dotted_api_documentation_candidates_keep_language_and_symbol() {
    assert!(api_documentation_candidate(
        "Python pathlib.Path.resolve documentation",
        "https://docs.python.org/3/library/pathlib.html#pathlib.Path.resolve"
    ));
    let question = "According to Python itertools.groupby documentation, what does it group?";
    assert!(api_documentation_candidate(
        question,
        "https://docs.python.org/3/library/itertools.html#itertools.groupby"
    ));
    assert!(api_documentation_candidate(
        question,
        "https://docs.python.org/3/library/itertools.html"
    ));
    for url in [
        "https://docs.rs/itertools/latest/itertools/",
        "https://docs.python.org/3/library/itertools.html#itertools.product",
        "https://docs.python.org/3/library/functools.html",
    ] {
        assert!(!api_documentation_candidate(question, url), "{url}");
    }
    assert!(!api_documentation_candidate(
        "Python docs.python.org documentation",
        "https://docs.python.org/3/library/docs.html"
    ));
}

#[test]
fn release_target_mismatch_cannot_close_facets_after_supported_audit() -> Result<()> {
    assert_eq!(
        release_versions("Engine v1.2.3"),
        HashSet::from(["1.2.3".into()])
    );
    assert!(release_versions("2026.11.18 18.11.2026 HTTP/2.0 RFC 9110").is_empty());

    let input: Input = serde_json::from_value(
        json!({"question":"For Engine 8.2.0b2, is it production ready and when is the next beta?"}),
    )?;
    let fixture = json!({"context":{
        "facets":["production readiness", "next beta"],
        "sources":[{"id":"S1","url_preview":"https://example.org/release","title":"Engine 8.2.0b1","eligible":true,"publication":null}],
        "chunks":[{"id":"S1P1","source_id":"S1","document_version":"v1","start":0,"end":100,"text":"This preview is not production ready. The next beta, Engine 8.2.0b2, is scheduled for November 10."}]
    }});
    let mut session = old_session(&input, &fixture);
    let visible = HashSet::from(["S1P1".into()]);
    let claim = |text: &str, quote: &str, facet| json!({"text":text,"kind":"fact","rationale":"","facets":[facet],"evidence":[{"chunk_id":"S1P1","quote":quote}]});
    let draft = json!({"claims":[
        claim("It is not production ready.", "This preview is not production ready.",0),
        claim("The next beta is scheduled for November 10.", "The next beta, Engine 8.2.0b2, is scheduled for November 10.",1)
    ],"next":{"facet":0,"query":""}});
    let judge: Audit = serde_json::from_value(audit(2, &[], true))?;
    session.apply_audit(
        serde_json::from_value(draft.clone())?,
        judge,
        &visible,
        &visible,
    )?;
    assert!(
        session.claims.is_empty(),
        "a stale release title cannot establish the requested release's status or next date"
    );
    assert_eq!(session.missing, vec![0, 1]);
    for question in [
        "Compare Engine 8.2.0b1 with 8.2.0b2.",
        "For Engine 8.2, what is the documented preview policy?",
        "What changed on 2026.11.18 in HTTP/2.0 and RFC 9110?",
    ] {
        let other: Input = serde_json::from_value(json!({"question":question}))?;
        let other_session = old_session(&other, &fixture);
        let old_draft: Draft = serde_json::from_value(draft.clone())?;
        assert!(
            other_session.release_scope_matches(&old_draft.claims[0]),
            "{question}"
        );
    }
    session.sources[0].title = "Engine release notes".into();
    let old_draft: Draft = serde_json::from_value(draft.clone())?;
    assert!(
        session.release_scope_matches(&old_draft.claims[0]),
        "an unversioned title is not a mismatch"
    );
    session.sources[0].title = "Engine 8.2.0b2".into();
    session.apply_audit(
        serde_json::from_value(draft)?,
        serde_json::from_value(audit(2, &[], true))?,
        &visible,
        &visible,
    )?;
    assert_eq!(
        session.claims.len(),
        2,
        "matching release remains eligible for semantic audit"
    );
    Ok(())
}

#[path = "research_scope_audit_experiment.rs"]
mod scope_audit_experiment;

#[path = "research_primary_search_experiment.rs"]
pub(super) mod primary_search_experiment;

#[tokio::test]
async fn audit_advisory_reason_overflow_must_not_drop_verified_claims() -> Result<()> {
    let fixture: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/research/oversized-audit-reason.json"
    ))?;
    for reason in [
        fixture["reason"].as_str().unwrap().to_owned(),
        "🧪".repeat(201),
        " ".repeat(250) + "Explanation",
    ] {
        let input = input();
        let mut session = Session::new(&input, Fake::new(), None);
        session.plan.facets = vec!["Condition".into()];
        for i in 1..=2 {
            session.queue.push_back(Target {
                url: format!("https://source{i}.example/doc"),
                parent: None,
            });
        }
        session.read_round().await?;
        let draft: Draft = serde_json::from_value(
            json!({"claims":[claim("Подтверждённое условие.",0,&[1,2])],"next":{"facet":0,"query":""}}),
        )?;
        let mut raw = audit(1, &[], true);
        raw["checks"][0]["reason"] = json!(reason);
        let parsed: Audit = serde_json::from_value(raw.clone())?;
        let visible = session.chunks.iter().map(|c| c.id.clone()).collect();
        assert!(session.apply_audit(draft, parsed, &visible, &visible)?.0);
        assert_eq!(session.claims.len(), 1);
        assert_eq!(raw["checks"][0]["reason"], reason);
    }
    Ok(())
}

#[tokio::test]
async fn audit_advisory_guards_and_schema_remain_strict() -> Result<()> {
    let schema = serde_json::to_value(schemars::schema_for!(Check))?;
    assert_eq!(schema["properties"]["reason"]["maxLength"], 200);
    for mode in [
        "empty",
        "blank",
        "late_control",
        "unknown_id",
        "invalid_verdict",
        "bad_quote",
        "unsupported",
    ] {
        let input = input();
        let mut session = Session::new(&input, Fake::new(), None);
        session.plan.facets = vec!["Condition".into()];
        for i in 1..=2 {
            session.queue.push_back(Target {
                url: format!("https://source{i}.example/doc"),
                parent: None,
            });
        }
        session.read_round().await?;
        let mut draft: Draft = serde_json::from_value(
            json!({"claims":[claim("Подтверждённое условие.",0,&[1,2])],"next":{"facet":0,"query":""}}),
        )?;
        let mut raw = audit(1, &[], true);
        raw["checks"][0]["reason"] = json!("x".repeat(201));
        match mode {
            "empty" => raw["checks"][0]["reason"] = json!(""),
            "blank" => raw["checks"][0]["reason"] = json!(" ".repeat(201)),
            "late_control" => raw["checks"][0]["reason"] = json!("x".repeat(201) + "\n"),
            "unknown_id" => raw["checks"][0]["id"] = json!("A99"),
            "invalid_verdict" => raw["checks"][0]["verdict"] = json!("probably"),
            "bad_quote" => {
                draft.claims[0].evidence[0].quote = "This quote is not in the source.".into()
            }
            "unsupported" => raw["checks"][0]["verdict"] = json!("unsupported"),
            _ => unreachable!(),
        }
        let parsed = serde_json::from_value::<Audit>(raw);
        if mode == "invalid_verdict" {
            assert!(parsed.is_err());
            continue;
        }
        let visible = session.chunks.iter().map(|c| c.id.clone()).collect();
        let result = session.apply_audit(draft, parsed?, &visible, &visible);
        if mode == "unsupported" {
            assert!(!result?.0);
            assert!(session.claims.is_empty());
        } else {
            assert!(result.is_err(), "{mode}");
        }
    }
    Ok(())
}

#[test]
#[ignore = "explicit offline replay of saved source-aware C01 audit; no model or network"]
fn audit_advisory_saved_201_replay() -> Result<()> {
    let path = std::path::PathBuf::from(std::env::var("AUDIT_REASON_REPLAY_INPUT")?);
    let output = std::path::PathBuf::from(std::env::var("AUDIT_REASON_REPLAY_OUTPUT")?);
    ensure!(!output.exists(), "replay output already exists");
    let fixture: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let input: Input = serde_json::from_value(
        json!({"question":fixture["context"]["question"],"language":fixture["context"]["language"]}),
    )?;
    let mut session = old_session(&input, &fixture);
    let draft: Draft = serde_json::from_value(fixture["draft"].clone())?;
    let audit: Audit = serde_json::from_value(fixture["audit"].clone())?;
    assert_eq!(audit.checks[3].reason.chars().count(), 201);
    assert_eq!(draft.claims.len(), 4);
    let (_, visible, conflicts) = session.audit_context(&draft.claims)?;
    assert!(
        !session.apply_audit(draft, audit, &visible, &conflicts)?.0,
        "one document still cannot satisfy two-document gate"
    );
    let result = session.result("offline_replay", None);
    assert_eq!(result["claims"].as_array().unwrap().len(), 4);
    assert_eq!(result["sufficiency"]["cited_distinct_documents"], 1);
    assert_eq!(result["sufficiency"]["audit_completed"], true);
    assert_eq!(result["status"], "partial");
    assert!(
        session
            .actions
            .iter()
            .any(|a| a["action"] == "audit_reason_truncated"
                && a["original_chars"] == 201
                && a["retained_chars"] == 200)
    );
    crate::save(&output, &result)?;
    Ok(())
}
