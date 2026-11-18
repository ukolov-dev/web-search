//! Explicit saved-material experiment; no runtime wiring or source HTTP.
use super::*;

const SCOPE_PROMPT: &str = "Use the scoped checks schema INSTEAD of verdict checks. For every known claim ID return object_version, time, conditions, own_quote_support, reason. Each axis is supported, unsupported, unknown, or not_applicable. object_version checks the exact entity/API and requested version; time checks announcement/event date, historical/current scope and relative schedules; conditions checks qualifiers, preconditions, exceptions, negation and guarantees. not_applicable is allowed ONLY when that axis is irrelevant to BOTH question and claim; missing required evidence is unknown, never not_applicable. own_quote_support checks the WHOLE proposition against ONLY its own exact evidence and explicit original-question premises; it must be supported to keep a claim and must never be not_applicable. rejection_context and source metadata may reject, disambiguate or reveal missing evidence, but MUST NOT supply missing support. Keep the claim and its evidence frozen; do not rewrite either or replace the proposition with an extractive statement. unknown/unsupported on any applicable axis prevents keeping the claim. Supply ordinary missing/conflicts/sufficient/next fields with the same constraints as the original audit; reasons <=200 characters.";

#[derive(Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Axis {
    Supported,
    Unsupported,
    Unknown,
    NotApplicable,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ScopeCheck {
    id: String,
    object_version: Axis,
    time: Axis,
    conditions: Axis,
    own_quote_support: Axis,
    reason: String,
}
impl ScopeCheck {
    fn keep(&self) -> bool {
        self.own_quote_support == Axis::Supported
            && [self.object_version, self.time, self.conditions]
                .iter()
                .all(|axis| matches!(axis, Axis::Supported | Axis::NotApplicable))
    }
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ScopeAudit {
    checks: Vec<ScopeCheck>,
    conflicts: Vec<Conflict>,
    missing: Vec<usize>,
    sufficient: bool,
    next: FollowUp,
}
impl ScopeAudit {
    fn into_audit(self, count: usize) -> Result<Audit> {
        let expected: HashSet<_> = (0..count).map(claim_id).collect();
        let actual: HashSet<_> = self.checks.iter().map(|c| c.id.clone()).collect();
        ensure!(
            count <= 4 && actual == expected && actual.len() == self.checks.len(),
            "scoped audit must check every claim exactly once"
        );
        Ok(Audit {
            sufficient: self.sufficient && self.checks.iter().all(ScopeCheck::keep),
            checks: self
                .checks
                .into_iter()
                .map(|check| Check {
                    verdict: if check.keep() {
                        Verdict::Supported
                    } else {
                        Verdict::Unsupported
                    },
                    id: check.id,
                    reason: check.reason,
                })
                .collect(),
            conflicts: self.conflicts,
            missing: self.missing,
            next: self.next,
        })
    }
}

fn validate_response(call: &Value) -> Result<()> {
    let choice = &call["response"]["choices"][0];
    let message = &choice["message"];
    ensure!(
        choice["finish_reason"] == "stop",
        "incomplete model response; no retry"
    );
    ensure!(
        message
            .get("refusal")
            .is_none_or(|v| v.is_null() || v == ""),
        "model refusal; no retry"
    );
    ensure!(
        message
            .get("tool_calls")
            .is_none_or(|v| v.is_null() || v.as_array().is_some_and(Vec::is_empty)),
        "unexpected model tool call; no retry"
    );
    Ok(())
}

#[test]
fn scope_response_gate_requires_complete_nonrefused_text() {
    let call =
        json!({"response":{"choices":[{"finish_reason":"stop","message":{"content":"{}"}}]}});
    assert!(validate_response(&call).is_ok());
    for change in ["length", "content_filter"] {
        let mut changed = call.clone();
        changed["response"]["choices"][0]["finish_reason"] = json!(change);
        assert!(validate_response(&changed).is_err());
    }
    for field in ["refusal", "tool_calls"] {
        let mut changed = call.clone();
        changed["response"]["choices"][0]["message"][field] = json!("unexpected");
        assert!(validate_response(&changed).is_err());
    }
}

#[test]
fn scope_axis_gate_rejects_unknown_and_requires_own_support() -> Result<()> {
    let make = || ScopeCheck {
        id: "A1".into(),
        object_version: Axis::Supported,
        time: Axis::NotApplicable,
        conditions: Axis::Supported,
        own_quote_support: Axis::Supported,
        reason: "own witness only".into(),
    };
    assert!(make().keep());
    for axis in 0..4 {
        for bad in [Axis::Unknown, Axis::Unsupported] {
            let mut check = make();
            match axis {
                0 => check.object_version = bad,
                1 => check.time = bad,
                2 => check.conditions = bad,
                _ => check.own_quote_support = bad,
            }
            assert!(!check.keep());
        }
    }
    let mut check = make();
    check.own_quote_support = Axis::NotApplicable;
    assert!(!check.keep());
    let audit = |checks| ScopeAudit {
        checks,
        conflicts: vec![],
        missing: vec![],
        sufficient: true,
        next: FollowUp::default(),
    };
    assert!(audit(vec![make(), make()]).into_audit(1).is_err());
    assert!(audit(vec![]).into_audit(1).is_err());
    let converted = audit(vec![check]).into_audit(1)?;
    assert!(converted.checks[0].verdict == Verdict::Unsupported);
    assert!(!converted.sufficient);
    assert!(audit(vec![make()]).into_audit(1)?.checks[0].verdict == Verdict::Supported);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit one saved audit per arm; prepare is offline; root freezes and serializes"]
async fn scope_saved_audit_experiment() -> Result<()> {
    let fixture_path = std::env::var("SCOPE_AUDIT_INPUT")?;
    let directory = std::path::PathBuf::from(std::env::var("SCOPE_AUDIT_OUTPUT")?);
    let arm = std::env::var("SCOPE_AUDIT_ARM")?;
    ensure!(
        matches!(arm.as_str(), "prepare" | "baseline" | "candidate"),
        "unknown scope audit arm"
    );
    std::fs::create_dir(&directory)?;
    let fixture: Value = serde_json::from_slice(&std::fs::read(fixture_path)?)?;
    let input: Input = serde_json::from_value(json!({"question":fixture["context"]["question"]}))?;
    let mut session = old_session(&input, &fixture);
    let claims: Vec<Claim> = fixture["context"]["claims"]
        .as_array()
        .context("claims")?
        .iter()
        .map(|row| serde_json::from_value(row["claim"].clone()))
        .collect::<serde_json::Result<_>>()?;
    let (prepared, visible, conflict_visible) = session.audit_context(&claims)?;
    let baseline_prompt = format!("{RULES}\n{AUDIT_PROMPT}");
    let candidate_prompt = format!("{baseline_prompt}\n{SCOPE_PROMPT}");
    let baseline_schema = serde_json::to_value(schemars::schema_for!(Audit))?;
    let candidate_schema = serde_json::to_value(schemars::schema_for!(ScopeAudit))?;
    if arm == "prepare" {
        let mut visible: Vec<_> = visible.into_iter().collect();
        visible.sort();
        let mut conflict_visible: Vec<_> = conflict_visible.into_iter().collect();
        conflict_visible.sort();
        crate::save(
            &directory.join("prepared-context.json"),
            &json!({"context_text":prepared,"visible":visible,"conflict_visible":conflict_visible,"baseline_prompt":baseline_prompt,"candidate_prompt":candidate_prompt,"baseline_schema":baseline_schema,"candidate_schema":candidate_schema}),
        )?;
        return Ok(()); // No credentials or model construction in prepare.
    }
    let context = fixture["context_text"]
        .as_str()
        .context("frozen context_text required")?;
    // Budgets/clocks in serialized context must be frozen by root, not regenerated per arm.
    let frozen: Value = serde_json::from_str(context)?;
    let fresh: Value = serde_json::from_str(&prepared)?;
    for field in [
        "question",
        "facets",
        "claims",
        "sources",
        "rejection_context",
    ] {
        ensure!(
            frozen[field] == fresh[field],
            "frozen context does not match saved material: {field}"
        );
    }
    ensure!(context.chars().count() <= 24_000, "audit context cap");
    let (prompt, schema) = if arm == "baseline" {
        (baseline_prompt, baseline_schema)
    } else {
        (candidate_prompt, candidate_schema)
    };
    crate::save(
        &directory.join("inputs.json"),
        &json!({"arm":arm,"prompt":prompt,"context_text":context,"schema":schema}),
    )?;
    let key =
        std::path::PathBuf::from(std::env::var("HOME")?).join(".config/web-search/openrouter.key");
    let model = crate::model::Model::from_openrouter(
        &key,
        "deepseek/deepseek-v4.1-flash-1017",
        Some("http://127.0.0.1:10808"),
    )?;
    let call = match model
        .research_call(&prompt, context, schema, 3072, Duration::from_secs(90))
        .await
    {
        Ok(call) => call,
        Err(error) => {
            // Never serialize raw transport errors/bodies or credentials.
            crate::save(
                &directory.join("error.json"),
                &json!({"status":"model_call_failed","retry":false,"http_429":error.to_string().contains("429")}),
            )?;
            anyhow::bail!("scope model call failed; no retry; see error.json");
        }
    };
    crate::save(&directory.join("audit.json"), &call)?;
    validate_response(&call)?;
    let content = call["response"]["choices"][0]["message"]["content"]
        .as_str()
        .context("audit JSON or refusal; no retry")?;
    let audit: Audit = if arm == "baseline" {
        serde_json::from_str(content)?
    } else {
        serde_json::from_str::<ScopeAudit>(content)?.into_audit(claims.len())?
    };
    crate::save(
        &directory.join("adapted-audit.json"),
        &serde_json::to_value(&audit)?,
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

#[tokio::test]
#[ignore = "explicit one local Qwen call on saved primary text; no source HTTP or OpenRouter"]
async fn primary_saved_qwen() -> Result<()> {
    let fixture: Value =
        serde_json::from_slice(&std::fs::read(std::env::var("PRIMARY_QWEN_INPUT")?)?)?;
    let directory = std::path::PathBuf::from(std::env::var("PRIMARY_QWEN_OUTPUT")?);
    std::fs::create_dir(&directory)?; // Never overwrite an attempted trial.
    let text = fixture["extracted"]["text"]
        .as_str()
        .context("saved text")?;
    let question = fixture["question"].as_str().context("saved question")?;
    ensure!(
        !text.trim().is_empty() && text.chars().count() <= 100_000,
        "saved body cap"
    );
    ensure!(
        fixture["extracted"]["text_hash"] == crate::cache::hash(text.as_bytes()),
        "saved text hash mismatch"
    );
    let settings = std::path::PathBuf::from(std::env::var("HOME")?).join(".omlx/settings.json");
    let result = async {
        let model = crate::model::Model::from_settings(&settings, crate::reranker::MODEL)?;
        crate::reranker::select(&model, question, text, Instant::now()).await
    }
    .await;
    match result {
        Ok((ranges, trace)) => {
            crate::save(
                &directory.join("selected.json"),
                &json!({"selected_ranges":ranges,"offset_unit":"UTF-8 bytes in supplied original text","text_hash":fixture["extracted"]["text_hash"],"trace":trace}),
            )?;
        }
        Err(_) => {
            crate::save(
                &directory.join("error.json"),
                &json!({"status":"local_qwen_failed","retry":false,"fallback":false}),
            )?;
            anyhow::bail!(
                "local Qwen unavailable or invalid response; no retry or fallback; see error.json"
            );
        }
    }
    Ok(())
}
