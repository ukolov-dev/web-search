//! Date/type evidence is deterministic; neither URL year nor an adjacent teaser dates an article.
use anyhow::{Result, ensure};
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, Utc};
use dom_query::{Document, Selection};
use serde_json::{Value, json};
use std::collections::HashSet;

fn compact(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn query_text(query: &str) -> String {
    query
        .split_whitespace()
        .filter(|word| !word.to_lowercase().starts_with("site:"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn is_query(query: &str) -> bool {
    let words: Vec<_> = query_text(query)
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_owned)
        .collect();
    let news = words
        .iter()
        .any(|w| w.starts_with("новост") || matches!(w.as_str(), "news" | "headlines"));
    let download = words.iter().any(|w| {
        matches!(
            w.as_str(),
            "скачать" | "установить" | "download" | "install" | "apk"
        )
    });
    let application = words
        .iter()
        .any(|w| w.starts_with("приложен") || matches!(w.as_str(), "app" | "apps" | "application"));
    let report = words.iter().any(|w| {
        w.starts_with("выпуск")
            || w.starts_with("запуск")
            || w.starts_with("обновл")
            || matches!(
                w.as_str(),
                "о" | "об" | "about" | "release" | "launch" | "launched"
            )
    });
    let tutorial = words
        .iter()
        .any(|w| matches!(w.as_str(), "tutorial" | "инструкция" | "руководство"));
    news && !download && !tutorial && (!application || report)
}

fn moscow() -> FixedOffset {
    FixedOffset::east_opt(3 * 3600).unwrap()
}

pub fn current_date() -> String {
    Utc::now().with_timezone(&moscow()).date_naive().to_string()
}

pub fn current_timestamp() -> String {
    Utc::now().to_rfc3339()
}

fn month(word: &str) -> Option<u32> {
    match word {
        "январь" | "января" | "январе" | "january" | "jan" => Some(1),
        "февраль" | "февраля" | "феврале" | "february" | "feb" => Some(2),
        "март" | "марта" | "марте" | "march" | "mar" => Some(3),
        "апрель" | "апреля" | "апреле" | "april" | "apr" => Some(4),
        "май" | "мая" | "мае" | "may" => Some(5),
        "июнь" | "июня" | "июне" | "june" | "jun" => Some(6),
        "июль" | "июля" | "июле" | "july" | "jul" => Some(7),
        "август" | "августа" | "августе" | "august" | "aug" => Some(8),
        "сентябрь" | "сентября" | "сентябре" | "september" | "sep" => {
            Some(9)
        }
        "октябрь" | "октября" | "октябре" | "october" | "oct" => Some(10),
        "ноябрь" | "ноября" | "ноябре" | "november" | "nov" => Some(11),
        "декабрь" | "декабря" | "декабре" | "december" | "dec" => Some(12),
        _ => None,
    }
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

fn parse_date(raw: &str) -> Option<(NaiveDate, Option<String>)> {
    let raw = raw.trim();
    if let Ok(t) = DateTime::parse_from_rfc3339(raw) {
        return Some((
            t.with_timezone(&moscow()).date_naive(),
            Some(t.to_rfc3339()),
        ));
    }
    for format in ["%Y-%m-%d", "%d.%m.%Y", "%d/%m/%Y", "%Y/%m/%d", "%d-%m-%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(raw, format)
            && (1900..=9999).contains(&d.year())
        {
            return Some((d, None));
        }
    }
    for format in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M"] {
        if let Ok(t) = NaiveDateTime::parse_from_str(raw, format) {
            return Some((t.date(), None));
        }
    }
    let tokens = words(raw);
    for w in tokens.windows(3) {
        if let (Ok(day), Some(month), Ok(year)) =
            (w[0].parse::<u32>(), month(&w[1]), w[2].parse::<i32>())
            && (1900..=9999).contains(&year)
        {
            return NaiveDate::from_ymd_opt(year, month, day).map(|d| (d, None));
        }
    }
    None
}

/// Calendar window; unsupported date constraints are explicit, rather than silently becoming "today".
pub fn window(query: &str, today: &str) -> Value {
    let Some((as_of, _)) = parse_date(today) else {
        return json!({"status":"invalid","reason":"invalid_as_of_date"});
    };
    let query = query_text(query);
    let tokens = words(&query);
    let mut selected = None;
    // Keep numeric date punctuation long enough to parse DD.MM.YYYY and ISO dates.
    for token in query.split_whitespace() {
        if let Some((date, _)) =
            parse_date(token.trim_matches(|c: char| matches!(c, ',' | ';' | '(' | ')')))
        {
            if selected.is_some_and(|(start, end)| start != date || end != date) {
                return json!({"status":"unsupported","reason":"multiple_dates_require_event_relation","as_of":today});
            }
            selected = Some((date, date));
        }
    }
    for (i, w) in tokens.windows(3).enumerate() {
        if let (Ok(day), Some(m), Ok(year)) =
            (w[0].parse::<u32>(), month(&w[1]), w[2].parse::<i32>())
            && w[2].len() == 4
        {
            if w[0].len() == 4 && i > 0 && month(&tokens[i - 1]).is_some() {
                continue;
            }
            if i > 0 && tokens[i - 1].parse::<u32>().is_ok() {
                return json!({"status":"unsupported","reason":"multiple_dates_require_event_relation","as_of":today});
            }
            let Some(date) = NaiveDate::from_ymd_opt(year, m, day) else {
                return json!({"status":"invalid","reason":"invalid_calendar_date","as_of":today});
            };
            if selected.is_some_and(|(start, end)| start != date || end != date) {
                return json!({"status":"unsupported","reason":"multiple_dates_require_event_relation","as_of":today});
            }
            selected = Some((date, date));
        }
    }
    if selected.is_none() {
        for w in tokens.windows(2) {
            if let (Some(m), Ok(year)) = (month(&w[0]), w[1].parse::<i32>())
                && w[1].len() == 4
            {
                let Some(start) = NaiveDate::from_ymd_opt(year, m, 1) else {
                    continue;
                };
                let end = start
                    .checked_add_months(chrono::Months::new(1))
                    .unwrap()
                    .pred_opt()
                    .unwrap();
                if selected.is_some_and(|period| period != (start, end)) {
                    return json!({"status":"unsupported","reason":"multiple_date_periods","as_of":today});
                }
                selected = Some((start, end));
            }
        }
    }
    let yesterday = tokens.iter().any(|w| w == "вчера" || w == "yesterday");
    let today_word = tokens.iter().any(|w| w == "сегодня" || w == "today");
    if yesterday && today_word {
        return json!({"status":"unsupported","reason":"multiple_relative_dates","as_of":today});
    }
    let relative = if yesterday {
        Some(as_of - Duration::days(1))
    } else if today_word {
        Some(as_of)
    } else {
        None
    };
    if let Some(day) = relative {
        if selected.is_some_and(|(start, end)| day < start || day > end) {
            return json!({"status":"conflict","reason":"relative_explicit_date_conflict","as_of":today});
        }
        selected = Some((day, day));
    }
    let unresolved_year = query.split(|c: char| !c.is_ascii_digit()).any(|part| {
        part.len() == 4
            && part
                .parse::<i32>()
                .is_ok_and(|year| (1900..=9999).contains(&year))
    });
    let unresolved_numeric_date = query.split_whitespace().any(|token| {
        let parts: Vec<_> = token.split(['.', '/', '-']).collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
    });
    if selected.is_none()
        && (unresolved_year
            || unresolved_numeric_date
            || tokens.iter().any(|w| {
                (w.len() == 4 && w.parse::<i32>().is_ok())
                    || month(w).is_some()
                    || w.starts_with("недел")
                    || w.starts_with("месяц")
                    || w.starts_with("прошл")
                    || w.starts_with("следующ")
                    || matches!(
                        w.as_str(),
                        "завтра" | "tomorrow" | "week" | "month" | "year" | "сутки"
                    )
            }))
    {
        return json!({"status":"unsupported","reason":"unsupported_date_constraint","as_of":today});
    }
    let (start, end) = selected.unwrap_or((as_of - Duration::days(2), as_of));
    let timestamp = if today == current_date() {
        current_timestamp()
    } else {
        format!("{today}T23:59:59+03:00")
    };
    json!({"status":"valid","start":start.to_string(),"end":end.to_string(),"as_of":today,"as_of_timestamp":timestamp,"timezone":"Europe/Moscow","basis":if selected.is_some(){"explicit"}else{"last_3_calendar_days"}})
}

fn schema_types(value: &Value) -> Vec<&str> {
    match value.get("@type") {
        Some(Value::String(s)) => vec![s.rsplit('/').next().unwrap_or(s)],
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .map(|s| s.rsplit('/').next().unwrap_or(s))
            .collect(),
        _ => vec![],
    }
}

fn same_page(raw: &str, url: &str) -> bool {
    let Ok(base) = crate::net::validate_url(url) else {
        return false;
    };
    let Ok(mut target) = base.join(raw) else {
        return false;
    };
    target.set_fragment(None);
    let mut base = base;
    base.set_fragment(None);
    target == base
}

fn owns_schema(value: &Value, url: &str, heading: &str) -> bool {
    let mut identities = Vec::new();
    for key in ["url", "mainEntityOfPage", "@id"] {
        if let Some(v) = value.get(key)
            && let Some(s) = v.as_str().or_else(|| v["@id"].as_str())
        {
            identities.push(s);
        }
    }
    if identities.is_empty() {
        !heading.is_empty()
            && value["headline"]
                .as_str()
                .is_some_and(|h| compact(h) == heading)
    } else {
        identities.iter().all(|id| same_page(id, url))
    }
}

fn schema_nodes<'a>(value: &'a Value, nodes: &mut Vec<&'a Value>) {
    match value {
        Value::Array(values) => {
            for v in values {
                schema_nodes(v, nodes);
            }
        }
        Value::Object(_) => {
            nodes.push(value);
            if let Some(graph) = value.get("@graph") {
                schema_nodes(graph, nodes);
            }
            if let Some(entity) = value.get("mainEntity") {
                schema_nodes(entity, nodes);
            }
        }
        _ => {}
    }
}

fn witness(source: &str, raw: &str) -> Value {
    if raw.chars().count() > 256 {
        return json!({"source":source,"raw":raw.chars().take(256).collect::<String>(),"date":null,"timestamp":null,"reason":"date_value_limit"});
    }
    let parsed = parse_date(raw);
    json!({"source":source,"raw":raw,"date":parsed.as_ref().map(|(d,_)|d.to_string()),"timestamp":parsed.and_then(|(_,t)|t)})
}

fn owns_article_scope(scope: &Selection<'_>, url: &str) -> bool {
    if !scope.is("[itemscope]") && !scope.ancestors(None).filter("[itemscope]").is_empty() {
        return false;
    }
    if let Some(types) = scope.attr("itemtype") {
        if types.trim().is_empty()
            || !types.split_whitespace().all(|t| {
                matches!(
                    t,
                    "https://schema.org/NewsArticle"
                        | "http://schema.org/NewsArticle"
                        | "https://schema.org/Article"
                        | "http://schema.org/Article"
                        | "https://schema.org/BlogPosting"
                        | "http://schema.org/BlogPosting"
                )
            })
        {
            return false;
        }
    } else if scope.is("[itemscope]") {
        return false;
    }
    if scope
        .attr("itemid")
        .is_some_and(|id| id.trim().is_empty() || !same_page(&id, url))
    {
        return false;
    }
    for node in scope
        .select("[itemprop~='url'], [itemprop~='mainEntityOfPage']")
        .iter()
    {
        let mut parent = node.parent();
        let mut nested = node.is("[itemscope]");
        while !parent.is_empty() && !parent.is_selection(scope) {
            if parent.is("[itemscope], article, aside, nav, footer") {
                nested = true;
                break;
            }
            parent = parent.parent();
        }
        // A nested WebPage or teaser owns its identity, rather than the surrounding Article.
        if nested || parent.is_empty() {
            continue;
        }
        let mut identities: Vec<_> = [node.attr("content"), node.attr("href")]
            .into_iter()
            .flatten()
            .map(|s| s.to_string())
            .collect();
        if identities.is_empty() {
            identities.push(compact(&node.text()));
        }
        if identities
            .iter()
            .any(|id| id.trim().is_empty() || !same_page(id, url))
        {
            return false;
        }
    }
    true
}

fn own_article_node(node: &Selection<'_>, scope: &Selection<'_>, url: &str) -> bool {
    if node.is("[itemscope]") || !node.ancestors(None).filter("aside, nav, footer").is_empty() {
        return false;
    }
    let mut parent = node.parent();
    while !parent.is_empty() && !parent.is_selection(scope) {
        let linked_teaser = parent.select("h1").is_empty()
            && parent.select("a[href]").iter().any(|anchor| {
                anchor
                    .attr("href")
                    .is_some_and(|href| !same_page(&href, url))
            });
        let linked_node = parent.is("a[href]")
            && parent
                .attr("href")
                .is_some_and(|href| !same_page(&href, url));
        if parent.is("[itemscope], article") || linked_teaser || linked_node {
            return false;
        }
        parent = parent.parent();
    }
    !parent.is_empty()
}

/// Shares the same heading and identity boundary between publication dates and article text.
pub(crate) fn main_article_scope<'a>(doc: &'a Document, url: &str) -> Option<Selection<'a>> {
    let headings = doc.select("h1");
    let main_articles = headings.ancestors(None).filter("article");
    (headings.length() == 1
        && !compact(&headings.text()).is_empty()
        && main_articles.length() == 1
        && main_articles
            .ancestors(None)
            .filter("aside, nav, footer")
            .is_empty()
        && own_article_node(&headings, &main_articles, url)
        && !headings
            .select("a[href]")
            .iter()
            .any(|a| a.attr("href").is_some_and(|href| !same_page(&href, url)))
        && owns_article_scope(&main_articles, url))
    .then_some(main_articles)
}

