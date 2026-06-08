use crate::prelude::*;

#[derive(Clone, Debug)]
pub(crate) struct Runtime {
    pub(crate) http: HttpClient,
    pub(crate) config_path: PathBuf,
    pub(crate) legacy_config_paths: Vec<PathBuf>,
    pub(crate) api_base: String,
    pub(crate) mcp_base: String,
    pub(crate) timeout: Duration,
    pub(crate) retries: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct ToolRequest {
    pub(crate) route: String,
    pub(crate) tool_name: String,
    pub(crate) payload: Value,
}

pub(crate) fn tool_route_url_path(route: &str) -> Result<String> {
    let route = route.trim();
    if route.is_empty() {
        return err("tool route must not be empty");
    }
    if route == "/" || route == "/tools/v2" {
        err("tool route must include a route path")
    } else if route.starts_with("/tools/v2/") {
        Ok(route.to_string())
    } else if route.starts_with("/tools/") {
        err(format!("tool route must be under /tools/v2: {route}"))
    } else if route.starts_with('/') {
        Ok(format!("/tools/v2{route}"))
    } else {
        Ok(format!("/tools/v2/{route}"))
    }
}

pub(crate) async fn run_tool_command(
    runtime: &Runtime,
    root: &ArgMatches,
    sub: &ArgMatches,
    spec: &CommandSpec,
) -> Result<()> {
    let payload = parse_payload(spec, sub)?;
    if spec.name == "contacts:merge" {
        let ids = payload
            .get("contact_ids")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let numeric = ids
            .into_iter()
            .map(|value| {
                value
                    .as_u64()
                    .ok_or_else(|| miette!("contacts:merge requires integer contact IDs"))
            })
            .collect::<Result<Vec<_>>>()?;
        validate_merge_ids(&numeric)?;
    }

    if spec.destructive && !sub.get_flag("dry-run") && !sub.get_flag("yes") {
        return err(format!(
            "{} writes me.sh data. Re-run with --yes, or use --dry-run.",
            spec.name
        ));
    }

    let request = ToolRequest {
        route: format!("/tools/v2{}", spec.route_path),
        tool_name: spec.tool_name.to_string(),
        payload: Value::Object(payload),
    };

    if sub.get_flag("dry-run") {
        write_value(
            root,
            json!({
                "route": request.route,
                "tool_name": request.tool_name,
                "payload": request.payload,
            }),
        )
    } else {
        let data = runtime.call_tool(spec.route_path, request.payload).await?;
        write_value(root, data)
    }
}

impl Runtime {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let timeout = Duration::from_secs(*matches.get_one::<u64>("timeout").unwrap_or(&30));
        let retries = *matches.get_one::<u32>("retries").unwrap_or(&2);
        let api_base = matches
            .get_one::<String>("api-base")
            .cloned()
            .unwrap_or_else(|| API_BASE.to_string());
        let mcp_base = matches
            .get_one::<String>("mcp-base")
            .cloned()
            .unwrap_or_else(|| MCP_BASE.to_string());
        let config_path = matches
            .get_one::<String>("config")
            .map(PathBuf::from)
            .unwrap_or_else(default_config_path);
        let legacy_config_paths = legacy_config_paths_for(&config_path);
        let http = HttpClient::builder()
            .user_agent(USER_AGENT)
            .timeout(timeout)
            .build()
            .into_diagnostic()
            .wrap_err("building HTTP client")?;
        Ok(Self {
            http,
            config_path,
            legacy_config_paths,
            api_base,
            mcp_base,
            timeout,
            retries,
        })
    }

    pub(crate) fn read_config(&self) -> Result<Option<MeshConfig>> {
        if let Some(config) = read_config_file(&self.config_path)? {
            return Ok(Some(config));
        }
        for path in &self.legacy_config_paths {
            if let Some(config) = read_config_file(path)? {
                self.write_config(&config)?;
                return Ok(Some(config));
            }
        }
        Ok(None)
    }

