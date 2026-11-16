use anyhow::{Context, Result, bail, ensure};
use reqwest::{Client, Url, redirect::Policy};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{OnceLock, RwLock},
    time::{Duration, Instant},
};

pub const MAX_HTML: usize = 5 * 1024 * 1024;
static LAST_REQUEST: OnceLock<tokio::sync::Mutex<HashMap<String, Instant>>> = OnceLock::new();
static BLOCKED_HOSTS: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();
pub fn set_blocked_hosts(hosts: HashMap<String, String>) {
    *BLOCKED_HOSTS
        .get_or_init(Default::default)
        .write()
        .expect("host state lock") = hosts;
}
fn check_host(url: &Url) -> Result<()> {
    if let Some(reason) = BLOCKED_HOSTS
        .get_or_init(Default::default)
        .read()
        .expect("host state lock")
        .get(url.host_str().unwrap_or(""))
    {
        bail!("host_disabled: {reason}");
    }
    Ok(())
}

pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(matches!(a, 0 | 10 | 127 | 224..=255)
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && (b == 168 || (b == 0 && c <= 2) || (b == 88 && c == 99)))
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            // Conservative global-unicast subset; special-purpose ranges stay blocked.
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8))
                && s[0] != 0x2002
                && !(s[0] == 0x3fff && s[1] < 0x1000)
        }
    }
}

pub fn validate_url(raw: &str) -> Result<Url> {
    ensure!(raw.chars().count() <= 4096, "unsafe_url: URL too long");
    let url = Url::parse(raw).context("unsafe_url: invalid URL")?;
    ensure!(
        matches!(url.scheme(), "http" | "https"),
        "unsafe_url: scheme"
    );
    let authority = raw
        .split_once("://")
        .map(|(_, s)| s.split('/').next().unwrap_or(""))
        .unwrap_or("");
    ensure!(!authority.contains('@'), "unsafe_url: userinfo");
    let host = url
        .host_str()
        .context("unsafe_url: no host")?
        .trim_end_matches('.');
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
        ensure!(public_ip(ip), "unsafe_url: non-public IP");
    } else {
        ensure!(host.contains('.'), "unsafe_url: local hostname");
        for suffix in ["localhost", "local", "internal", "lan", "home", "onion"] {
            ensure!(
                host != suffix && !host.ends_with(&format!(".{suffix}")),
                "unsafe_url: local hostname"
            );
        }
    }
    Ok(url)
}