pub fn metadata(html: &str, url: &str) -> Value {
    if html.len() > crate::net::MAX_HTML {
        return json!({"kind":"unknown","date_status":"invalid","reason":"HTML_body_limit"});
    }
    let doc = Document::from(html);
    if doc.select("*").length() > 50_000 {
        return json!({"kind":"unknown","date_status":"invalid","reason":"DOM_limit"});
    }
    let headings = doc.select("h1");
    let heading = if headings.length() == 1 {
        compact(&headings.text())
    } else {
        String::new()
    };
    let title = compact(&doc.select("head title").text());
    let mut values = Vec::new();
    for script in doc
        .select("script[type='application/ld+json']")
        .iter()
        .take(20)
    {
        let text = script.text();
        if text.len() <= 256 * 1024
            && let Ok(v) = serde_json::from_str::<Value>(&text)
        {
            values.push(v);
        }
    }
    let mut schemas = Vec::new();
    for value in &values {
        schema_nodes(value, &mut schemas);
    }
    let own: Vec<_> = schemas
        .iter()
        .copied()
        .filter(|v| owns_schema(v, url, &heading))
        .collect();
    let app = own.iter().any(|v| {
        schema_types(v).iter().any(|t| {
            matches!(
                *t,
                "SoftwareApplication" | "MobileApplication" | "WebApplication"
            )
        })
    });
    let articles: Vec<_> = own
        .iter()
        .copied()
        .filter(|v| {
            schema_types(v)
                .iter()
                .any(|t| matches!(*t, "NewsArticle" | "Article" | "BlogPosting"))
        })
        .collect();
    let article_dom = doc.select("article");
    let article_scope = main_article_scope(&doc, url);
    let og_article = doc
        .select("head meta[property='og:type']")
        .attr("content")
        .is_some_and(|v| v.eq_ignore_ascii_case("article"));
    let root = crate::net::validate_url(url).is_ok_and(|u| u.path().trim_matches('/').is_empty());
    let article = !root
        && !app
        && !heading.is_empty()
        && (article_scope.is_some() || og_article || !articles.is_empty());
    let feed = !article
        && !app
        && (root
            || article_dom.length() > 1
            || schemas
                .iter()
                .any(|v| schema_types(v).contains(&"ItemList"))
            || links(html, url, "новости", 2).is_ok_and(|links| links.len() >= 2));
    let mut witnesses = Vec::new();
    if article {
        for value in &articles {
            if let Some(raw) = value["datePublished"].as_str() {
                witnesses.push(witness("jsonld.datePublished", raw));
            }
        }
        for meta in doc.select("head meta[property='article:published_time'], head meta[name='article:published_time']").iter() {
            if let Some(raw) = meta.attr("content") { witnesses.push(witness("meta.article:published_time",&raw)); }
        }
        if let Some(article_dom) = article_scope {
            for node in article_dom
                .select("[itemprop='datePublished'], time[pubdate]")
                .iter()
            {
                // A nested item or linked teaser owns its dates, even when rendered inside the article.
                if !own_article_node(&node, &article_dom, url) {
                    continue;
                }
                let raw = node
                    .attr("content")
                    .or_else(|| node.attr("datetime"))
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| compact(&node.text()));
                witnesses.push(witness("article.datePublished", &raw));
            }
        }
    }
    let dates: HashSet<_> = witnesses
        .iter()
        .filter_map(|w| w["date"].as_str())
        .collect();
    let status = if dates.len() > 1 {
        "conflict"
    } else if witnesses.iter().any(|w| w["date"].is_null()) {
        "invalid"
    } else if dates.is_empty() {
        "missing"
    } else {
        "valid"
    };
    json!({"kind":if app{"application"}else if article{"article"}else if feed{"feed"}else{"unknown"},"date_status":status,"published_date":if status=="valid"{dates.iter().next().copied()}else{None},"publication_witnesses":witnesses,"heading":heading,"title":title,"source_url":url})
}