    pub(crate) fn write_config(&self, config: &MeshConfig) -> Result<()> {
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)
                .into_diagnostic()
                .wrap_err_with(|| format!("creating {}", parent.display()))?;
        }
        let content = serde_json::to_string_pretty(config)
            .into_diagnostic()
            .wrap_err("serializing me.sh config")?;
        fs::write(&self.config_path, content)
            .into_diagnostic()
            .wrap_err_with(|| format!("writing {}", self.config_path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.config_path, fs::Permissions::from_mode(0o600))
                .into_diagnostic()
                .wrap_err_with(|| {
                    format!("setting permissions on {}", self.config_path.display())
                })?;
        }
        Ok(())
    }

    pub(crate) async fn access_token(&self) -> Result<String> {
        if let Ok(token) = std::env::var("MESH_ACCESS_TOKEN")
            && !token.trim().is_empty()
        {
            return Ok(token);
        }
        let mut config = self
            .read_config()?
            .ok_or_else(|| miette!("not logged in. Run `mesh login` first."))?;
        let auth = config
            .auth
            .clone()
            .ok_or_else(|| miette!("not logged in. Run `mesh login` first."))?;
        if !token_expired(&auth) {
            return Ok(auth.access_token);
        }
        let refreshed = self.refresh_token(&auth.refresh_token).await?;
        config.auth = Some(refreshed.clone());
        self.write_config(&config)?;
        Ok(refreshed.access_token)
    }

    pub(crate) async fn refresh_token(&self, refresh_token: &str) -> Result<AuthTokens> {
        let data = self
            .api_request(
                Method::POST,
                "/api/v2/o/token/",
                None,
                Some(&[
                    ("grant_type", "refresh_token"),
                    ("client_id", CLIENT_ID),
                    ("refresh_token", refresh_token),
                ]),
            )
            .await?;
        tokens_from_value(data)
    }

    pub(crate) async fn exchange_code(&self, code: &str) -> Result<AuthTokens> {
        let data = self
            .api_request(
                Method::POST,
                "/api/v2/o/token/",
                None,
                Some(&[
                    ("grant_type", "authorization_code"),
                    ("client_id", CLIENT_ID),
                    ("code", code),
                    ("redirect_uri", REDIRECT_URI),
                ]),
            )
            .await?;
        tokens_from_value(data)
    }

    pub(crate) async fn current_user(&self) -> Result<MeshUser> {
        let token = self.access_token().await?;
        let data = self
            .api_request(Method::GET, "/api/v1/users/self/", Some(token), None)
            .await?;
        serde_json::from_value(data)
            .into_diagnostic()
            .wrap_err("decoding current user")
    }

    pub(crate) async fn call_tool(&self, route: &str, payload: Value) -> Result<Value> {
        let token = self.access_token().await?;
        let route = tool_route_url_path(route)?;
        let url = join_url(&self.mcp_base, &route)?;
        let body = if payload.is_object() {
            payload
        } else {
            json!({})
        };
        self.request_json_with_retry(Method::POST, url, Some(token), Some(body), None)
            .await
    }

    pub(crate) async fn search_total(&self, mut payload: Map<String, Value>) -> Result<usize> {
        payload.set("limit", 0);
        let data = self
            .call_tool(route::SEARCH, Value::Object(payload))
            .await?;
        total_from_search_usize(&data)
    }

    pub(crate) async fn api_request(
        &self,
        method: Method,
        path: &str,
        token: Option<String>,
        form: Option<&[(&str, &str)]>,
    ) -> Result<Value> {
        let url = join_url(&self.api_base, path)?;
        self.request_json_with_retry(method, url, token, None, form)
            .await
    }

    pub(crate) async fn request_json_with_retry(
        &self,
        method: Method,
        url: Url,
        token: Option<String>,
        json_body: Option<Value>,
        form: Option<&[(&str, &str)]>,
    ) -> Result<Value> {
        let mut attempt = 0;
        loop {
            let result = self
                .request_json_once(
                    method.clone(),
                    url.clone(),
                    token.clone(),
                    json_body.clone(),
                    form,
                )
                .await;
            match result {
                Ok(value) => return Ok(value),
                Err(error) if should_retry(&error, attempt, self.retries) => {
                    let delay = Duration::from_millis(250 * 2_u64.pow(attempt));
                    warn!(%error, attempt, ?delay, "transient me.sh request failed; retrying");
                    sleep(delay).await;
                    attempt += 1;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    pub(crate) async fn request_json_once(
        &self,
        method: Method,
        url: Url,
        token: Option<String>,
        json_body: Option<Value>,
        form: Option<&[(&str, &str)]>,
    ) -> std::result::Result<Value, RequestError> {
        debug!(%method, %url, "me.sh HTTP request");
        let mut request = self.http.request(method, url.clone());
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = json_body {
            request = request.json(&body);
        }
        if let Some(form) = form {
            request = request.form(form);
        }
        let response = request.send().await.map_err(RequestError::Transport)?;
        let status = response.status();
        let text = response.text().await.map_err(RequestError::Transport)?;
        let parsed = parse_maybe_json(&text);
        if !status.is_success() {
            return Err(RequestError::Status {
                status,
                url,
                body: parsed,
            });
        }
        Ok(parsed)
    }

    pub(crate) async fn doctor(&self) -> Result<Value> {
        let config = self.read_config()?;
        let mut report = Map::new();
        report.set("version", VERSION.to_string());
        report.insert(
            "config_path".to_string(),
            Value::String(self.config_path.display().to_string()),
        );
        report.insert(
            "legacy_config_paths".to_string(),
            Value::Array(
                self.legacy_config_paths
                    .iter()
                    .map(|path| Value::String(path.display().to_string()))
                    .collect(),
            ),
        );
        report.set("has_config", config.is_some());
        report.insert(
            "has_auth".to_string(),
            Value::Bool(config.as_ref().and_then(|c| c.auth.as_ref()).is_some()),
        );
        report.insert(
            "has_user".to_string(),
            Value::Bool(config.as_ref().and_then(|c| c.user.as_ref()).is_some()),
        );
        report.set("api_base", self.api_base.clone());
        report.set("mcp_base", self.mcp_base.clone());
        report.insert(
            "timeout_seconds".to_string(),
            Value::Number(Number::from(self.timeout.as_secs())),
        );
        report.insert(
            "retries".to_string(),
            Value::Number(Number::from(self.retries)),
        );
        if let Some(auth) = config.as_ref().and_then(|config| config.auth.as_ref()) {
            report.insert(
                "token_expired".to_string(),
                Value::Bool(token_expired(auth)),
            );
            report.insert(
                "token_expires_at".to_string(),
                Value::Number(Number::from(auth.expires_at)),
            );
        } else {
            report.set("token_expired", Value::Null);
        }

        let mcp_url = join_url(&self.mcp_base, "/")?;
        match self.http.get(mcp_url).send().await {
            Ok(response) => {
                report.insert(
                    "mcp_status".to_string(),
                    Value::Number(Number::from(response.status().as_u16())),
                );
                report.set("mcp_reachable", true);
            }
            Err(error) => {
                report.set("mcp_reachable", false);
                report.set("mcp_error", error.to_string());
            }
        }
        Ok(Value::Object(report))
    }
}

/// Call a tool for each action with at most `concurrency` requests in flight per
/// chunk, preserving input order. `call_of` maps an action to its `(route,
/// payload)`. Each action is returned paired with its tool-call result, leaving
/// the success/failure policy (fail-fast, best-effort, accumulate) to the
/// caller. `join_context` labels the rare case where a spawned task fails to
/// join. This is the shared fan-out behind every bulk write command.
pub(crate) async fn run_bulk_tool_calls<A>(
    runtime: &Runtime,
    actions: Vec<A>,
    concurrency: usize,
    join_context: &str,
    call_of: impl Fn(&A) -> (&'static str, Value),
) -> Result<Vec<(A, Result<Value>)>> {
    if concurrency == 0 {
        return err("bulk tool-call concurrency must be greater than zero");
    }
    let mut outcomes = Vec::with_capacity(actions.len());
    let mut remaining = actions.into_iter();
    loop {
        let chunk: Vec<A> = remaining.by_ref().take(concurrency).collect();
        if chunk.is_empty() {
            break;
        }
        let mut handles = Vec::with_capacity(chunk.len());
        for action in &chunk {
            let (route, payload) = call_of(action);
            let runtime = runtime.clone();
            handles.push(tokio::spawn(async move {
                runtime.call_tool(route, payload).await
            }));
        }
        for (action, handle) in chunk.into_iter().zip(handles) {
            let result = handle
                .await
                .into_diagnostic()
                .wrap_err_with(|| join_context.to_string())?;
            outcomes.push((action, result));
        }
    }
    Ok(outcomes)
}

pub(crate) fn total_from_search_usize(data: &Value) -> Result<usize> {
    let Some(total) = data
        .get("total")
        .or_else(|| data.get("count"))
        .and_then(Value::as_u64)
    else {
        return err("me.sh search response did not include numeric total");
    };
    usize::try_from(total).into_diagnostic()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_route_url_path_accepts_bare_and_prefixed_v2_routes() -> Result<()> {
        assert_eq!(tool_route_url_path("search")?, "/tools/v2/search");
        assert_eq!(tool_route_url_path("/search")?, "/tools/v2/search");
        assert_eq!(tool_route_url_path("/tools/v2/search")?, "/tools/v2/search");
        Ok(())
    }

    #[test]
    fn tool_route_url_path_rejects_non_v2_tool_routes() {
        assert!(tool_route_url_path("/tools/v20/search").is_err());
        assert!(tool_route_url_path("/tools/v1/search").is_err());
        assert!(tool_route_url_path("/tools/v2").is_err());
        assert!(tool_route_url_path("/").is_err());
        assert!(tool_route_url_path("").is_err());
    }
}
