use crate::prelude::*;

#[derive(Debug, Error, miette::Diagnostic)]
pub(crate) enum RequestError {
    #[error("transport error: {0}")]
    Transport(reqwest::Error),
    #[error("HTTP {status} from {url}: {body}")]
    Status {
        status: StatusCode,
        url: Url,
        body: Value,
    },
}

pub(crate) fn should_retry(error: &RequestError, attempt: u32, retries: u32) -> bool {
    if attempt >= retries {
        return false;
    }
    match error {
        RequestError::Transport(error) => error.is_timeout() || error.is_connect(),
        RequestError::Status { status, .. } => {
            status.is_server_error() || *status == StatusCode::TOO_MANY_REQUESTS
        }
    }
}

pub(crate) fn parse_maybe_json(text: &str) -> Value {
    if text.trim().is_empty() {
        return Value::Null;
    }
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()))
}

pub(crate) fn join_url(base: &str, path: &str) -> Result<Url> {
    let base = Url::parse(base)
        .into_diagnostic()
        .wrap_err_with(|| format!("invalid base URL {base}"))?;
    base.join(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("joining URL path {path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status_error(status: StatusCode) -> RequestError {
        RequestError::Status {
            status,
            url: Url::parse("https://example.test/path").unwrap(),
            body: json!({"error": "no"}),
        }
    }

    #[test]
    fn should_retry_respects_retry_budget() {
        let error = status_error(StatusCode::INTERNAL_SERVER_ERROR);

        assert!(should_retry(&error, 1, 2));
        assert!(!should_retry(&error, 2, 2));
    }

    #[test]
    fn should_retry_retries_server_errors_and_rate_limits() {
        assert!(should_retry(
            &status_error(StatusCode::INTERNAL_SERVER_ERROR),
            0,
            1
        ));
        assert!(should_retry(
            &status_error(StatusCode::TOO_MANY_REQUESTS),
            0,
            1
        ));
    }

    #[test]
    fn should_retry_ignores_success_and_non_rate_limited_client_errors() {
        assert!(!should_retry(&status_error(StatusCode::OK), 0, 1));
        assert!(!should_retry(&status_error(StatusCode::BAD_REQUEST), 0, 1));
    }

    #[test]
    fn parse_maybe_json_handles_empty_json_and_plain_text() {
        assert_eq!(parse_maybe_json("   "), Value::Null);
        assert_eq!(parse_maybe_json("{\"ok\":true}"), json!({"ok": true}));
        assert_eq!(parse_maybe_json("not json"), json!("not json"));
        assert_eq!(parse_maybe_json("  not json  "), json!("  not json  "));
    }

    #[test]
    fn join_url_uses_url_joining_rules() -> Result<()> {
        let relative = join_url("https://api.example.test/root/", "items")?;
        let absolute = join_url("https://api.example.test/root/", "/items")?;

        assert_eq!(relative.as_str(), "https://api.example.test/root/items");
        assert_eq!(absolute.as_str(), "https://api.example.test/items");
        Ok(())
    }

    #[test]
    fn join_url_reports_invalid_base_url() {
        let error = join_url("not a url", "items").unwrap_err().to_string();

        assert!(error.contains("invalid base URL not a url"));
    }
}