pub fn check_date(meta: &Value, window: &Value) -> Value {
    let future_timestamp = meta["publication_witnesses"].as_array().is_some_and(|ws| {
        ws.iter().filter_map(|w| w["timestamp"].as_str()).any(|t| {
            DateTime::parse_from_rfc3339(t)
                .ok()
                .zip(
                    window["as_of_timestamp"]
                        .as_str()
                        .and_then(|s| DateTime::parse_from_rfc3339(s).ok()),
                )
                .is_some_and(|(t, now)| t > now)
        })
    });
    let reason = if meta["kind"] != "article" {
        "not_article"
    } else if window["status"] != "valid" {
        "invalid_or_unsupported_window"
    } else if meta["date_status"] != "valid" {
        "publication_date_not_verified"
    } else if meta["published_date"]
        .as_str()
        .zip(window["as_of"].as_str())
        .is_some_and(|(d, t)| d > t)
        || future_timestamp
    {
        "future_publication"
    } else if meta["published_date"]
        .as_str()
        .zip(window["start"].as_str())
        .is_none_or(|(d, s)| d < s)
        || meta["published_date"]
            .as_str()
            .zip(window["end"].as_str())
            .is_none_or(|(d, e)| d > e)
    {
        "outside_publication_window"
    } else {
        "verified_publication_date"
    };
    json!({"eligible":reason=="verified_publication_date","status":if reason=="verified_publication_date"{"verified"}else{"rejected"},"reason":reason,"published_date":meta["published_date"],"window":window})
}

