use crate::prelude::*;

pub(crate) async fn login(runtime: &Runtime, open_browser: bool) -> Result<()> {
    let listener = TcpListener::bind(CALLBACK_ADDR)
        .await
        .into_diagnostic()
        .wrap_err_with(|| format!("binding OAuth callback listener on {CALLBACK_ADDR}"))?;
    // RFC 7636 S256 PKCE plus a state nonce: with a public client and a fixed
    // loopback port, a local attacker who wins the port race could otherwise
    // redeem a stolen authorization code. Hex is a valid verifier charset and
    // 64 chars sits inside the 43-128 range the RFC requires.
    let code_verifier = hex::encode(random_bytes::<32>()?);
    let code_challenge = pkce_challenge(&code_verifier);
    let state = hex::encode(random_bytes::<16>()?);
    let mut auth_url = Url::parse(AUTH_URL)
        .into_diagnostic()
        .wrap_err("parsing OAuth URL")?;
    auth_url
        .query_pairs_mut()
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("response_type", "code")
        .append_pair("scope", "read write")
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state);

    println!("Open this URL in your browser:");
    println!("{auth_url}");
    println!("Or paste the callback URL/code here:");

    if open_browser {
        let status = ProcessCommand::new("open")
            .arg(auth_url.as_str())
            .status()
            .into_diagnostic()
            .wrap_err("running macOS open")?;
        if !status.success() {
            warn!(?status, "system browser open returned a non-zero status");
        }
    }

    let code = tokio::select! {
        code = oauth_code_from_listener(listener, &state) => code?,
        // The pasted-code path cannot carry state; the browser callback path
        // verifies it, and the token exchange still requires the verifier.
        code = oauth_code_from_stdin() => code?,
    };
    let auth = runtime
        .exchange_code(&code, &code_verifier)
        .await
        .map_err(|source| MeshError::auth_with_source("OAuth code exchange failed", source))?;
    runtime.write_config(&MeshConfig {
        auth: Some(auth.clone()),
        user: None,
    })?;
    let user = runtime.current_user().await.ok();
    runtime.write_config(&MeshConfig {
        auth: Some(auth),
        user,
    })?;
    println!(
        "Login successful. Token saved to {}",
        runtime.config_path.display()
    );
    Ok(())
}

pub(crate) async fn oauth_code_from_listener(
    listener: TcpListener,
    expected_state: &str,
) -> Result<String> {
    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .into_diagnostic()
            .wrap_err("accepting OAuth callback")?;
        let mut buffer = vec![0_u8; 8192];
        let read = stream
            .read(&mut buffer)
            .await
            .into_diagnostic()
            .wrap_err("reading OAuth callback")?;
        let request = String::from_utf8_lossy(&buffer[..read]);
        let first_line = request.lines().next().unwrap_or_default();
        match callback_from_http_request_line(first_line) {
            Ok(callback) => {
                if callback.state.as_deref() != Some(expected_state) {
                    let body = "<html><body><h1>me.sh login failed</h1><p>State mismatch.</p></body></html>";
                    write_http_response(&mut stream, 400, body).await?;
                    return auth_err(
                        "OAuth callback state mismatch; aborting login. Re-run `mesh login` and use the freshly printed URL.",
                    );
                }
                let body = "<html><body><h1>Logged in to me.sh</h1><p>You can close this tab.</p></body></html>";
                write_http_response(&mut stream, 200, body).await?;
                return Ok(callback.code);
            }
            Err(error) => {
                let body = format!(
                    "<html><body><h1>me.sh login failed</h1><p>{}</p></body></html>",
                    html_escape(&error.to_string())
                );
                write_http_response(&mut stream, 400, &body).await?;
            }
        }
    }
}

pub(crate) async fn oauth_code_from_stdin() -> Result<String> {
    let mut line = String::new();
    let mut reader = BufReader::new(tokio::io::stdin());
    let read = reader
        .read_line(&mut line)
        .await
        .into_diagnostic()
        .wrap_err("reading OAuth callback from stdin")?;
    if read == 0 {
        // stdin EOF (e.g. `mesh login </dev/null` or a non-interactive shell):
        // never resolve this branch, so the select! in `login` keeps the
        // loopback listener alive for the browser callback.
        debug!("stdin closed; waiting on the browser callback listener");
        return std::future::pending().await;
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return auth_err("no callback URL or code was pasted");
    }
    code_from_callback_text(trimmed)
}

