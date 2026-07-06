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
        retry_after: Option<Duration>,
    },
}

/// How aggressively a failed request may be retried.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RetryPolicy {
    /// Reads and other idempotent requests: retrying can at worst repeat work.
    Idempotent,
    /// Writes: a timeout or 5xx may have committed server-side, so retry only
    /// when the request provably never reached the server.
    NonIdempotent,
}

pub(crate) fn should_retry(
    error: &RequestError,
    attempt: u32,
    retries: u32,
    policy: RetryPolicy,
) -> bool {
    if attempt >= retries {
        return false;
    }
    match error {
        // `is_connect()` means the connection was never established, so the
        // request was provably never sent and even writes are safe to retry.
        RequestError::Transport(error) => match policy {
            RetryPolicy::Idempotent => error.is_timeout() || error.is_connect(),
            RetryPolicy::NonIdempotent => error.is_connect(),
        },
        // 429 is rejected before processing, so it is safe for both classes.
        RequestError::Status { status, .. } => {
            *status == StatusCode::TOO_MANY_REQUESTS
                || (policy == RetryPolicy::Idempotent && status.is_server_error())
        }
    }
}

pub(crate) const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);

/// Backoff before the next attempt: a 429 with a parsable `Retry-After:
/// <seconds>` header is honored (capped at [`MAX_RETRY_AFTER`]); everything
/// else uses the existing exponential backoff.
pub(crate) fn retry_delay(error: &RequestError, attempt: u32) -> Duration {
    if let RequestError::Status {
        status,
        retry_after: Some(retry_after),
        ..
    } = error
        && *status == StatusCode::TOO_MANY_REQUESTS
    {
        return (*retry_after).min(MAX_RETRY_AFTER);
    }
    Duration::from_millis(250 * 2_u64.pow(attempt))
}

/// Parse the seconds form of `Retry-After`. The HTTP-date form is rare on
/// rate limiters and is deliberately ignored (we fall back to backoff).
pub(crate) fn retry_after_from_headers(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let value = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    let seconds: u64 = value.trim().parse().ok()?;
    Some(Duration::from_secs(seconds))
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
        status_error_with_retry_after(status, None)
    }

    fn status_error_with_retry_after(
        status: StatusCode,
        retry_after: Option<Duration>,
    ) -> RequestError {
        RequestError::Status {
            status,
            url: Url::parse("https://example.test/path").unwrap(),
            body: json!({"error": "no"}),
            retry_after,
        }
    }

    #[test]
    fn should_retry_respects_retry_budget() {
        let error = status_error(StatusCode::INTERNAL_SERVER_ERROR);

        assert!(should_retry(&error, 1, 2, RetryPolicy::Idempotent));
        assert!(!should_retry(&error, 2, 2, RetryPolicy::Idempotent));
    }

    #[test]
    fn should_retry_retries_server_errors_and_rate_limits_for_reads() {
        assert!(should_retry(
            &status_error(StatusCode::INTERNAL_SERVER_ERROR),
            0,
            1,
            RetryPolicy::Idempotent
        ));
        assert!(should_retry(
            &status_error(StatusCode::TOO_MANY_REQUESTS),
            0,
            1,
            RetryPolicy::Idempotent
        ));
    }

    #[test]
    fn should_retry_never_retries_server_errors_for_writes() {
        // A 5xx (or timeout) after a write may have committed server-side;
        // blind retries would duplicate the write.
        assert!(!should_retry(
            &status_error(StatusCode::INTERNAL_SERVER_ERROR),
            0,
            1,
            RetryPolicy::NonIdempotent
        ));
        assert!(!should_retry(
            &status_error(StatusCode::BAD_GATEWAY),
            0,
            1,
            RetryPolicy::NonIdempotent
        ));
    }

    #[test]
    fn should_retry_retries_rate_limits_for_writes() {
        // 429 means the server refused before processing, so retrying a write
        // cannot duplicate it.
        assert!(should_retry(
            &status_error(StatusCode::TOO_MANY_REQUESTS),
            0,
            1,
            RetryPolicy::NonIdempotent
        ));
    }

    #[test]
    fn should_retry_ignores_success_and_non_rate_limited_client_errors() {
        for policy in [RetryPolicy::Idempotent, RetryPolicy::NonIdempotent] {
            assert!(!should_retry(&status_error(StatusCode::OK), 0, 1, policy));
            assert!(!should_retry(
                &status_error(StatusCode::BAD_REQUEST),
                0,
                1,
                policy
            ));
        }
    }

    #[test]
    fn retry_delay_honors_capped_retry_after_only_for_rate_limits() {
        let rate_limited = status_error_with_retry_after(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(3)),
        );
        let rate_limited_slow = status_error_with_retry_after(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(600)),
        );
        let server_error = status_error_with_retry_after(
            StatusCode::INTERNAL_SERVER_ERROR,
            Some(Duration::from_secs(3)),
        );

        assert_eq!(retry_delay(&rate_limited, 0), Duration::from_secs(3));
        assert_eq!(retry_delay(&rate_limited_slow, 0), MAX_RETRY_AFTER);
        assert_eq!(retry_delay(&server_error, 0), Duration::from_millis(250));
        assert_eq!(
            retry_delay(&status_error(StatusCode::TOO_MANY_REQUESTS), 2),
            Duration::from_millis(1000)
        );
    }

    #[test]
    fn retry_after_from_headers_parses_only_the_seconds_form() {
        let mut headers = reqwest::header::HeaderMap::new();
        assert_eq!(retry_after_from_headers(&headers), None);

        headers.insert(reqwest::header::RETRY_AFTER, "7".parse().unwrap());
        assert_eq!(
            retry_after_from_headers(&headers),
            Some(Duration::from_secs(7))
        );

        headers.insert(
            reqwest::header::RETRY_AFTER,
            "Wed, 21 Oct 2026 07:28:00 GMT".parse().unwrap(),
        );
        assert_eq!(retry_after_from_headers(&headers), None);

        headers.insert(reqwest::header::RETRY_AFTER, "-1".parse().unwrap());
        assert_eq!(retry_after_from_headers(&headers), None);
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