pub fn links(html: &str, url: &str, query: &str, max: usize) -> Result<Vec<Value>> {
    ensure!((1..=50).contains(&max), "news link limit");
    let discovered = crate::discover::filtered_links(html, url, query, 50, |link| {
        let Ok(url) = crate::net::validate_url(link["url"].as_str().unwrap_or("")) else {
            return false;
        };
        let path = url.path().to_lowercase();
        let leaf = path.trim_matches('/').rsplit('/').next().unwrap_or("");
        let excluded = [
            "/tag/",
            "/tags/",
            "/category/",
            "/theme/",
            "/themes/",
            "/topics/",
            "/rubric/",
            "/author/",
            "/search",
            "/download",
            "/android/",
            "/ios/",
            "/app/",
            "/privacy",
            "/contacts",
            "/about",
            "/rules",
            "/pravila",
            "/cookie-policy",
            "/usloviya-ispolzovaniya",
            "/politika-konfidencialnosti",
        ]
        .iter()
        .any(|s| path.contains(s));
        let title = link["label"].as_str().unwrap_or("");
        !excluded
            && title.chars().count() >= 25
            && ((leaf.chars().count() >= 12 && (leaf.contains('-') || leaf.ends_with(".html")))
                || leaf.len() >= 5
                    && leaf.chars().all(|c| c.is_ascii_digit())
                    && path.matches('/').count() >= 2)
    })?;
    let terms: Vec<_> = query
        .split_whitespace()
        .filter(|w| !w.to_lowercase().starts_with("site:"))
        .flat_map(words)
        .filter(|w| {
            w.chars().count() > 4
                && w.chars().all(char::is_alphabetic)
                && month(w).is_none()
                && !w.starts_with("новост")
                && !matches!(w.as_str(), "сегодня" | "вчера" | "today" | "yesterday")
        })
        .map(|w| {
            let mut chars: Vec<_> = w.chars().collect();
            if chars.iter().all(|c| ('а'..='я').contains(c) || *c == 'ё') {
                chars.pop();
            }
            chars.into_iter().collect::<String>()
        })
        .collect();
    let mut links = discovered["links"].as_array().unwrap().clone();
    // A short Cyrillic prefix tolerates case endings; coverage and the date gate still decide relevance.
    links.sort_by_key(|link| {
        std::cmp::Reverse(
            terms
                .iter()
                .filter(|term| {
                    words(link["label"].as_str().unwrap_or(""))
                        .iter()
                        .any(|word| word.starts_with(term.as_str()))
                })
                .count(),
        )
    });
    links.truncate(max);
    Ok(links)
}

