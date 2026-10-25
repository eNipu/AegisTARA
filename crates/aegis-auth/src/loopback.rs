//! The loopback redirect receiver (RFC 8252 section 7.3).
//!
//! A desktop app cannot own a URL, so it listens on `127.0.0.1` on a random
//! port for the one redirect the browser makes after login. The listener:
//!
//! - binds only to the IPv4 loopback address, so nothing off the machine
//!   can reach it;
//! - answers only `GET /callback` with the expected `Host`, so a web page
//!   cannot reach it through DNS rebinding;
//! - needs the exact `state` it generated, so a forged redirect from another
//!   local process or page is ignored and the wait continues;
//! - reads at most 8 KiB per connection, with a 5 s read timeout;
//! - stops after the caller's deadline.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::error::AuthError;

const MAX_REQUEST_BYTES: usize = 8 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(5);
const PAGE: &str = "<!doctype html><meta charset=utf-8><title>AegisTARA</title>\
<body style=\"font-family:sans-serif;margin:3em\"><h1>Login complete</h1>\
<p>You can close this window and return to AegisTARA.</p></body>";

/// A bound loopback listener.
pub struct Loopback {
    listener: TcpListener,
    redirect_uri: String,
    host: String,
}

impl Loopback {
    /// Binds `127.0.0.1` on a free port.
    ///
    /// # Errors
    ///
    /// [`AuthError::Http`] if binding fails.
    pub async fn bind() -> Result<Self, AuthError> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|e| AuthError::Http(format!("cannot listen on loopback: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| AuthError::Http(e.to_string()))?
            .port();
        let host = format!("127.0.0.1:{port}");
        Ok(Loopback {
            listener,
            redirect_uri: format!("http://{host}/callback"),
            host,
        })
    }

    /// The `redirect_uri` to register in the authorization request.
    #[must_use]
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// Waits for the redirect and returns the authorization code.
    ///
    /// # Errors
    ///
    /// [`AuthError::Timeout`] if no valid redirect arrives in time;
    /// [`AuthError::Idp`] if the provider redirected with an `error`.
    pub async fn wait_for_code(
        self,
        expected_state: &str,
        timeout: Duration,
    ) -> Result<String, AuthError> {
        let wait = async {
            loop {
                let Ok((stream, _)) = self.listener.accept().await else {
                    continue;
                };
                if let Some(result) = self.handle(stream, expected_state).await {
                    return result;
                }
            }
        };
        tokio::time::timeout(timeout, wait)
            .await
            .map_err(|_| AuthError::Timeout)?
    }

    /// Serves one connection. `Some` ends the wait; `None` keeps waiting.
    async fn handle(
        &self,
        mut stream: TcpStream,
        expected_state: &str,
    ) -> Option<Result<String, AuthError>> {
        let request = tokio::time::timeout(READ_TIMEOUT, read_head(&mut stream))
            .await
            .ok()??;
        let Some(parsed) = parse(&request, &self.host) else {
            respond(&mut stream, "400 Bad Request", "bad request").await;
            return None;
        };
        let params = parsed;
        if params.get("state").map(String::as_str) != Some(expected_state) {
            respond(&mut stream, "400 Bad Request", "unexpected state").await;
            return None;
        }
        if let Some(error) = params.get("error") {
            respond(&mut stream, "400 Bad Request", "login was refused").await;
            return Some(Err(AuthError::Idp(sanitize_code(error))));
        }
        match params.get("code") {
            Some(code) if !code.is_empty() && code.len() <= 2048 => {
                respond(&mut stream, "200 OK", PAGE).await;
                Some(Ok(code.clone()))
            }
            _ => {
                respond(&mut stream, "400 Bad Request", "missing code").await;
                None
            }
        }
    }
}

/// Keeps an OAuth error code short and printable.
pub(crate) fn sanitize_code(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .take(64)
        .collect()
}

async fn read_head(stream: &mut TcpStream) -> Option<Vec<u8>> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            return Some(buf);
        }
        if buf.len() >= MAX_REQUEST_BYTES {
            return None;
        }
    }
}