pub(crate) async fn write_http_response(
    stream: &mut tokio::net::TcpStream,
    status: u16,
    body: &str,
) -> Result<()> {
    let status_text = if status == 200 { "OK" } else { "Bad Request" };
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .await
        .into_diagnostic()
        .wrap_err("writing OAuth browser response")?;
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OauthCallback {
    pub(crate) code: String,
    pub(crate) state: Option<String>,
}

pub(crate) fn callback_from_http_request_line(line: &str) -> Result<OauthCallback> {
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    if method != "GET" {
        return auth_err("OAuth callback must be a GET request");
    }
    let url = Url::parse(&format!("{REDIRECT_URI}{path}"))
        .into_diagnostic()
        .wrap_err("parsing OAuth callback")?;
    callback_from_url(&url)
}

pub(crate) fn code_from_callback_text(text: &str) -> Result<String> {
    if let Ok(url) = Url::parse(text) {
        return callback_from_url(&url).map(|callback| callback.code);
    }
    Ok(text.to_string())
}

pub(crate) fn callback_from_url(url: &Url) -> Result<OauthCallback> {
    let mut error = None;
    let mut code = None;
    let mut state = None;
    for (key, value) in url.query_pairs() {
        if key == "error" {
            error = Some(value.to_string());
        }
        if key == "code" {
            code = Some(value.to_string());
        }
        if key == "state" {
            state = Some(value.to_string());
        }
    }
    if let Some(error) = error {
        return auth_err(format!("OAuth error: {error}"));
    }
    let code = code.ok_or_else(|| MeshError::auth("OAuth callback did not include code"))?;
    Ok(OauthCallback { code, state })
}

/// `N` random bytes from the OS. On unix this reads `/dev/urandom` directly
/// to avoid a new dependency; there is no non-unix fallback because guessable
/// PKCE/state values would silently defeat the point.
pub(crate) fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    #[cfg(unix)]
    {
        let mut bytes = [0_u8; N];
        fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut bytes))
            .into_diagnostic()
            .wrap_err("reading OS randomness from /dev/urandom")?;
        Ok(bytes)
    }
    #[cfg(not(unix))]
    {
        err("mesh login requires a unix OS randomness source (/dev/urandom)")
    }
}

/// RFC 7636 S256: BASE64URL-nopad(SHA256(ASCII(code_verifier))).
pub(crate) fn pkce_challenge(code_verifier: &str) -> String {
    base64url_nopad(Sha256::digest(code_verifier.as_bytes()).as_slice())
}

/// RFC 4648 base64url without padding, hand-rolled so we do not pull in a
/// base64 crate for the one PKCE challenge this CLI ever encodes.
pub(crate) fn base64url_nopad(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let triple = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        out.push(ALPHABET[(triple >> 18) as usize & 63] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(triple >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[triple as usize & 63] as char);
        }
    }
    out
}

pub(crate) fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(crate) fn logout(runtime: &Runtime) -> Result<()> {
    // Legacy configs must go too: `read_config` migrates them back on the
    // next run, which used to silently resurrect a logged-out session.
    let mut removed = Vec::new();
    for path in std::iter::once(&runtime.config_path).chain(&runtime.legacy_config_paths) {
        match fs::remove_file(path) {
            Ok(()) => removed.push(path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("removing {}", path.display()));
            }
        }
    }
    if removed.is_empty() {
        println!("Already logged out.");
    } else {
        println!("Logged out. Removed:");
        for path in removed {
            println!("  {}", path.display());
        }
    }
    Ok(())
}