#[cfg(test)]
mod tests {
    use super::*;
    const URL: &str = "https://example.org/news/city-event/";
    fn article(date: &str) -> String {
        format!(
            r#"<html><head><meta property="og:type" content="article"><meta property="article:published_time" content="{date}"></head><body><article><h1>Городская новость</h1><p>Текст конкретной новости.</p></article></body></html>"#
        )
    }

    #[test]
    fn saved_bezformata_own_microdata_survives_related_article_siblings() {
        for (html, url) in [
            (
                include_str!("../tests/fixtures/news/bezformata-train-microdata.html"),
                "https://nnovgorod.bezformata.com/listnews/poezd-svyazhet-kazan/165476847/",
            ),
            (
                include_str!("../tests/fixtures/news/bezformata-dispatch-microdata.html"),
                "https://nnovgorod.bezformata.com/listnews/punkt-dlya-otdiha/165482058/",
            ),
        ] {
            let meta = metadata(html, url);
            assert_eq!(meta["kind"], "article", "{url}");
            assert_eq!(meta["date_status"], "valid", "{url}: {meta}");
            assert_eq!(meta["published_date"], "2026-11-03", "{url}");
            assert_eq!(meta["publication_witnesses"].as_array().unwrap().len(), 1);
            assert_eq!(
                meta["publication_witnesses"][0]["source"],
                "article.datePublished"
            );
            assert_eq!(
                check_date(
                    &meta,
                    &window("новости Нижнего Новгорода сегодня", "2026-11-03")
                )["eligible"],
                true,
                "{url}"
            );
        }
    }

    fn scoped_microdata(attributes: &str, own: &str) -> String {
        format!(
            r#"<head><meta property="og:type" content="article"></head><main><article {attributes}><h1>Городская новость</h1>{own}</article><article itemscope itemtype="https://schema.org/NewsArticle"><h2>Другой материал</h2><meta itemprop="datePublished" content="2019-01-01"></article></main>"#
        )
    }