/// Returns the query parameters of a valid `GET /callback` for `host`.
fn parse(request: &[u8], host: &str) -> Option<std::collections::BTreeMap<String, String>> {
    let text = std::str::from_utf8(request).ok()?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, target, version) = (first.next()?, first.next()?, first.next()?);
    if method != "GET" || !version.starts_with("HTTP/1.") {
        return None;
    }
    let host_ok = lines
        .take_while(|l| !l.is_empty())
        .filter_map(|l| l.split_once(':'))
        .any(|(n, v)| n.eq_ignore_ascii_case("host") && v.trim() == host);
    if !host_ok {
        return None;
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != "/callback" {
        return None;
    }
    let decode = |s: &str| {
        percent_encoding::percent_decode_str(&s.replace('+', " "))
            .decode_utf8()
            .ok()
            .map(|c| c.into_owned())
    };
    let mut params = std::collections::BTreeMap::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        params.insert(decode(k)?, decode(v)?);
    }
    Some(params)
}

async fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\n\
         cache-control: no-store\r\nreferrer-policy: no-referrer\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    // The browser may already be gone; nothing to do about it.
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn get(addr: &str, path: &str, host: &str) -> String {
        let mut s = TcpStream::connect(addr).await.expect("connect");
        let req = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\n\r\n");
        s.write_all(req.as_bytes()).await.expect("write");
        let mut out = String::new();
        s.read_to_string(&mut out).await.expect("read");
        out
    }

    #[tokio::test]
    async fn returns_the_code_for_the_expected_state() {
        let lb = Loopback::bind().await.expect("bind");
        let host = lb.host.clone();
        let waiter = tokio::spawn(lb.wait_for_code("s1", Duration::from_secs(5)));
        let reply = get(&host, "/callback?code=abc%20d&state=s1", &host).await;
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
        assert_eq!(waiter.await.expect("task").expect("code"), "abc d");
    }

    #[tokio::test]
    async fn forged_requests_do_not_end_the_wait() {
        let lb = Loopback::bind().await.expect("bind");
        let host = lb.host.clone();
        let waiter = tokio::spawn(lb.wait_for_code("good", Duration::from_secs(5)));
        // Wrong state, wrong path, wrong Host (DNS rebinding), missing code.
        for (path, h) in [
            ("/callback?code=evil&state=bad", host.as_str()),
            ("/other?code=evil&state=good", host.as_str()),
            ("/callback?code=evil&state=good", "evil.example:80"),
            ("/callback?state=good", host.as_str()),
        ] {
            let reply = get(&host, path, h).await;
            assert!(reply.starts_with("HTTP/1.1 400"), "{path}: {reply}");
        }
        assert!(!waiter.is_finished());
        get(&host, "/callback?code=real&state=good", &host).await;
        assert_eq!(waiter.await.expect("task").expect("code"), "real");
    }

    #[tokio::test]
    async fn a_provider_error_is_reported_without_echoing_unsafe_text() {
        let lb = Loopback::bind().await.expect("bind");
        let host = lb.host.clone();
        let waiter = tokio::spawn(lb.wait_for_code("s", Duration::from_secs(5)));
        get(
            &host,
            "/callback?error=access_denied%3Cscript%3E&state=s",
            &host,
        )
        .await;
        assert_eq!(
            waiter.await.expect("task"),
            Err(AuthError::Idp("access_deniedscript".into()))
        );
    }

    #[tokio::test]
    async fn waiting_times_out() {
        let lb = Loopback::bind().await.expect("bind");
        assert_eq!(
            lb.wait_for_code("s", Duration::from_millis(50)).await,
            Err(AuthError::Timeout)
        );
    }

    #[test]
    fn only_get_callback_with_the_right_host_parses() {
        let ok = b"GET /callback?a=1&b=x+y HTTP/1.1\r\nHost: h:1\r\n\r\n";
        let params = parse(ok, "h:1").expect("parses");
        assert_eq!(params["a"], "1");
        assert_eq!(params["b"], "x y");
        assert!(parse(b"POST /callback HTTP/1.1\r\nHost: h:1\r\n\r\n", "h:1").is_none());
        assert!(parse(b"GET /callback HTTP/1.1\r\n\r\n", "h:1").is_none());
        assert!(parse(b"GET /callback HTTP/2\r\nHost: h:1\r\n\r\n", "h:1").is_none());
        assert!(parse(b"\xff\xfe", "h:1").is_none());
    }
}
