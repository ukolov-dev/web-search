use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

pub const MODEL: &str = "Qwen3-Reranker-0.6B-4bit";

pub fn order(body: &Value, count: usize) -> Result<Vec<usize>> {
    let results = body["results"].as_array().context("reranker results")?;
    ensure!(results.len() == count, "reranker incomplete scores");
    let mut seen = vec![false; count];
    let mut scores = Vec::new();
    for row in results {
        let index = row["index"].as_u64().context("reranker index")? as usize;
        ensure!(
            index < count && !seen[index],
            "reranker duplicate/unknown index"
        );
        let score = row["relevance_score"].as_f64().context("reranker score")?;
        ensure!(
            score.is_finite() && (0.0..=1.0).contains(&score),
            "reranker invalid score"
        );
        seen[index] = true;
        scores.push((index, score));
    }
    scores.sort_by(|(a, x), (b, y)| y.total_cmp(x).then(a.cmp(b)));
    Ok(scores.into_iter().map(|(i, _)| i).collect())
}

// Full extracted body, including its tail; exact UTF-8 offsets survive selection.
pub fn windows(text: &str) -> Result<Vec<(usize, usize)>> {
    ensure!(text.chars().count() <= 100_000, "reranker body limit");
    let mut bounds: Vec<_> = text.char_indices().map(|(i, _)| i).collect();
    bounds.push(text.len());
    let len = bounds.len() - 1;
    let mut spans = Vec::new();
    let mut start = 0;
    while start < len {
        let end = (start + 1200).min(len);
        spans.push((bounds[start], bounds[end]));
        if end == len {
            break;
        }
        start += 1000;
    }
    ensure!(spans.len() <= 128, "reranker window limit");
    Ok(spans)
}

#[cfg(test)]
pub fn top3(spans: &[(usize, usize)], order: &[usize]) -> Vec<(usize, usize)> {
    let mut selected: Vec<(usize, usize)> = Vec::new();
    for &i in order {
        let (start, end) = spans[i];
        if !selected
            .iter()
            .any(|(s, e)| end.min(*e).saturating_sub(start.max(*s)) * 2 >= (end - start).min(e - s))
        {
            selected.push((start, end));
            if selected.len() == 3 {
                break;
            }
        }
    }
    selected
}

pub fn top3_diverse(
    text: &str,
    query: &str,
    spans: &[(usize, usize)],
    order: &[usize],
) -> Result<Vec<(usize, usize)>> {
    let selected = diverse_ranges(text, query, spans, order)?;
    let mut selected = operation_context(text, query, &selected);
    if crate::news::is_query(query)
        && let Some(&(start, end)) = spans.first()
    {
        ensure!(
            text.get(start..end)
                .is_some_and(|v| !v.is_empty() && v.chars().count() <= 1200),
            "reranker news lead range"
        );
        // The article lead can carry place/event evidence below Qwen's topical top six.
        if !selected.is_empty() && !selected.contains(&(start, end)) {
            selected.truncate(2);
            selected.push((start, end));
        }
    }
    Ok(selected)
}

