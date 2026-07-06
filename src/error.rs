use crate::prelude::*;

#[derive(Debug, Error, miette::Diagnostic)]
pub(crate) enum MeshError {
    #[error("{0}")]
    Message(String),
    /// Typed marker for authentication failures (not logged in, token refresh
    /// failed). [`classify_report`] maps any report whose cause chain contains
    /// this variant to [`ErrorClass::Auth`] / exit code 3.
    #[error("{message}")]
    Auth {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },
}

impl MeshError {
    pub(crate) fn auth(message: impl Into<String>) -> Self {
        MeshError::Auth {
            message: message.into(),
            source: None,
        }
    }

    /// An auth failure caused by another report (e.g. the HTTP error behind a
    /// failed token refresh). The source report stays in the cause chain, so
    /// its own typed errors remain visible to [`classify_report`] and the
    /// JSON error envelope.
    pub(crate) fn auth_with_source(message: impl Into<String>, source: miette::Report) -> Self {
        MeshError::Auth {
            message: message.into(),
            source: Some(source.into()),
        }
    }
}

pub(crate) fn err<T>(message: impl Into<String>) -> Result<T> {
    Err(MeshError::Message(message.into()).into())
}

/// Like [`err`], but carrying the [`MeshError::Auth`] marker so the failure
/// exits with the auth code (3). For OAuth login-flow failures.
pub(crate) fn auth_err<T>(message: impl Into<String>) -> Result<T> {
    Err(MeshError::auth(message).into())
}

pub(crate) fn not_logged_in() -> MeshError {
    MeshError::auth("not logged in. Run `mesh login` first.")
}

/// Failure class for a finished run, mapped 1:1 to the process exit code.
///
/// The full taxonomy: 0 success, 1 generic failure, 2 clap usage error
/// (emitted by clap itself before dispatch), 3 auth, 4 network transport,
/// 5 any other non-2xx HTTP status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ErrorClass {
    /// Not logged in, token refresh failure, or HTTP 401/403 from the API.
    Auth,
    /// reqwest transport failure: connect, timeout, TLS, or body read.
    Network,
    /// Any other non-2xx HTTP status from the API.
    Http,
    /// Everything else, including partial bulk-write failures that already
    /// printed a machine-readable report.
    Other,
}

impl ErrorClass {
    pub(crate) fn exit_code(self) -> u8 {
        match self {
            ErrorClass::Other => 1,
            ErrorClass::Auth => 3,
            ErrorClass::Network => 4,
            ErrorClass::Http => 5,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ErrorClass::Auth => "auth",
            ErrorClass::Network => "network",
            ErrorClass::Http => "http",
            ErrorClass::Other => "other",
        }
    }
}

/// Classify a failed run by walking the report's cause chain for the typed
/// errors that survive `wrap_err` (wrapping adds a new chain head and keeps
/// the original error reachable as its source). An auth marker or a 401/403
/// status wins outright; otherwise the first `RequestError` decides.
pub(crate) fn classify_report(report: &miette::Report) -> ErrorClass {
    let mut class = ErrorClass::Other;
    for cause in report.chain() {
        if matches!(
            cause.downcast_ref::<MeshError>(),
            Some(MeshError::Auth { .. })
        ) {
            return ErrorClass::Auth;
        }
        if class != ErrorClass::Other {
            continue;
        }
        class = match cause.downcast_ref::<RequestError>() {
            Some(RequestError::Status { status, .. }) => {
                if *status == StatusCode::UNAUTHORIZED || *status == StatusCode::FORBIDDEN {
                    return ErrorClass::Auth;
                }
                ErrorClass::Http
            }
            Some(RequestError::Transport(_)) => ErrorClass::Network,
            None => ErrorClass::Other,
        };
    }
    class
}

/// Which error rendering `mesh` uses on stderr when a run fails.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ErrorFormat {
    Human,
    Json,
}

