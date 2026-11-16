use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use chromiumoxide::{
    Browser, BrowserConfig,
    cdp::browser_protocol::{
        fetch::{EventRequestPaused, FailRequestParams, FulfillRequestParams},
        network::ErrorReason,
    },
};
use futures::StreamExt;
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn config(
    executable: Option<&Path>,
    profile: &Path,
    proxy_port: Option<u16>,
) -> Result<BrowserConfig> {
    let mut builder = BrowserConfig::builder()
        .new_headless_mode()
        .user_data_dir(profile)
        .incognito()
        .respect_https_errors()
        .enable_request_intercept()
        .disable_cache()
        .arg("disable-background-networking")
        .arg("disable-component-update")
        .request_timeout(Duration::from_secs(10));
    if let Some(path) = executable {
        builder = builder.chrome_executable(path);
    }
    if let Some(port) = proxy_port {
        let proxy = format!("http://127.0.0.1:{port}");
        builder = builder
            .arg(("proxy-server", proxy.as_str()))
            .arg(("proxy-bypass-list", "<-loopback>"));
    }
    builder.build().map_err(anyhow::Error::msg)
}

// This is an isolated capability test, not an enabled public browser fetcher.
pub async fn probe(executable: Option<&Path>) -> Result<Value> {
    tokio::time::timeout(Duration::from_secs(45), probe_inner(executable))
        .await
        .context("browser probe timeout")?
}

async fn probe_inner(executable: Option<&Path>) -> Result<Value> {
    let profile = tempfile::tempdir()?;
    let mut tasks = tokio::task::JoinSet::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let sink_hits = Arc::new(AtomicUsize::new(0));
    let sink_count = sink_hits.clone();
    let observed = Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
    let sink_observed = observed.clone();
    tasks.spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            sink_count.fetch_add(1, Ordering::SeqCst);
            let observed = sink_observed.clone();
            tokio::spawn(async move {
                let mut buf = [0; 1024];
                if let Ok(Ok(n)) =
                    tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf)).await
                {
                    let first = String::from_utf8_lossy(&buf[..n])
                        .lines()
                        .next()
                        .unwrap_or("")
                        .to_owned();
                    observed.lock().await.push(first);
                }
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
            });
        }
    });
    let proxy = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let proxy_port = proxy.local_addr()?.port();
    let proxy_hits = Arc::new(AtomicUsize::new(0));
    let proxy_count = proxy_hits.clone();
    // shortcut: this diagnostic proxy rejects all traffic; qualify a public-only broker before enabling browser fetches.
    tasks.spawn(async move {
        while let Ok((mut stream, _)) = proxy.accept().await {
            proxy_count.fetch_add(1, Ordering::SeqCst);
            let _ = stream
                .write_all(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await;
        }
    });
    let config = config(executable, profile.path(), Some(proxy_port))?;
    let (mut browser, mut handler) = Browser::launch(config).await?;
    tasks.spawn(async move {
        while let Some(event) = handler.next().await {
            if event.is_err() {
                break;
            }
        }
    });
    let page = Arc::new(browser.new_page("about:blank").await?);
    let mut paused = page.event_listener::<EventRequestPaused>().await?;
    let fixture_url = "http://fixture.invalid/".to_owned();
    let html = format!(
        r#"<!doctype html><html><head><title>Isolated JS probe</title></head><body><article><h1>Loading</h1></article><script>
        document.querySelector('h1').textContent='JS_RENDER_OK';
        fetch('http://127.0.0.1:{port}/private').catch(()=>{{}});
        const ws=new WebSocket('ws://127.0.0.1:{port}/ws'); ws.onerror=()=>{{}};
        </script></body></html>"#
    );
    let blocked = Arc::new(AtomicUsize::new(0));
    let block_count = blocked.clone();
    let intercept_page = page.clone();
    let target = fixture_url.clone();
    tasks.spawn(async move {
        while let Some(event) = paused.next().await {
            let result = if event.request.url == target {
                match FulfillRequestParams::builder()
                    .request_id(event.request_id.clone())
                    .response_code(200)
                    .body(STANDARD.encode(&html))
                    .build()
                {
                    Ok(params) => intercept_page.execute(params).await.map(|_| ()),
                    Err(_) => break,
                }
            } else {
                block_count.fetch_add(1, Ordering::SeqCst);
                intercept_page
                    .execute(FailRequestParams::new(
                        event.request_id.clone(),
                        ErrorReason::BlockedByClient,
                    ))
                    .await
                    .map(|_| ())
            };
            if result.is_err() {
                break;
            }
        }
    });
    let run=async {
        page.goto(&fixture_url).await?;
        tokio::time::sleep(Duration::from_secs(2)).await;
        let content=page.content().await?;
        Ok::<_,anyhow::Error>(json!({"js_rendered":content.contains("JS_RENDER_OK"),"blocked_http_requests":blocked.load(Ordering::SeqCst),"connections_to_private_sink":sink_hits.load(Ordering::SeqCst),"private_request_lines":*observed.lock().await,"denied_proxy_connections":proxy_hits.load(Ordering::SeqCst),"browser_transport_policy":"deny_all_proxy; implicit loopback bypass removed","public_browser_enabled":false,"full_ssrf_protection_proven":false,"dns_rebinding_tested":false,"scope":"One isolated target and a synthetic page with a deny-all transport proxy. Public fetching, new targets, frames, WebRTC and DNS rebinding require separate qualification."}))
    }.await;
    let _ = browser.close().await;
    tasks.abort_all();
    run
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn emits_effective_browser_switches() {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("fake-chromium");
        std::fs::write(&executable, "#!/bin/sh\nprintf '%s\\n' \"$@\" >&2\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let output = config(Some(&executable), dir.path(), Some(12345))
            .unwrap()
            .launch()
            .unwrap()
            .into_inner()
            .wait_with_output()
            .await
            .unwrap();
        let arguments = String::from_utf8(output.stderr).unwrap();
        assert!(
            arguments.lines().any(|a| a == "--disable-component-update"),
            "{arguments}"
        );
        assert!(
            !arguments.lines().any(|a| a.starts_with("----")),
            "{arguments}"
        );
        assert!(!arguments.lines().any(|a| a == "--no-sandbox"));
        assert!(
            arguments
                .lines()
                .any(|a| a == "--proxy-server=http://127.0.0.1:12345")
        );
        assert!(
            arguments
                .lines()
                .any(|a| a == "--proxy-bypass-list=<-loopback>")
        );
    }

    #[tokio::test]
    #[ignore = "requires installed Chromium and local network permission"]
    async fn blocks_private_connections() {
        let result = probe(None).await.unwrap();
        assert_eq!(result["js_rendered"], true, "{result}");
        assert_eq!(result["connections_to_private_sink"], 0, "{result}");
        assert!(
            result["denied_proxy_connections"].as_u64().unwrap() > 0,
            "{result}"
        );
    }
}