fn diverse_ranges(
    text: &str,
    query: &str,
    spans: &[(usize, usize)],
    order: &[usize],
) -> Result<Vec<(usize, usize)>> {
    let query_features = ranking_features(query);
    let pool = order
        .iter()
        .take(6)
        .map(|&i| {
            let &(start, end) = spans.get(i).context("reranker selection index")?;
            let body = text
                .get(start..end)
                .filter(|s| !s.is_empty())
                .context("reranker selection range")?;
            let features = ranking_features(body)
                .intersection(&query_features)
                .cloned()
                .collect::<HashSet<_>>();
            Ok(((start, end), features))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut selected = Vec::new();
    let mut covered = HashSet::new();
    for _ in 0..3 {
        let next = pool
            .iter()
            .enumerate()
            .filter(|(_, (range, _))| {
                let (start, end) = *range;
                !selected.iter().any(|&(s, e)| {
                    end.min(e).saturating_sub(start.max(s)) * 2 >= (end - start).min(e - s)
                })
            })
            .max_by(|(a, (_, x)), (b, (_, y))| {
                let gain = |features: &HashSet<String>| {
                    if selected.is_empty() {
                        0
                    } else {
                        features.difference(&covered).count()
                    }
                };
                gain(x).cmp(&gain(y)).then_with(|| b.cmp(a))
            });
        let Some((_, (range, features))) = next else {
            break;
        };
        selected.push(*range);
        covered.extend(features.iter().cloned());
    }
    Ok(selected)
}

// A repeated operation heading followed by a declaration is stronger than a name in prose.
fn operation_headings(text: &str) -> Vec<(usize, &str)> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (i, c) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
    {
        if c.is_ascii_alphanumeric() || c == '_' {
            start.get_or_insert(i);
        } else if let Some(s) = start.take() {
            tokens.push((s, i, &text[s..i]));
        }
    }
    let mut headings = Vec::new();
    for (i, &(start, end, name)) in tokens.iter().enumerate() {
        if name.len() < 2 || !name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            continue;
        }
        let mut declaration = None;
        for &(s, e, token) in tokens
            .iter()
            .skip(i + 1)
            .take_while(|(s, _, _)| s - end <= 160)
        {
            if token == name
                && declaration.is_some_and(|(end, keyword)| {
                    let prefix = text[end..s].trim();
                    if matches!(keyword, "fn" | "def" | "func" | "function") {
                        return prefix.is_empty();
                    }
                    let mut words = prefix.split_whitespace().filter(|word| {
                        !matches!(
                            *word,
                            "static"
                                | "final"
                                | "abstract"
                                | "native"
                                | "synchronized"
                                | "virtual"
                                | "override"
                                | "async"
                                | "unsafe"
                                | "extern"
                        )
                    });
                    words.next().is_some_and(|ty| {
                        !ty.ends_with(['.', ':'])
                            && ty.chars().any(char::is_alphabetic)
                            && ty
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || "_.:<>?[],&*".contains(c))
                    }) && words.next().is_none()
                })
            {
                let tail = text[e..].trim_start();
                if tail.starts_with('(') && tail.find(')').is_some_and(|n| n <= 240) {
                    headings.push((start, name));
                    break;
                }
            }
            if matches!(
                token,
                "public"
                    | "private"
                    | "protected"
                    | "internal"
                    | "pub"
                    | "fn"
                    | "def"
                    | "func"
                    | "function"
            ) {
                declaration = Some((e, token));
            }
        }
    }
    headings
}

fn operation_context(text: &str, query: &str, selected: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let headings = operation_headings(text);
    let operations: HashSet<_> = query
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
        .collect();
    let query_features = ranking_features(query);
    let mut result = selected.to_vec();
    // Preserve Qwen's strongest scored fragment verbatim.
    for i in 1..selected.len() {
        let (s, e) = selected[i];
        let budget = text[s..e].chars().count();
        let features: HashSet<_> = ranking_features(&text[s..e])
            .intersection(&query_features)
            .cloned()
            .collect();
        for &(start, name) in &headings {
            if start <= s
                || start >= e
                || !operations.contains(name)
                || text[s..start].chars().count() > budget * 2 / 3
            {
                continue;
            }
            let mut end = text[start..]
                .char_indices()
                .nth(budget)
                .map_or(text.len(), |(n, _)| start + n);
            if let Some(&(next, _)) = headings
                .iter()
                .find(|&&(n, other)| n > start && n < end && other != name)
            {
                end = next;
            }
            let new_features = ranking_features(&text[start..end]);
            let overlaps = result.iter().enumerate().any(|(j, &(a, b))| {
                j != i && end.min(b).saturating_sub(start.max(a)) * 2 >= (end - start).min(b - a)
            });
            if !overlaps && features.is_subset(&new_features) {
                result[i] = (start, end);
                break;
            }
        }
    }
    result
}

// Approximate word forms guide selection only; source quotes and API literal checks keep their original text.
fn ranking_features(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|w| {
            w.chars().count() >= 2
                && !matches!(
                    *w,
                    "the"
                        | "for"
                        | "with"
                        | "this"
                        | "that"
                        | "from"
                        | "into"
                        | "only"
                        | "when"
                        | "does"
                        | "are"
                        | "has"
                        | "have"
                        | "was"
                        | "were"
                        | "to"
                        | "of"
                        | "in"
                        | "by"
                        | "an"
                        | "as"
                        | "is"
                        | "it"
                        | "be"
                )
        })
        .map(|word| {
            if word.len() > 5 && word.bytes().all(|c| c.is_ascii_alphabetic()) {
                word[..5].to_owned()
            } else {
                word.to_owned()
            }
        })
        .collect()
}