    #[test]
    fn main_microdata_identity_is_local_to_the_article_scope() {
        let date = r#"<meta itemprop="datePublished" content="2026-11-03">"#;
        let own_url = format!(r#"<link itemprop="url" href="{URL}">"#);
        let typed = r#"itemscope itemtype="https://schema.org/NewsArticle""#;
        for (attributes, properties) in [
            (
                format!(r#"{typed} itemid="https://example.org/foreign/""#),
                format!("{own_url}{date}"),
            ),
            (
                typed.to_owned(),
                format!(
                    r#"{own_url}<meta itemprop="url" content="https://example.org/foreign/">{date}"#
                ),
            ),
            (
                typed.to_owned(),
                format!(r#"<link itemprop="url" href="/foreign/" content="{URL}">{date}"#),
            ),
            (
                typed.to_owned(),
                format!(
                    r#"{own_url}<link itemprop="mainEntityOfPage" href="https://example.org/foreign/">{date}"#
                ),
            ),
            (format!(r#"{typed} itemid="""#), date.to_owned()),
            (
                typed.to_owned(),
                format!(r#"<link itemprop="url" href="">{date}"#),
            ),
            (
                typed.to_owned(),
                format!(r#"<link itemprop="mainEntityOfPage" href="">{date}"#),
            ),
        ] {
            assert_eq!(
                metadata(&scoped_microdata(&attributes, &properties), URL)["date_status"],
                "missing",
                "{attributes}: {properties}"
            );
        }
        let nested = format!(
            r#"{own_url}<div itemscope itemprop="mainEntityOfPage" itemtype="https://schema.org/WebPage" itemid="https://example.org/"><link itemprop="url" href="https://example.org/"></div>{date}"#
        );
        assert_eq!(
            metadata(&scoped_microdata(typed, &nested), URL)["published_date"],
            "2026-11-03"
        );
        let fragment_id = format!(r#"{typed} itemid="{URL}#article""#);
        assert_eq!(
            metadata(&scoped_microdata(&fragment_id, date), URL)["published_date"],
            "2026-11-03"
        );
        let own_entity = format!(r#"<link itemprop="mainEntityOfPage" href="{URL}">{date}"#);
        assert_eq!(
            metadata(&scoped_microdata(typed, &own_entity), URL)["published_date"],
            "2026-11-03"
        );
    }

    #[test]
    fn main_microdata_rejects_foreign_types_and_ambiguous_headings() {
        let date = r#"<meta itemprop="datePublished" content="2026-11-03">"#;
        for attributes in [
            r#"itemscope itemtype="https://schema.org/Event""#,
            r#"itemscope itemtype="https://schema.org/SoftwareApplication""#,
            r#"itemscope itemtype="https://schema.org/NewsArticle https://schema.org/Event""#,
            r#"itemscope itemtype="https://foreign.example/NewsArticle""#,
            "itemscope",
        ] {
            assert_eq!(
                metadata(&scoped_microdata(attributes, date), URL)["date_status"],
                "missing",
                "{attributes}"
            );
        }
        for html in [
            scoped_microdata("", date).replace(
                "<h2>Другой материал</h2>",
                "<h1>Второй главный заголовок</h1>",
            ),
            format!(
                r#"<head><meta property="og:type" content="article"></head><article><article><h1>Городская новость</h1>{date}</article></article>"#
            ),
        ] {
            let meta = metadata(&html, URL);
            assert_eq!(meta["date_status"], "missing", "{html}");
            assert_eq!(
                check_date(&meta, &window("новости сегодня", "2026-11-03"))["eligible"],
                false
            );
        }
    }

    #[test]
    fn unscoped_article_cannot_inherit_a_foreign_microdata_entity() {
        let date = r#"<meta itemprop="datePublished" content="2026-11-03">"#;
        for attributes in ["", r#"itemtype="https://schema.org/NewsArticle""#] {
            let html = format!(
                r#"<section itemscope itemtype="https://schema.org/Event">{}</section>"#,
                scoped_microdata(attributes, date)
            );
            assert_eq!(
                metadata(&html, URL)["date_status"],
                "missing",
                "{attributes}"
            );
        }
        let html = format!(
            r#"<section itemscope itemtype="https://schema.org/Event">{}</section>"#,
            scoped_microdata(
                r#"itemscope itemtype="https://schema.org/NewsArticle""#,
                date
            )
        );
        assert_eq!(metadata(&html, URL)["published_date"], "2026-11-03");
    }

    #[test]
    fn main_article_heading_cannot_belong_to_a_teaser_or_nested_entity() {
        let date = r#"<meta itemprop="datePublished" content="2026-11-03">"#;
        for heading in [
            r#"<aside><h1>Чужой заголовок</h1></aside>"#,
            r#"<nav><h1>Чужой заголовок</h1></nav>"#,
            r#"<footer><h1>Чужой заголовок</h1></footer>"#,
            r#"<section itemscope itemtype="https://schema.org/Event"><h1>Чужой заголовок</h1></section>"#,
            r#"<h1 itemscope itemtype="https://schema.org/Event">Чужой заголовок</h1>"#,
            r#"<a href="/foreign/"><h1>Чужой заголовок</h1></a>"#,
            r#"<h1><a href="/foreign/">Чужой заголовок</a></h1>"#,
        ] {
            let html = scoped_microdata("", date).replace("<h1>Городская новость</h1>", heading);
            assert_eq!(metadata(&html, URL)["date_status"], "missing", "{heading}");
        }
        let heading = format!(
            r#"<header><h1><a href="{URL}#headline">Городская новость</a></h1><a href="/author/editor/">Редактор</a></header>"#
        );
        let html = scoped_microdata("", date).replace("<h1>Городская новость</h1>", &heading);
        assert_eq!(metadata(&html, URL)["published_date"], "2026-11-03");
    }

    #[test]
    fn main_microdata_never_borrows_nested_event_teaser_or_modified_dates() {
        let typed = r#"itemscope itemtype="https://schema.org/NewsArticle""#;
        for extra in [
            r#"<meta itemprop="dateModified" content="2026-11-03">"#.to_owned(),
            format!(r#"<section itemscope itemtype="https://schema.org/NewsArticle" itemid="{URL}#teaser"><h2>Анонс</h2><meta itemprop="datePublished" content="2026-11-03"></section>"#),
            r#"<section itemscope itemtype="https://schema.org/Event"><meta itemprop="datePublished" content="2026-11-03"><time itemprop="startDate" datetime="2026-11-03"></time></section>"#.to_owned(),
            r#"<section><h2><a href="/foreign/">Другой материал</a></h2><time itemprop="datePublished" datetime="2026-11-03"></time></section>"#.to_owned(),
            r#"<article><meta itemprop="datePublished" content="2026-11-03"></article>"#.to_owned(),
            r#"<aside><meta itemprop="datePublished" content="2026-11-03"></aside>"#.to_owned(),
        ] {
            let html = scoped_microdata(typed, &extra);
            assert_eq!(metadata(&html, URL)["date_status"], "missing", "{extra}");
        }
        let date = r#"<time itemprop="datePublished" datetime="2026-11-03"></time>"#;
        let own = format!(r#"<header><a href="{URL}#date">{date}</a></header>"#);
        assert_eq!(
            metadata(&scoped_microdata(typed, &own), URL)["published_date"],
            "2026-11-03"
        );
        let linked = format!(r#"<header><a href="/foreign/">{date}</a></header>"#);
        assert_eq!(
            metadata(&scoped_microdata(typed, &linked), URL)["date_status"],
            "missing"
        );
    }

    #[test]
    fn main_microdata_keeps_conflicts_invalid_dates_and_no_year_inference() {
        let w = window("новости 3 ноября 2026", "2026-11-03");
        for date in ["3 ноября", "03.11.26", "2026-02-30", "сегодня"] {
            let html = scoped_microdata(
                r#"itemscope itemtype="https://schema.org/Article""#,
                &format!(r#"<meta itemprop="datePublished" content="{date}"><p>©2026</p>"#),
            );
            let meta = metadata(&html, "https://example.org/2026/11/03/news/");
            assert_eq!(meta["date_status"], "invalid", "{date}");
            assert_eq!(check_date(&meta, &w)["eligible"], false, "{date}");
        }
        let html = scoped_microdata(
            "",
            r#"<meta itemprop="datePublished" content="2026-11-03"><time itemprop="datePublished" datetime="2026-11-02"></time>"#,
        );
        let meta = metadata(&html, URL);
        assert_eq!(meta["date_status"], "conflict");
        assert_eq!(check_date(&meta, &w)["eligible"], false);
        let html = scoped_microdata(
            r#"itemscope itemtype="https://schema.org/BlogPosting""#,
            r#"<meta itemprop="datePublished" content="2026-11-02T22:30:00Z">"#,
        );
        assert_eq!(metadata(&html, URL)["published_date"], "2026-11-03");
    }

    #[test]
    fn query_scope_preserves_application_selection_and_news_about_apps() {
        assert!(is_query("новости Казани сегодня"));
        assert!(is_query("новости о выпуске приложения для Казани"));
        assert!(!is_query("приложение новости Воронежа для чтения"));
        assert!(!is_query("скачать Новости Воронежа apk"));
        assert!(!is_query("tutorial news website programming"));
        assert!(!is_query("site:news.example.org Rust ownership"));
    }

    #[test]
    fn published_dates_reject_invalid_calendar_unknown_year_and_future() {
        let w = window("новости 3 ноября 2026", "2026-11-03");
        assert!(
            check_date(&metadata(&article("2026-11-03T00:00:00+03:00"), URL), &w)["eligible"]
                == true
        );
        for date in [
            "2026-02-30",
            "3 ноября",
            "03.11.26",
            "2026-13-01",
            "2026-11-03T99:00:00Z",
        ] {
            assert_ne!(
                metadata(&article(date), URL)["date_status"],
                "valid",
                "{date}"
            );
        }
        assert_eq!(
            check_date(&metadata(&article("2026-11-04"), URL), &w)["reason"],
            "future_publication"
        );
        assert_eq!(
            metadata(&article("2026-11-02T22:30:00Z"), URL)["published_date"],
            "2026-11-03"
        );
    }

    #[test]
    fn publication_binding_does_not_borrow_sidebar_event_modified_or_copyright() {
        let html = r#"<html><head><script type="application/ld+json">{"@type":"Event","startDate":"2026-11-03"}</script></head><body><article><h1>Городская новость</h1><time itemprop="dateModified" datetime="2026-11-03"></time><aside><time itemprop="datePublished" datetime="2019-01-01"></time></aside><p>©2026 3 ноября</p></article></body></html>"#;
        assert_eq!(metadata(html, URL)["date_status"], "missing");
        let html = r#"<h1>Городская новость</h1><script type="application/ld+json">{"@type":"NewsArticle","url":"https://example.org/other/","datePublished":"2026-11-03"}</script>"#;
        assert_eq!(metadata(html, URL)["date_status"], "missing");
        let html=article("2026-11-03").replace("</head>",r#"<script type="application/ld+json">{"@type":"NewsArticle","url":"https://example.org/news/city-event/","datePublished":"2026-11-02"}</script></head>"#);
        assert_eq!(metadata(&html, URL)["date_status"], "conflict");
    }

    #[test]
    fn own_jsonld_and_scoped_publication_dates_keep_evidence() {
        let html = r#"<h1>Городская новость</h1><script type="application/ld+json">{"@graph":[{"@type":"WebPage","url":"https://example.org/news/city-event/"},{"@type":"NewsArticle","mainEntityOfPage":{"@id":"https://example.org/news/city-event/"},"datePublished":"2026-11-03T01:05:00+03:00","dateModified":"2026-11-04"}]}</script>"#;
        assert_eq!(metadata(html, URL)["published_date"], "2026-11-03");
        let html = r#"<article><h1>Городская новость</h1><meta itemprop="datePublished" content="3 ноября 2026"><aside><time itemprop="datePublished" datetime="2019-01-01"></time></aside></article>"#;
        assert_eq!(metadata(html, URL)["date_status"], "valid");
        let html = r#"<h1>News app</h1><script type="application/ld+json">{"@type":"SoftwareApplication","url":"https://example.org/news/city-event/","datePublished":"2026-11-03"}</script>"#;
        assert_eq!(metadata(html, URL)["kind"], "application");
        assert_eq!(
            metadata(
                "<h1>Новости</h1><article><h2>A</h2></article><article><h2>B</h2></article>",
                "https://example.org/"
            )["kind"],
            "feed"
        );
    }

    #[test]
    fn calendar_windows_are_explicit_and_fail_closed() {
        assert_eq!(window("новости", "2026-11-03")["start"], "2026-11-01");
        assert_eq!(window("новости вчера", "2026-11-03")["start"], "2026-11-02");
        assert_eq!(
            window("новости ноябрь 2026", "2026-11-03")["end"],
            "2026-11-30"
        );
        assert_eq!(
            window("новости 03.11.2026", "2026-11-03")["start"],
            "2026-11-03"
        );
        assert_eq!(
            window("новости 30 февраля 2026", "2026-11-03")["status"],
            "invalid"
        );
        assert_eq!(
            window("новости 2026", "2026-11-03")["status"],
            "unsupported"
        );
        assert_eq!(
            window("новости сегодня 2 ноября 2026", "2026-11-03")["status"],
            "conflict"
        );
        assert_eq!(
            window("новости 3 4 ноября 2026", "2026-11-03")["status"],
            "unsupported"
        );
        assert_eq!(
            window("новости за прошлый месяц", "2026-11-03")["status"],
            "unsupported"
        );
        assert_eq!(
            window("новости сегодня и вчера", "2026-11-03")["status"],
            "unsupported"
        );
        assert_eq!(
            window("новости ноябрь 2026 декабрь 2026", "2026-11-03")["status"],
            "unsupported"
        );
        assert_eq!(
            window("новости ноябрь2025", "2026-11-03")["status"],
            "unsupported"
        );
        assert_eq!(
            window("новости 03.11.26", "2026-11-03")["status"],
            "unsupported"
        );
    }

    #[test]
    fn navigation_skips_menus_and_unsafe_urls_before_bounded_selection() {
        let html = r##"<a href="/news/">Все новости нашего города</a><a href="/privacy-policy/">Правила использования нашего сайта</a><a href="https://example.org.evil.example/news/some-bad-article/">Новая городская новость с другого домена</a><a href="http://127.1/news/some-bad-article/">Новая городская новость с приватного IP</a><a href="/news/local-city-event/">В городе открыли новый общественный центр</a><a href="/news/another-city-event/">В городе изменятся маршруты транспорта</a>"##;
        let links = links(html, "https://example.org/", "новости", 1).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0]["url"],
            "https://example.org/news/local-city-event/"
        );
        assert_eq!(
            metadata(html, "https://example.org/latest/")["kind"],
            "feed"
        );
    }

    #[test]
    fn conflicting_schema_identity_and_unbound_article_time_are_not_publication_evidence() {
        let conflicting = r#"<h1>Городская новость</h1><script type="application/ld+json">{"@type":"NewsArticle","url":"https://example.org/news/city-event/","mainEntityOfPage":{"@id":"https://example.org/other/"},"datePublished":"2026-11-03"}</script>"#;
        assert_eq!(metadata(conflicting, URL)["date_status"], "missing");
        let outside = r#"<h1>Городская новость</h1><article><p>Новость без собственного заголовка</p><time itemprop="datePublished" datetime="2026-11-03"></time></article>"#;
        assert_eq!(metadata(outside, URL)["date_status"], "missing");
    }

    #[test]
    fn same_day_future_timestamp_is_rejected_even_inside_publication_window() {
        let mut w = window("новости 3 ноября 2026", "2026-11-03");
        w["as_of_timestamp"] = json!("2026-11-03T10:00:00Z");
        let future = metadata(&article("2026-11-03T13:30:00+03:00"), URL);
        assert_eq!(future["published_date"], "2026-11-03");
        assert_eq!(check_date(&future, &w)["reason"], "future_publication");
    }

    #[test]
    fn nested_schema_and_linked_teasers_cannot_date_the_own_article() {
        for nested in [
            r#"<section itemscope itemtype="https://schema.org/NewsArticle"><a href="/news/other/">Другая статья</a><time itemprop="datePublished" datetime="2026-11-03"></time></section>"#,
            r#"<section itemscope itemtype="https://schema.org/Event"><time itemprop="datePublished" datetime="2026-11-03"></time></section>"#,
            r#"<section><h2><a href="/news/other/">Другая статья без microdata</a></h2><time itemprop="datePublished" datetime="2026-11-03"></time></section>"#,
        ] {
            let html = format!(
                r#"<article itemscope itemtype="https://schema.org/NewsArticle"><h1>Городская новость</h1><p>Основная статья без своей даты.</p>{nested}</article>"#
            );
            assert_eq!(metadata(&html, URL)["date_status"], "missing", "{nested}");
        }
        let own = r#"<article itemscope itemtype="https://schema.org/NewsArticle"><header><h1>Городская новость</h1><a href="/author/editor/">Редактор</a><time itemprop="datePublished" datetime="2026-11-03"></time></header><p>Основная статья.</p></article>"#;
        assert_eq!(metadata(own, URL)["published_date"], "2026-11-03");
        let linked = r#"<article><header><h1>Городская новость</h1><a href="/news/other/"><time itemprop="datePublished" datetime="2026-11-03"></time></a></header></article>"#;
        assert_eq!(metadata(linked, URL)["date_status"], "missing");
    }
}