pub(crate) async fn status(runtime: &Runtime) -> Result<()> {
    let Some(config) = runtime.read_config()? else {
        println!("Not logged in. Run `mesh login` to authenticate.");
        return Ok(());
    };
    let Some(auth) = config.auth else {
        println!("Not logged in. Run `mesh login` to authenticate.");
        return Ok(());
    };
    println!(
        "Logged in.{}",
        if token_expired(&auth) {
            " Token expired; it will refresh on the next network request."
        } else {
            ""
        }
    );
    if let Some(user) = config.user {
        let name = [user.first_name, user.last_name]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        let email = user.email.unwrap_or_else(|| "unknown email".to_string());
        if name.is_empty() {
            println!("User: {email}");
        } else {
            println!("User: {name} ({email})");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_from_http_request_line_extracts_get_query_code_and_state() -> Result<()> {
        let callback =
            callback_from_http_request_line("GET /?code=abc%20123&state=nonce HTTP/1.1")?;

        assert_eq!(
            callback,
            OauthCallback {
                code: "abc 123".to_string(),
                state: Some("nonce".to_string()),
            }
        );
        Ok(())
    }

    #[test]
    fn callback_from_http_request_line_rejects_non_get_requests() {
        let error = callback_from_http_request_line("POST /?code=abc HTTP/1.1")
            .unwrap_err()
            .to_string();

        assert!(error.contains("OAuth callback must be a GET request"));
    }

    #[test]
    fn code_from_callback_text_accepts_callback_urls_and_raw_codes() -> Result<()> {
        assert_eq!(
            code_from_callback_text("http://127.0.0.1:6374/?code=from-url")?,
            "from-url"
        );
        assert_eq!(code_from_callback_text("raw-code")?, "raw-code");
        Ok(())
    }

    #[test]
    fn callback_from_url_prefers_oauth_error_over_code() {
        let url = Url::parse("http://127.0.0.1:6374/?code=ok&error=denied").unwrap();
        let error = callback_from_url(&url).unwrap_err().to_string();

        assert_eq!(error, "OAuth error: denied");
    }

    #[test]
    fn login_flow_failures_classify_as_auth() {
        let url = Url::parse("http://127.0.0.1:6374/?error=access_denied").unwrap();
        let report = callback_from_url(&url).unwrap_err();
        assert_eq!(classify_report(&report), ErrorClass::Auth);

        let url = Url::parse("http://127.0.0.1:6374/?state=only").unwrap();
        let report = callback_from_url(&url).unwrap_err();
        assert_eq!(classify_report(&report), ErrorClass::Auth);

        let report = callback_from_http_request_line("POST / HTTP/1.1").unwrap_err();
        assert_eq!(classify_report(&report), ErrorClass::Auth);
    }

    #[test]
    fn callback_from_url_requires_code_but_not_state() -> Result<()> {
        let url = Url::parse("http://127.0.0.1:6374/").unwrap();
        let error = callback_from_url(&url).unwrap_err().to_string();
        assert!(error.contains("OAuth callback did not include code"));

        let url = Url::parse("http://127.0.0.1:6374/?code=ok").unwrap();
        assert_eq!(
            callback_from_url(&url)?,
            OauthCallback {
                code: "ok".to_string(),
                state: None,
            }
        );
        Ok(())
    }

    #[test]
    fn base64url_nopad_matches_rfc_4648_vectors() {
        assert_eq!(base64url_nopad(b""), "");
        assert_eq!(base64url_nopad(b"f"), "Zg");
        assert_eq!(base64url_nopad(b"fo"), "Zm8");
        assert_eq!(base64url_nopad(b"foo"), "Zm9v");
        assert_eq!(base64url_nopad(b"foob"), "Zm9vYg");
        assert_eq!(base64url_nopad(b"fooba"), "Zm9vYmE");
        assert_eq!(base64url_nopad(b"foobar"), "Zm9vYmFy");
        // Bytes that exercise the url-safe alphabet (62 -> '-', 63 -> '_').
        assert_eq!(base64url_nopad(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn pkce_challenge_matches_rfc_7636_appendix_b() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn random_bytes_returns_distinct_valid_verifier_material() -> Result<()> {
        let first = random_bytes::<32>()?;
        let second = random_bytes::<32>()?;

        assert_ne!(first, second, "OS randomness must not repeat");
        let verifier = hex::encode(first);
        assert_eq!(verifier.len(), 64);
        assert!(
            verifier.chars().all(|ch| ch.is_ascii_hexdigit()),
            "hex verifier stays inside the RFC 7636 unreserved charset"
        );
        Ok(())
    }

    #[tokio::test]
    async fn oauth_code_from_listener_hard_errors_on_state_mismatch() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await.into_diagnostic()?;
        let addr = listener.local_addr().into_diagnostic()?;
        let request = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            stream
                .write_all(b"GET /?code=stolen&state=wrong HTTP/1.1\r\n\r\n")
                .await
                .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).await.unwrap();
            response
        });

        let error = oauth_code_from_listener(listener, "expected")
            .await
            .unwrap_err()
            .to_string();
        let response = request.await.into_diagnostic()?;

        assert!(error.contains("state mismatch"), "got: {error}");
        assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
        Ok(())
    }

    #[tokio::test]
    async fn oauth_code_from_listener_accepts_matching_state() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await.into_diagnostic()?;
        let addr = listener.local_addr().into_diagnostic()?;
        let request = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            stream
                .write_all(b"GET /?code=good&state=expected HTTP/1.1\r\n\r\n")
                .await
                .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).await.unwrap();
            response
        });

        let code = oauth_code_from_listener(listener, "expected").await?;
        let response = request.await.into_diagnostic()?;

        assert_eq!(code, "good");
        assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
        Ok(())
    }

    #[test]
    fn logout_removes_active_and_legacy_configs() -> Result<()> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("meshx-auth-logout-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&dir).into_diagnostic()?;
        let config_path = dir.join("mesh.json");
        let runtime = Runtime {
            http: HttpClient::new(),
            config_path: config_path.clone(),
            legacy_config_paths: legacy_config_paths_for(&config_path),
            api_base: API_BASE.to_string(),
            mcp_base: MCP_BASE.to_string(),
            timeout: Duration::from_secs(5),
            retries: 0,
            refresh_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        };
        for path in std::iter::once(&runtime.config_path).chain(&runtime.legacy_config_paths) {
            fs::write(path, "{}").into_diagnostic()?;
        }

        logout(&runtime)?;

        assert!(!runtime.config_path.exists());
        for legacy in &runtime.legacy_config_paths {
            assert!(
                !legacy.exists(),
                "legacy config {} must not survive logout",
                legacy.display()
            );
        }
        // With every token file gone, a fresh read cannot resurrect a login.
        assert_eq!(runtime.read_config()?.map(|_| ()), None);
        fs::remove_dir_all(&dir).into_diagnostic()?;
        Ok(())
    }

    #[test]
    fn html_escape_escapes_markup_characters() {
        assert_eq!(
            html_escape("bad & <worse> \"quoted\""),
            "bad &amp; &lt;worse&gt; \"quoted\""
        );
    }
}