/// Resolve the error output format once, right after clap parsing. The global
/// `--error-format` flag is read defensively via `try_get_one` so this
/// defaults to human even if the flag is absent from the CLI definition; the
/// flag (which itself honors `MESH_ERROR_FORMAT` through clap) wins over the
/// raw environment fallback.
pub(crate) fn error_format_from_matches(matches: &ArgMatches) -> ErrorFormat {
    let value = matches
        .try_get_one::<String>("error-format")
        .ok()
        .flatten()
        .cloned()
        .or_else(|| std::env::var("MESH_ERROR_FORMAT").ok());
    match value.as_deref() {
        Some("json") => ErrorFormat::Json,
        _ => ErrorFormat::Human,
    }
}

/// The single-line JSON error envelope printed on stderr under
/// `--error-format json`. `message` is the top-level report message and
/// `chain` lists each underlying cause in order. `http` carries the typed
/// HTTP failure when one is in the chain; `retry_after_seconds` surfaces a
/// parsed `Retry-After` header. Rendering the returned [`Value`] with
/// `Display` yields compact JSON with no newlines.
pub(crate) fn error_envelope(report: &miette::Report) -> Value {
    let class = classify_report(report);
    let chain: Vec<Value> = report
        .chain()
        .skip(1)
        .map(|cause| Value::String(cause.to_string()))
        .collect();
    let mut http = Value::Null;
    let mut retry_after_seconds = Value::Null;
    for cause in report.chain() {
        if let Some(RequestError::Status {
            status,
            url,
            body,
            retry_after,
        }) = cause.downcast_ref::<RequestError>()
        {
            http = json!({
                "status": status.as_u16(),
                "url": url.as_str(),
                "body": body,
            });
            if let Some(retry_after) = retry_after {
                retry_after_seconds = json!(retry_after.as_secs());
            }
            break;
        }
    }
    json!({
        "error": {
            "message": report.to_string(),
            "chain": chain,
            "class": class.as_str(),
            "exit_code": class.exit_code(),
            "http": http,
            "retry_after_seconds": retry_after_seconds,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status_report(status: StatusCode, retry_after: Option<Duration>) -> miette::Report {
        RequestError::Status {
            status,
            url: Url::parse("https://mcp.example.test/tools/v2/get-contact").unwrap(),
            body: json!({"detail": "boom"}),
            retry_after,
        }
        .into()
    }

    #[test]
    fn classify_report_maps_http_statuses() {
        assert_eq!(
            classify_report(&status_report(StatusCode::INTERNAL_SERVER_ERROR, None)),
            ErrorClass::Http
        );
        assert_eq!(
            classify_report(&status_report(StatusCode::NOT_FOUND, None)),
            ErrorClass::Http
        );
        assert_eq!(
            classify_report(&status_report(StatusCode::UNAUTHORIZED, None)),
            ErrorClass::Auth
        );
        assert_eq!(
            classify_report(&status_report(StatusCode::FORBIDDEN, None)),
            ErrorClass::Auth
        );
        assert_eq!(ErrorClass::Http.exit_code(), 5);
        assert_eq!(ErrorClass::Auth.exit_code(), 3);
    }

    #[test]
    fn classify_report_finds_typed_errors_behind_wrap_err() {
        let report = Err::<(), miette::Report>(status_report(StatusCode::BAD_GATEWAY, None))
            .wrap_err("calling get-contact")
            .unwrap_err();

        assert_eq!(report.to_string(), "calling get-contact");
        assert_eq!(classify_report(&report), ErrorClass::Http);
    }

    #[test]
    fn classify_report_maps_auth_markers_and_plain_messages() {
        let auth: miette::Report = MeshError::auth("not logged in").into();
        assert_eq!(classify_report(&auth), ErrorClass::Auth);

        let not_logged_in: miette::Report = not_logged_in().into();
        assert_eq!(classify_report(&not_logged_in), ErrorClass::Auth);

        let plain = err::<()>("plain failure").unwrap_err();
        assert_eq!(classify_report(&plain), ErrorClass::Other);
        assert_eq!(ErrorClass::Other.exit_code(), 1);
    }

    #[test]
    fn classify_report_prefers_the_auth_marker_over_its_http_source() {
        // A failed token refresh wraps the underlying HTTP report: the run
        // must classify as auth (3), not as the wrapped 500 (5).
        let inner = status_report(StatusCode::INTERNAL_SERVER_ERROR, None);
        let report: miette::Report =
            MeshError::auth_with_source("token refresh failed; run `mesh login` again", inner)
                .into();

        assert_eq!(classify_report(&report), ErrorClass::Auth);
    }

    #[tokio::test]
    async fn classify_report_maps_transport_errors_to_network() {
        // Port 1 on loopback is never listening; the connect is refused
        // locally, giving a genuine reqwest transport error without a server.
        let error = HttpClient::new()
            .get("http://127.0.0.1:1/")
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .expect_err("connect to loopback port 1 must fail");
        let report = Err::<(), miette::Report>(RequestError::Transport(error).into())
            .wrap_err("requesting me.sh")
            .unwrap_err();

        assert_eq!(classify_report(&report), ErrorClass::Network);
        assert_eq!(ErrorClass::Network.exit_code(), 4);
    }

    #[test]
    fn error_envelope_reports_http_failures_with_chain_in_order() {
        let report = Err::<(), miette::Report>(status_report(
            StatusCode::INTERNAL_SERVER_ERROR,
            Some(Duration::from_secs(7)),
        ))
        .wrap_err("inner context")
        .wrap_err("outer context")
        .unwrap_err();

        let envelope = error_envelope(&report);
        let error = &envelope["error"];

        assert_eq!(error["message"], json!("outer context"));
        let chain = error["chain"].as_array().expect("chain array");
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0], json!("inner context"));
        assert!(
            chain[1]
                .as_str()
                .unwrap()
                .starts_with("HTTP 500 Internal Server Error from"),
            "got: {}",
            chain[1]
        );
        assert_eq!(error["class"], json!("http"));
        assert_eq!(error["exit_code"], json!(5));
        assert_eq!(error["http"]["status"], json!(500));
        assert_eq!(
            error["http"]["url"],
            json!("https://mcp.example.test/tools/v2/get-contact")
        );
        assert_eq!(error["http"]["body"], json!({"detail": "boom"}));
        assert_eq!(error["retry_after_seconds"], json!(7));
    }

    #[test]
    fn error_envelope_reports_auth_failures_without_http_details() {
        let report: miette::Report = not_logged_in().into();

        let envelope = error_envelope(&report);
        let error = &envelope["error"];

        assert_eq!(
            error["message"],
            json!("not logged in. Run `mesh login` first.")
        );
        assert_eq!(error["chain"], json!([]));
        assert_eq!(error["class"], json!("auth"));
        assert_eq!(error["exit_code"], json!(3));
        assert_eq!(error["http"], Value::Null);
        assert_eq!(error["retry_after_seconds"], Value::Null);
    }

    #[test]
    fn error_envelope_display_is_a_single_line() {
        let report = status_report(StatusCode::INTERNAL_SERVER_ERROR, None);
        let line = error_envelope(&report).to_string();

        assert!(!line.contains('\n'), "envelope must be one line: {line}");
        assert!(line.starts_with("{\"error\":{"), "got: {line}");
    }

    #[test]
    fn error_format_reads_the_global_flag_and_defaults_to_human() {
        let matches = build_cli().get_matches_from(["mesh", "--error-format", "json", "whoami"]);
        assert_eq!(error_format_from_matches(&matches), ErrorFormat::Json);

        // A matches object without the flag defined at all: the defensive
        // `try_get_one` path. Skip when the environment forces a format.
        if std::env::var_os("MESH_ERROR_FORMAT").is_some() {
            return;
        }
        let matches = clap::Command::new("test").get_matches_from(["test"]);
        assert_eq!(error_format_from_matches(&matches), ErrorFormat::Human);

        let matches = build_cli().get_matches_from(["mesh", "whoami"]);
        assert_eq!(error_format_from_matches(&matches), ErrorFormat::Human);
    }
}