pub fn checked_addrs(addrs: Vec<SocketAddr>) -> Result<Vec<SocketAddr>> {
    ensure!(
        !addrs.is_empty() && addrs.iter().all(|a| public_ip(a.ip())),
        "unsafe_url: DNS has non-public address"
    );
    Ok(addrs)
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct CacheInfo {
    pub request_url: String,
    pub fetched_at: i64,
    pub checked_at: i64,
    pub expires_at: i64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub allowed: bool,
    pub hit: bool,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Page {
    pub url: String,
    pub status: u16,
    pub html: String,
    pub hops: Vec<String>,
    #[serde(default)]
    pub cache: CacheInfo,
}

impl Page {
    fn storage_info(&self) -> CacheInfo {
        let mut info = self.cache.clone();
        let kind = crate::news::metadata(&self.html, &self.url)["kind"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        if matches!(kind.as_str(), "article" | "feed") {
            info.expires_at = info
                .expires_at
                .min(info.checked_at + crate::cache::NEWS_TTL);
        }
        info
    }
    /// Call only after a consumer has accepted the page, never for a parser error/challenge.
    pub async fn store(&self) -> CacheInfo {
        if !crate::cache::enabled() {
            return self.cache.clone();
        }
        let mut page = self.clone();
        if crate::cache::enabled() && self.status == 200 && !self.cache.allowed {
            crate::cache::remove("html-v1", self.cache.request_url.clone()).await;
        }
        if crate::cache::enabled()
            && self.status == 200
            && self.cache.allowed
            && !self.cache.hit
            && crate::search::navigation_guard(&self.html).is_ok()
        {
            page.cache = self.storage_info();
            crate::cache::write(
                "html-v1",
                page.cache.request_url.clone(),
                page.cache.request_url.clone(),
                serde_json::to_value(&page).expect("page JSON"),
                page.cache.fetched_at,
                page.cache.checked_at,
                page.cache.expires_at,
            )
            .await;
        }
        page.cache
    }
}

pub async fn fetch(raw: &str) -> Result<Page> {
    fetch_guarded(raw, |_| Ok(())).await
}
pub async fn fetch_with_ttl(raw: &str, ttl: i64) -> Result<Page> {
    fetch_guarded_with_ttl(raw, ttl, |_| Ok(())).await
}

pub async fn fetch_guarded(
    raw: &str,
    check: impl FnMut(&Url) -> Result<()> + Send,
) -> Result<Page> {
    fetch_guarded_with_ttl(raw, crate::cache::DOCUMENT_TTL, check).await
}
pub async fn fetch_guarded_with_ttl(
    raw: &str,
    ttl: i64,
    check: impl FnMut(&Url) -> Result<()> + Send,
) -> Result<Page> {
    tokio::time::timeout(Duration::from_secs(15), fetch_inner(raw, ttl, check))
        .await
        .context("fetch_timeout")?
}

fn cached_page_valid(page: &Page, request: &str) -> bool {
    page.status == 200
        && page.cache.allowed
        && page.cache.request_url == request
        && page.html.len() <= MAX_HTML
        && (1..=6).contains(&page.hops.len())
        && page.hops.first().is_some_and(|s| s == request)
        && page.hops.last().is_some_and(|s| s == &page.url)
        && page.hops.iter().all(|s| validate_url(s).is_ok())
        && page.cache.fetched_at <= page.cache.checked_at
}

fn reusable(info: &CacheInfo, at: i64, ttl: i64, refresh: bool) -> bool {
    !refresh
        && info.checked_at <= at
        && info.expires_at.min(info.checked_at.saturating_add(ttl)) > at
}

async fn public_dns(url: &Url) -> Result<Vec<SocketAddr>> {
    checked_addrs(
        tokio::net::lookup_host((
            url.host_str()
                .context("unsafe_url: no host")?
                .trim_matches(['[', ']']),
            url.port_or_known_default().context("unsafe_url: no port")?,
        ))
        .await?
        .collect(),
    )
}

fn storage_policy(headers: &reqwest::header::HeaderMap, ttl: i64) -> (bool, i64) {
    let control = headers
        .get_all("cache-control")
        .iter()
        .filter_map(|s| s.to_str().ok())
        .collect::<Vec<_>>()
        .join(",")
        .to_ascii_lowercase();
    let vary = headers
        .get_all("vary")
        .iter()
        .filter_map(|s| s.to_str().ok())
        .collect::<Vec<_>>()
        .join(",");
    let age = headers
        .get("age")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let directives: Vec<_> = control.split(',').map(str::trim).collect();
    let ttl = if directives.contains(&"no-cache") {
        0
    } else {
        directives
            .iter()
            .filter_map(|s| s.strip_prefix("max-age="))
            .find_map(|s| s.trim_matches('"').parse::<i64>().ok())
            .map(|max_age| max_age.saturating_sub(age))
            .unwrap_or(ttl)
            .clamp(0, ttl)
    };
    (
        !directives.contains(&"no-store") && !vary.split(',').any(|s| s.trim() == "*"),
        ttl,
    )
}

fn revalidated(
    mut page: Page,
    url: &str,
    hops: Vec<String>,
    headers: &reqwest::header::HeaderMap,
    ttl: i64,
    at: i64,
) -> Result<Page> {
    ensure!(
        page.url == url && page.cache.checked_at <= at,
        "304 representation mismatch or clock rollback"
    );
    let (allowed, max_age) = storage_policy(headers, ttl);
    page.hops = hops;
    page.cache.checked_at = at;
    page.cache.expires_at = at + max_age;
    page.cache.allowed &= allowed;
    page.cache.hit = false;
    if let Some(value) = headers.get("etag").and_then(|s| s.to_str().ok()) {
        page.cache.etag = Some(value.into());
    }
    if let Some(value) = headers.get("last-modified").and_then(|s| s.to_str().ok()) {
        page.cache.last_modified = Some(value.into());
    }
    Ok(page)
}

async fn fetch_inner(
    raw: &str,
    ttl: i64,
    mut check: impl FnMut(&Url) -> Result<()> + Send,
) -> Result<Page> {
    let mut url = validate_url(raw)?;
    url.set_fragment(None);
    check_host(&url)?;
    check(&url)?;
    let request_url = url.to_string();
    let saved = crate::cache::read("html-v1", request_url.clone(), true)
        .await
        .and_then(|v| serde_json::from_value::<Page>(v).ok())
        .filter(|page| cached_page_valid(page, &request_url))
        .map(|mut page| {
            page.cache = page.storage_info();
            page
        });
    if let Some(page) = &saved
        && reusable(
            &page.cache,
            crate::cache::now(),
            ttl,
            crate::cache::refresh(),
        )
    {
        let mut targets = Vec::new();
        for (index, hop) in page.hops.iter().enumerate() {
            let hop = validate_url(hop)?;
            check_host(&hop)?;
            if index > 0 {
                check(&hop)?;
            }
            targets.push(hop);
        }
        for hop in targets {
            public_dns(&hop).await?;
        }
        let mut page = page.clone();
        page.cache.hit = true;
        return Ok(page);
    }
    let mut hops = Vec::new();
    for redirects in 0..=5 {
        if redirects > 0 {
            check_host(&url)?;
            check(&url)?;
        }
        let host = url
            .host_str()
            .context("unsafe_url: no host")?
            .trim_matches(['[', ']'])
            .to_owned();
        let addrs = public_dns(&url).await?;
        let mut times = LAST_REQUEST.get_or_init(Default::default).lock().await;
        if let Some(last) = times.get(&host) {
            tokio::time::sleep(Duration::from_secs(2).saturating_sub(last.elapsed())).await;
        }
        times.insert(host.clone(), Instant::now());
        drop(times);
        let client = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .referer(false)
            .timeout(Duration::from_secs(15))
            .resolve_to_addrs(&host, &addrs)
            .user_agent("web-search-feasibility/0.1 (local research pilot)")
            .build()?;
        hops.push(url.to_string());
        let mut request = client.get(url.clone());
        if let Some(saved) = &saved
            && saved.url == url.as_str()
        {
            if let Some(etag) = &saved.cache.etag {
                request = request.header("if-none-match", etag);
            }
            if let Some(date) = &saved.cache.last_modified {
                request = request.header("if-modified-since", date);
            }
        }
        crate::cache::http_read();
        let mut response = request.send().await?;
        let peer = response
            .remote_addr()
            .context("unsafe_url: peer unavailable")?;
        ensure!(
            addrs.iter().any(|a| a.ip() == peer.ip()) && public_ip(peer.ip()),
            "unsafe_url: peer mismatch"
        );
        if response.status().as_u16() == 304 {
            let saved = saved.clone().context("304 without cached representation")?;
            return revalidated(
                saved,
                url.as_str(),
                hops,
                response.headers(),
                ttl,
                crate::cache::now(),
            );
        }
        if response.status().is_redirection() {
            ensure!(redirects < 5, "too_many_redirects");
            let location = response
                .headers()
                .get("location")
                .context("redirect without location")?
                .to_str()?;
            url = validate_url(url.join(location)?.as_str())?;
            url.set_fragment(None);
            continue;
        }
        let status = response.status().as_u16();
        ensure!(status != 429, "rate_limited: no retry in this probe");
        ensure!(status != 403, "source_blocked: no retry in this probe");
        let (allowed, max_age) = storage_policy(response.headers(), ttl);
        let cache = CacheInfo {
            request_url: request_url.clone(),
            fetched_at: crate::cache::now(),
            checked_at: crate::cache::now(),
            expires_at: crate::cache::now() + max_age,
            etag: response
                .headers()
                .get("etag")
                .and_then(|s| s.to_str().ok())
                .map(str::to_owned),
            last_modified: response
                .headers()
                .get("last-modified")
                .and_then(|s| s.to_str().ok())
                .map(str::to_owned),
            allowed,
            hit: false,
        };
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        ensure!(
            content_type.starts_with("text/html")
                || content_type.starts_with("application/xhtml+xml"),
            "unsupported_content"
        );
        let charset = content_type
            .split(';')
            .find_map(|x| x.trim().strip_prefix("charset="))
            .unwrap_or("utf-8");
        ensure!(
            charset.eq_ignore_ascii_case("utf-8") || charset.eq_ignore_ascii_case("utf8"),
            "unsupported_content: probe supports UTF-8 only"
        );
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                body.len() + chunk.len() <= MAX_HTML,
                "unsupported_content: body limit"
            );
            body.extend_from_slice(&chunk);
        }
        return Ok(Page {
            url: url.to_string(),
            status,
            html: String::from_utf8(body).context("invalid UTF-8")?,
            hops,
            cache,
        });
    }
    bail!("too_many_redirects")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cached() -> Page {
        Page {
            url: "https://example.org/a?q=1".into(),
            status: 200,
            html: "<p>body</p>".into(),
            hops: vec!["https://example.org/a?q=1".into()],
            cache: CacheInfo {
                request_url: "https://example.org/a?q=1".into(),
                fetched_at: 10,
                checked_at: 20,
                expires_at: 30,
                etag: Some("\"v1\"".into()),
                allowed: true,
                ..Default::default()
            },
        }
    }

    #[test]
    fn cached_provenance_and_http_storage_rules() {
        let mut p = cached();
        let request = p.url.clone();
        assert!(cached_page_valid(&p, &request));
        assert!(reusable(&p.cache, 29, 300, false));
        assert!(!reusable(&p.cache, 29, 300, true));
        assert!(!reusable(&p.cache, 30, 300, false));
        assert!(!reusable(&p.cache, 19, 300, false));
        assert!(!reusable(&p.cache, 26, 5, false));
        assert!(!cached_page_valid(&p, "https://example.org/a?q=2"));
        p.hops.push("http://127.0.0.1/".into());
        p.url = "http://127.0.0.1/".into();
        assert!(!cached_page_valid(&p, &request));
        p = cached();
        p.status = 403;
        assert!(!cached_page_valid(&p, &request));
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("cache-control", "private, max-age=60".parse().unwrap());
        assert_eq!(storage_policy(&h, 300), (true, 60));
        h.insert("age", "45".parse().unwrap());
        assert_eq!(storage_policy(&h, 300), (true, 15));
        h.remove("age");
        h.insert("cache-control", "no-cache".parse().unwrap());
        assert_eq!(storage_policy(&h, 300), (true, 0));
        h.insert("cache-control", "public, no-store".parse().unwrap());
        assert!(!storage_policy(&h, 300).0);
        h.clear();
        h.insert("vary", "*".parse().unwrap());
        assert!(!storage_policy(&h, 300).0);
    }

    #[test]
    fn conditional_304_preserves_body_version_and_fetched_time() {
        let p = cached();
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("cache-control", "max-age=60".parse().unwrap());
        h.insert("etag", "\"v2\"".parse().unwrap());
        let updated = revalidated(p.clone(), &p.url, p.hops.clone(), &h, 300, 40).unwrap();
        assert_eq!(updated.html, p.html);
        assert_eq!(updated.cache.fetched_at, 10);
        assert_eq!(updated.cache.checked_at, 40);
        assert_eq!(updated.cache.expires_at, 100);
        assert_eq!(updated.cache.etag.as_deref(), Some("\"v2\""));
        assert!(
            revalidated(
                p.clone(),
                "https://example.org/other",
                p.hops.clone(),
                &h,
                300,
                40
            )
            .is_err()
        );
        assert!(revalidated(p.clone(), &p.url, p.hops.clone(), &h, 300, 19).is_err());
    }

    #[test]
    fn standalone_feed_and_article_ttl_cannot_outlive_news_window() {
        let mut page = cached();
        page.url = "https://example.org/news/".into();
        page.cache.expires_at = page.cache.checked_at + crate::cache::DOCUMENT_TTL;
        page.html="<html><head><title>Новости города</title></head><body><h1>Новости</h1><article><h2><a href='/one/'>Первая новость</a></h2></article><article><h2><a href='/two/'>Вторая новость</a></h2></article></body></html>".into();
        assert_eq!(crate::news::metadata(&page.html, &page.url)["kind"], "feed");
        assert_eq!(
            page.storage_info().expires_at,
            page.cache.checked_at + crate::cache::NEWS_TTL
        );
        assert!(
            !reusable(
                &page.storage_info(),
                page.cache.checked_at + 301,
                crate::cache::DOCUMENT_TTL,
                false
            ),
            "a wide legacy feed expiry cannot bypass the current news TTL"
        );
        page.html="<html><head><meta property='og:type' content='article'><meta property='article:published_time' content='2026-11-05T10:00:00+03:00'></head><body><article><h1>Событие</h1><p>Содержание события.</p></article></body></html>".into();
        assert_eq!(
            crate::news::metadata(&page.html, &page.url)["kind"],
            "article"
        );
        assert_eq!(
            page.storage_info().expires_at,
            page.cache.checked_at + crate::cache::NEWS_TTL
        );
        page.cache.expires_at = page.cache.checked_at + 10;
        assert_eq!(
            page.storage_info().expires_at,
            page.cache.checked_at + 10,
            "server freshness cannot be extended"
        );
    }

    #[test]
    fn rejects_local_and_encoded_addresses() {
        for url in [
            "file:///etc/passwd",
            "http://localhost/",
            "http://x.local/",
            "http://127.1/",
            "http://2130706433/",
            "http://0x7f000001/",
            "http://10.0.0.1/",
            "http://169.254.169.254/",
            "http://[::1]/",
            "http://[::ffff:127.0.0.1]/",
            "http://[fe80::1]/",
            "http://[fd00::1]/",
            "https://user:pass@example.com/",
            "https://@example.com/",
        ] {
            assert!(validate_url(url).is_err(), "{url}");
        }
        assert!(validate_url("https://example.com/path?q=1").is_ok());
    }

    #[test]
    fn rejects_mixed_dns_and_special_ranges() {
        assert!(
            checked_addrs(vec![
                "8.8.8.8:443".parse().unwrap(),
                "127.0.0.1:443".parse().unwrap()
            ])
            .is_err()
        );
        for ip in [
            "0.0.0.0",
            "100.64.0.1",
            "192.0.2.1",
            "198.19.1.1",
            "224.0.0.1",
            "2001:db8::1",
            "2002:7f00:1::1",
            "3fff::1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(public_ip("2606:4700:4700::1111".parse().unwrap()));
    }
}