pub async fn select(
    model: &crate::model::Model,
    query: &str,
    text: &str,
    started: Instant,
) -> Result<(Vec<(usize, usize)>, Value)> {
    let spans = windows(text)?;
    let docs: Vec<_> = spans.iter().map(|&(s, e)| text[s..e].to_owned()).collect();
    let call = model
        .rerank(
            query,
            &docs,
            Duration::from_secs(180).saturating_sub(started.elapsed()),
        )
        .await?;
    let order = order(&call["response"], spans.len())?;
    let selected = top3_diverse(text, query, &spans, &order)?;
    let query_features = ranking_features(query);
    let mut covered = HashSet::new();
    let mut new_counts = Vec::new();
    for &(start, end) in &selected {
        let features = ranking_features(&text[start..end]);
        let before = covered.len();
        covered.extend(features.intersection(&query_features).cloned());
        new_counts.push(covered.len() - before);
    }
    let trace = json!({"selector":"query-feature-cover-v3-news-lead","call":call,"pool_ranges":spans,"candidate_pool":order.iter().take(6).collect::<Vec<_>>(),"selected_ranges":selected,"query_feature_coverage":{"query_count":query_features.len(),"covered_count":covered.len(),"new_counts":new_counts},"offset_unit":"UTF-8 bytes in whitespace-normalized body"});
    Ok((selected, trace))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firecrawl_news_lead_witness_survives_low_qwen_score() -> Result<()> {
        let fixture: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/reranker/news-lead-firecrawl.json"
        ))?;
        let text = fixture["text"].as_str().unwrap();
        let spans = serde_json::from_value::<Vec<(usize, usize)>>(fixture["pool_ranges"].clone())?;
        let rank = order(&fixture["scores"], spans.len())?;
        let before =
            serde_json::from_value::<Vec<(usize, usize)>>(fixture["before_ranges"].clone())?;
        let city_witness = fixture["witnesses"][1].as_str().unwrap();
        assert!(
            !before
                .iter()
                .any(|&(s, e)| text[s..e].contains(city_witness))
        );
        assert_eq!(
            diverse_ranges(text, fixture["query"].as_str().unwrap(), &spans, &rank)?,
            before,
            "reproduce the original scored selection"
        );
        let selected = top3_diverse(text, fixture["query"].as_str().unwrap(), &spans, &rank)?;
        assert_eq!(selected[0], before[0], "preserve the strongest Qwen window");
        assert!(selected.len() <= 3);
        assert!(
            selected
                .iter()
                .all(|&(s, e)| text.get(s..e).is_some_and(|v| v.chars().count() <= 1200))
        );
        for witness in fixture["witnesses"].as_array().unwrap() {
            let witness = witness.as_str().unwrap();
            assert!(text.contains(witness));
            assert!(
                selected.iter().any(|&(s, e)| text[s..e].contains(witness)),
                "lost source witness: {witness}"
            );
        }
        Ok(())
    }

    #[test]
    fn operation_return_clause_survives_window_boundary() -> Result<()> {
        let capture: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/reranker/operation-return-boundary.json"
        ))?;
        let text = capture["text"].as_str().unwrap();
        let spans: Vec<(usize, usize)> = serde_json::from_value(capture["pool_ranges"].clone())?;
        let rank = order(&capture["scores"], spans.len())?;
        let selected = top3_diverse(text, capture["query"].as_str().unwrap(), &spans, &rank)?;
        let before: Vec<(usize, usize)> = serde_json::from_value(capture["before_ranges"].clone())?;
        assert_eq!(
            diverse_ranges(text, capture["query"].as_str().unwrap(), &spans, &rank)?,
            before,
            "reproduce the frozen scored selection"
        );
        assert_eq!(
            selected[0], before[0],
            "keep Qwen's strongest scored window"
        );
        assert_eq!(selected.len(), before.len());
        assert!(
            selected
                .iter()
                .all(|&(s, e)| text[s..e].chars().count() <= 1200)
        );
        let operation = capture["operation"].as_str().unwrap();
        let returns = capture["return_witness"].as_str().unwrap();
        let throws = capture["throws_witness"].as_str().unwrap();
        assert!(text.contains(returns) && text.contains(throws));
        let bound = selected.iter().map(|&(s, e)| &text[s..e]).find(|body| {
            body.contains(operation) && body.contains(returns) && body.contains(throws)
        });
        assert!(
            bound.is_some(),
            "keep the return and exception conditions with their actual operation"
        );
        assert!(
            !bound
                .unwrap()
                .contains(capture["next_operation"].as_str().unwrap()),
            "do not bleed into the neighboring API"
        );
        Ok(())
    }

    #[test]
    fn operation_context_requires_declaration_and_preserves_utf8_scope() {
        let prefix = "α strongest fragment. ";
        let context = "π Previous operation has finished. ".repeat(2);
        let text = format!(
            "{prefix}{context}encodeValue pub fn encodeValue(input) Encodes the argument. Returns encoded value. eraseValue pub fn eraseValue(input) Removes the argument. {}",
            "tail ".repeat(30)
        );
        let start = prefix.len();
        let end = start + text[start..].char_indices().nth(160).unwrap().0;
        let original = [(0, start), (start, end)];
        let aligned = operation_context(&text, "encodeValue encoded value", &original);
        assert_eq!(aligned[0], original[0]);
        assert_eq!(aligned.len(), original.len());
        let body = &text[aligned[1].0..aligned[1].1];
        assert!(body.starts_with("encodeValue pub fn encodeValue(input)"));
        assert!(body.contains("Returns encoded value."));
        assert!(!body.contains("eraseValue"));
        assert!(body.chars().count() <= 160);

        let prose = format!(
            "{prefix}{context}encodeValue is described in a public article. A later example calls encodeValue(input). {}",
            "argument prose ".repeat(30)
        );
        let end = start + prose[start..].char_indices().nth(160).unwrap().0;
        let original = [(0, start), (start, end)];
        assert_eq!(
            operation_context(&prose, "encodeValue argument", &original),
            original,
            "a query-name mention and call in prose are not an operation heading"
        );
    }

    fn fixture(parts: &[&str]) -> (String, Vec<(usize, usize)>) {
        let mut text = String::new();
        let mut spans = Vec::new();
        for part in parts {
            let start = text.len();
            text.push_str(part);
            spans.push((start, text.len()));
            text.push(' ');
        }
        (text, spans)
    }

    #[test]
    fn news_lead_reservation_is_bounded_and_not_a_download_or_tutorial_rule() -> Result<()> {
        let (text, spans) = fixture(&[
            "α Actual article lead carrying the event and place.",
            "β Top ranked passage.",
            "γ Complementary passage.",
            "δ Another topical passage.",
        ]);
        let order = [1, 2, 3, 0];
        let selected = top3_diverse(&text, "новости города", &spans, &order)?;
        assert_eq!(selected, [spans[1], spans[2], spans[0]]);
        for query in [
            "tutorial news city",
            "download news city app",
            "Rust BufRead UTF-8",
        ] {
            assert_eq!(
                top3_diverse(&text, query, &spans, &order)?,
                [spans[1], spans[2], spans[3]]
            );
        }
        assert!(top3_diverse("ж abc", "news city", &[(0, 1), (3, 6)], &[1]).is_err());
        Ok(())
    }

    #[test]
    fn secondary_query_condition_survives_topical_windows() -> Result<()> {
        let (text, spans) = fixture(&[
            "Widget allocation scope explained.",
            "Widget allocation scope with examples.",
            "Widget allocation scope with more examples.",
            "Widget allocation scope reference.",
            "Widget state can persist in child environments.",
            "Widget allocation scope introduction.",
            "A low ranked catalogue lists allocation scope persistence.",
        ]);
        let selected = top3_diverse(
            &text,
            "Widget allocation scope persistence",
            &spans,
            &[0, 1, 2, 3, 4, 5, 6],
        )?;
        assert_eq!(
            selected.first(),
            Some(&spans[0]),
            "keep Qwen's strongest window"
        );
        assert!(
            selected.contains(&spans[4]),
            "a second condition must not disappear behind three topical windows"
        );
        assert!(
            !selected.contains(&spans[6]),
            "do not introduce low ranked keyword catalogues"
        );
        Ok(())
    }

    #[test]
    fn diverse_selection_preserves_bounds_and_utf8() -> Result<()> {
        let (text, spans) = fixture(&[
            "α Alpha allocation scope.",
            "β Alpha scope.",
            "γ Alpha allocation.",
            "δ Alpha scope.",
            "ε Alpha allocation.",
            "ζ Alpha scope.",
            "η Alpha allocation persistence.",
        ]);
        let selected = top3_diverse(
            &text,
            "Alpha allocation persistence",
            &spans,
            &[0, 1, 2, 3, 4, 5, 6],
        )?;
        assert_eq!(selected.len(), 3);
        assert!(
            !selected.contains(&spans[6]),
            "feature coverage stays inside the bounded high-rank pool"
        );
        for &(s, e) in &selected {
            assert!(text.get(s..e).is_some());
        }
        let duplicate = top3_diverse("ж abc", "abc", &[(0, 6), (0, 6), (3, 6)], &[0, 1, 2])?;
        assert_eq!(
            duplicate.len(),
            1,
            "overlapping source windows must not be repeated"
        );
        assert!(
            top3_diverse("ж", "ж", &[(0, 1)], &[0]).is_err(),
            "invalid UTF8 range must fail closed"
        );
        Ok(())
    }

    #[test]
    #[ignore = "explicit offline development replay of frozen Qwen scores; no network or model calls"]
    fn frozen_scores_component_replay() -> Result<()> {
        let data: Value = serde_json::from_slice(&std::fs::read(
            "feasibility/direct-coverage-corpus-2026-10-30.json",
        )?)?;
        let mut rows = Vec::new();
        let mut restored = 0;
        let mut v8_hits = 0;
        let mut after_hits = 0;
        for row in data["branches"]["fixed"]["cases"].as_array().unwrap() {
            let cid = row["id"].as_str().unwrap();
            let gold = data["gold"]["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|g| g["id"] == cid)
                .unwrap();
            for source in gold["gold"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|g| g["grade"] == 2)
            {
                let id = source["id"].as_str().unwrap();
                let call = row["page_verification"]["passage_reranker"]["calls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|c| c["id"] == id);
                let Some(call) = call else {
                    rows.push(json!({"case":cid,"id":id,"status":"not_reranked","reason":"unread or no successfully extracted body"}));
                    continue;
                };
                let doc = &data["documents"]["fixed"][format!("{cid}-{id}-document.json")];
                let text = doc["text"]
                    .as_str()
                    .unwrap()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                let spans: Vec<(usize, usize)> =
                    serde_json::from_value(call["trace"]["pool_ranges"].clone())?;
                let rank = order(&call["trace"]["call"]["response"], spans.len())?;
                let old = top3(&spans, &rank);
                let recorded: Vec<(usize, usize)> =
                    serde_json::from_value(call["trace"]["selected_ranges"].clone())?;
                assert_eq!(old, recorded, "reconstruct the real previous selection");
                let selected = top3_diverse(&text, row["query"].as_str().unwrap(), &spans, &rank)?;
                let v8 = diverse_ranges(&text, row["query"].as_str().unwrap(), &spans, &rank)?;
                let old_body = old.iter().map(|&(s, e)| &text[s..e]).collect::<Vec<_>>();
                let v8_body = v8.iter().map(|&(s, e)| &text[s..e]).collect::<Vec<_>>();
                let new_body = selected
                    .iter()
                    .map(|&(s, e)| &text[s..e])
                    .collect::<Vec<_>>();
                let mut witnesses = Vec::new();
                for support in source["support"].as_array().unwrap() {
                    let quote = support["quote"].as_str().unwrap();
                    let retained_before = v8_body.iter().any(|w| w.contains(quote));
                    let retained_after = new_body.iter().any(|w| w.contains(quote));
                    assert!(
                        !retained_before || retained_after,
                        "lost frozen witness: {cid}/{id}: {quote}"
                    );
                    v8_hits += usize::from(retained_before);
                    after_hits += usize::from(retained_after);
                    witnesses.push(json!({"quote":quote,"in_extracted_body":text.contains(quote),"before":old_body.iter().any(|w|w.contains(quote)),"v8":retained_before,"after":retained_after}));
                }
                let required = match (cid, id) {
                    ("c05", "C014") => Some(
                        "If a NOT NULL constraint is specified, then the column must have a default value other than NULL.",
                    ),
                    ("c06", "C005") => Some(
                        "Environment variables are passed through to the build execution environment, and persist in containers instantiated from the image.",
                    ),
                    _ => None,
                };
                if let Some(quote) = required {
                    assert!(text.contains(quote));
                    assert!(!old_body.iter().any(|w| w.contains(quote)));
                    assert!(
                        new_body.iter().any(|w| w.contains(quote)),
                        "secondary source condition must be recovered"
                    );
                    restored += 1;
                }
                rows.push(json!({"case":cid,"id":id,"status":"replayed","body_chars":text.chars().count(),"before_ranges":old,"v8_ranges":v8,"after_ranges":selected,"witnesses":witnesses}));
            }
        }
        assert_eq!(restored, 2);
        let output =
            std::path::Path::new("runs/passage-boundary-2026-10-30/frozen-score-replay.json");
        crate::save(
            output,
            &json!({"kind":"offline frozen-scoring component replay, not final relevance labels","network_calls":0,"model_calls":0,"selector":"query-feature-cover-v2-operation-context","v8_witness_hits":v8_hits,"after_witness_hits":after_hits,"lost_v8_witnesses":0,"restored_secondary_conditions":restored,"cases":rows}),
        )?;
        println!("saved {} source comparisons", rows.len());
        Ok(())
    }
    #[tokio::test]
    #[ignore = "explicit local Qwen3 inference only, no remote network"]
    async fn local_rust_transport() -> Result<()> {
        let model = crate::model::Model::from_settings(
            std::path::Path::new(&std::env::var("WEB_SEARCH_OMLX_SETTINGS")?),
            MODEL,
        )?;
        model.check().await?;
        let call = model
            .rerank(
                "Why can a Rust Mutex be poisoned?",
                &[
                    "A mutex is poisoned if a thread panics while holding the lock.".into(),
                    "Bananas contain potassium and grow in warm climates.".into(),
                ],
                Duration::from_secs(30),
            )
            .await?;
        assert_eq!(order(&call["response"], 2)?, vec![0, 1]);
        crate::save(
            std::path::Path::new("runs/reranker-2026-10-30/rust-local-smoke.json"),
            &call,
        )?;
        Ok(())
    }
    #[test]
    fn complete_utf8_pool_tail_and_nonoverlapping_selection() -> Result<()> {
        let text = "ж".repeat(100_000);
        let spans = windows(&text)?;
        assert_eq!(spans.len(), 100);
        assert_eq!(spans.last().unwrap().1, text.len());
        assert!(
            spans
                .iter()
                .all(|&(s, e)| text[s..e].chars().count() <= 1200)
        );
        let selected = top3(
            &[(0, 100), (10, 110), (100, 200), (200, 300)],
            &[1, 0, 2, 3],
        );
        assert_eq!(selected, vec![(10, 110), (100, 200), (200, 300)]);
        assert!(windows(&"a".repeat(100_001)).is_err());
        Ok(())
    }
    #[test]
    fn rejects_partial_duplicate_nonfinite_and_invalid_scores() {
        let good = json!({"results":[{"index":1,"relevance_score":0.8},{"index":0,"relevance_score":0.8}]});
        assert_eq!(order(&good, 2).unwrap(), vec![0, 1]);
        assert!(order(&good, 3).is_err());
        for rows in [
            json!([{"index":0,"relevance_score":0.1},{"index":0,"relevance_score":0.5}]),
            json!([{"index":0,"relevance_score":1.1},{"index":1,"relevance_score":0.5}]),
            json!([{"index":2,"relevance_score":0.1},{"index":1,"relevance_score":null}]),
        ] {
            assert!(order(&json!({"results":rows}), 2).is_err());
        }
    }
}
