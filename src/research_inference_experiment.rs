//! Test-only experimental audit schema; not wired into product acceptance.
use super::*;

const PROMPT: &str = "Check an inference solely against its own evidence and original question. First normalize its obligations: extract the required general rule verbatim from one of its own evidence strings (index is zero-based), each required concrete input verbatim from the question, and the complete claimed result. The result field MUST copy the entire original claim string EXACTLY, character for character; do not translate, summarize or normalize that field. Normalize reasoning only in reason. A particular example does not prove a general rule. Never borrow another claim's evidence or invent a premise. If no suitable general rule exists, emit empty rule and supported=false. Verify entity/API, all values, version, negation and conditions. supported requires all obligations plus the result to follow. Return JSON only.";

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Obligation {
    evidence_index: usize,
    rule: String,
    inputs: Vec<String>,
    result: String,
    supported: bool,
    reason: String,
}
fn bindings_valid(row: &Value, check: &Obligation) -> bool {
    !check.rule.trim().is_empty()
        && row["evidence"][check.evidence_index]
            .as_str()
            .is_some_and(|s| s.contains(&check.rule))
        && !check.inputs.is_empty()
        && check
            .inputs
            .iter()
            .all(|p| !p.trim().is_empty() && row["question"].as_str().unwrap().contains(p))
        && check.result == row["claim"].as_str().unwrap()
}

#[test]
fn normalized_inference_bindings_reject_unbound_premises() {
    let row = json!({"question":"API X input 7 version 1", "claim":"result 9", "evidence":["Return supplied default."]});
    let mut check = Obligation {
        evidence_index: 0,
        rule: "Return supplied default.".into(),
        inputs: vec!["input 7".into()],
        result: "result 9".into(),
        supported: true,
        reason: "test".into(),
    };
    assert!(bindings_valid(&row, &check));
    check.rule.clear();
    assert!(!bindings_valid(&row, &check));
    check.rule = "Return supplied default.".into();
    check.inputs = vec!["input 8".into()];
    assert!(!bindings_valid(&row, &check));
    check.inputs = vec!["API Y".into()];
    assert!(!bindings_valid(&row, &check));
    check.inputs = vec!["version 2".into()];
    assert!(!bindings_valid(&row, &check));
    check.inputs = vec!["input 7".into()];
    check.result = "not result 9".into();
    assert!(!bindings_valid(&row, &check));
}

#[tokio::test]
#[ignore = "explicit frozen normalized inference experiment; serial model calls, no source HTTP"]
async fn normalized_inference_frozen_audit() -> Result<()> {
    let directory = std::path::PathBuf::from(std::env::var("RESEARCH_INFERENCE_DIR")?);
    std::fs::create_dir(&directory)?;
    let fixture: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/research/inference-obligations.json"
    ))?;
    let fixture = if let Ok(path) = std::env::var("RESEARCH_INFERENCE_FIXTURE") {
        serde_json::from_str::<Value>(&std::fs::read_to_string(path)?)?
    } else {
        fixture
    };
    let key =
        std::path::PathBuf::from(std::env::var("HOME")?).join(".config/web-search/openrouter.key");
    let model = crate::model::Model::from_openrouter(
        &key,
        "deepseek/deepseek-v4.1-flash-1017",
        Some("http://127.0.0.1:10808"),
    )?;
    for row in fixture["cases"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let context =
            json!({"question":row["question"],"claim":row["claim"],"evidence":row["evidence"]});
        crate::save(
            &directory.join(format!("{id}-input.json")),
            &json!({"prompt":PROMPT,"context":context}),
        )?;
        let call = model
            .research_call(
                PROMPT,
                &serde_json::to_string(&context)?,
                serde_json::to_value(schemars::schema_for!(Obligation))?,
                3072,
                Duration::from_secs(90),
            )
            .await?;
        crate::save(&directory.join(format!("{id}-raw.json")), &call)?;
        let check: Obligation = serde_json::from_str(
            call["response"]["choices"][0]["message"]["content"]
                .as_str()
                .context("missing model content")?,
        )?;
        let valid = bindings_valid(row, &check);
        crate::save(
            &directory.join(format!("{id}-result.json")),
            &json!({"check":check,"bindings_valid":valid,"supported":valid&&check.supported,"expected":row["expected_supported"]}),
        )?;
    }
    Ok(())
}

#[tokio::test]
#[ignore = "explicit same10 frozen original audit baseline; serial no retry"]
async fn original_inference_frozen_audit() -> Result<()> {
    let directory = std::path::PathBuf::from(std::env::var("RESEARCH_INFERENCE_DIR")?);
    std::fs::create_dir(&directory)?;
    let fixture: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/research/inference-obligations.json"
    ))?;
    let prompt = include_str!("../tests/fixtures/research/inference-baseline-prompt.txt");
    let key =
        std::path::PathBuf::from(std::env::var("HOME")?).join(".config/web-search/openrouter.key");
    let model = crate::model::Model::from_openrouter(
        &key,
        "deepseek/deepseek-v4.1-flash-1017",
        Some("http://127.0.0.1:10808"),
    )?;
    for row in fixture["cases"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let context = json!({"question":row["question"],"facets":["Required result"],"facet_indices":[0],"missing":[0],"claims":[{"id":"A1","claim":{"text":row["claim"],"kind":"inference","rationale":"Apply the documented rule to the inputs in the original question.","facets":[0],"evidence":row["evidence"].as_array().unwrap().iter().enumerate().map(|(i,q)|json!({"quote":q,"source_id":format!("S{}",i+1)})).collect::<Vec<_>>()}}],"rejection_context":[],"unresolved_conflicts":[]});
        crate::save(
            &directory.join(format!("{id}-input.json")),
            &json!({"prompt":prompt,"context":context}),
        )?;
        let call = model
            .research_call(
                prompt,
                &serde_json::to_string(&context)?,
                serde_json::to_value(schemars::schema_for!(Audit))?,
                3072,
                Duration::from_secs(90),
            )
            .await?;
        crate::save(&directory.join(format!("{id}-raw.json")), &call)?;
        let audit: Audit = serde_json::from_str(
            call["response"]["choices"][0]["message"]["content"]
                .as_str()
                .context("missing model content")?,
        )?;
        let supported = audit.checks.len() == 1
            && audit.checks[0].id == "A1"
            && audit.checks[0].verdict == Verdict::Supported;
        crate::save(
            &directory.join(format!("{id}-result.json")),
            &json!({"audit":audit,"supported":supported,"expected":row["expected_supported"]}),
        )?;
    }
    Ok(())
}
