use crate::prelude::*;

pub(crate) async fn login(runtime: &Runtime, open_browser: bool) -> Result<()> {
    let listener = TcpListener::bind(CALLBACK_ADDR)
        .await
        .into_diagnostic()
        .wrap_err_with(|| format!("binding OAuth callback listener on {CALLBACK_ADDR}"))?;
    let mut auth_url = Url::parse(AUTH_URL)
        .into_diagnostic()
        .wrap_err("parsing OAuth URL")?;
    auth_url
        .query_pairs_mut()
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("response_type", "code")
        .append_pair("scope", "read write");

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
        code = oauth_code_from_listener(listener) => code?,
        code = oauth_code_from_stdin() => code?,
    };
    let auth = runtime.exchange_code(&code).await?;
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

pub(crate) async fn oauth_code_from_listener(listener: TcpListener) -> Result<String> {
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
        match code_from_http_request_line(first_line) {
            Ok(code) => {
                let body = "<html><body><h1>Logged in to me.sh</h1><p>You can close this tab.</p></body></html>";
                write_http_response(&mut stream, 200, body).await?;
                return Ok(code);
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
    reader
        .read_line(&mut line)
        .await
        .into_diagnostic()
        .wrap_err("reading OAuth callback from stdin")?;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return err("stdin closed before a callback URL or code was pasted");
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

pub(crate) fn code_from_http_request_line(line: &str) -> Result<String> {
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    if method != "GET" {
        return err("OAuth callback must be a GET request");
    }
    let url = Url::parse(&format!("{REDIRECT_URI}{path}"))
        .into_diagnostic()
        .wrap_err("parsing OAuth callback")?;
    code_from_url(&url)
}

pub(crate) fn code_from_callback_text(text: &str) -> Result<String> {
    if let Ok(url) = Url::parse(text) {
        return code_from_url(&url);
    }
    Ok(text.to_string())
}

pub(crate) fn code_from_url(url: &Url) -> Result<String> {
    let mut error = None;
    let mut code = None;
    for (key, value) in url.query_pairs() {
        if key == "error" {
            error = Some(value.to_string());
        }
        if key == "code" {
            code = Some(value.to_string());
        }
    }
    if let Some(error) = error {
        return err(format!("OAuth error: {error}"));
    }
    code.ok_or_else(|| miette!("OAuth callback did not include code"))
}

pub(crate) fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(crate) fn logout(runtime: &Runtime) -> Result<()> {
    match fs::remove_file(&runtime.config_path) {
        Ok(()) => println!("Logged out. Token removed."),
        Err(error) if error.kind() == io::ErrorKind::NotFound => println!("Already logged out."),
        Err(error) => return Err(error).into_diagnostic().wrap_err("removing config"),
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
    fn code_from_http_request_line_extracts_get_query_code() -> Result<()> {
        let code = code_from_http_request_line("GET /?code=abc%20123 HTTP/1.1")?;

        assert_eq!(code, "abc 123");
        Ok(())
    }

    #[test]
    fn code_from_http_request_line_rejects_non_get_requests() {
        let error = code_from_http_request_line("POST /?code=abc HTTP/1.1")
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
    fn code_from_url_prefers_oauth_error_over_code() {
        let url = Url::parse("http://127.0.0.1:6374/?code=ok&error=denied").unwrap();
        let error = code_from_url(&url).unwrap_err().to_string();

        assert_eq!(error, "OAuth error: denied");
    }

    #[test]
    fn code_from_url_requires_code() {
        let url = Url::parse("http://127.0.0.1:6374/").unwrap();
        let error = code_from_url(&url).unwrap_err().to_string();

        assert!(error.contains("OAuth callback did not include code"));
    }

    #[test]
    fn html_escape_escapes_markup_characters() {
        assert_eq!(
            html_escape("bad & <worse> \"quoted\""),
            "bad &amp; &lt;worse&gt; \"quoted\""
        );
    }
}
