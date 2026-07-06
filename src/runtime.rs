use crate::prelude::*;

use std::sync::{Arc, OnceLock};

#[derive(Clone, Debug)]
pub(crate) struct Runtime {
    pub(crate) http: HttpClient,
    pub(crate) config_path: PathBuf,
    pub(crate) legacy_config_paths: Vec<PathBuf>,
    pub(crate) api_base: String,
    pub(crate) mcp_base: String,
    pub(crate) timeout: Duration,
    pub(crate) retries: u32,
    /// Serializes token refreshes across clones: bulk fan-outs clone the
    /// `Runtime` into up to 16 tasks, and without this every task would fire
    /// its own refresh POST and rewrite the config (fatal when the server
    /// rotates refresh tokens). Shared via `Arc` so clones contend on one lock.
    pub(crate) refresh_lock: Arc<tokio::sync::Mutex<()>>,
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
            refresh_lock: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    pub(crate) fn read_config(&self) -> Result<Option<MeshConfig>> {
        if let Some(config) = read_config_file(&self.config_path)? {
            return Ok(Some(config));
        }
        for path in &self.legacy_config_paths {
            if let Some(config) = read_config_file(path)? {
                self.write_config(&config)?;
                // The migrated config now owns the tokens; remove the legacy
                // plaintext copy so it cannot linger or resurrect a logout.
                if let Err(error) = fs::remove_file(path) {
                    warn!(%error, path = %path.display(), "could not remove migrated legacy config");
                }
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
        write_secret_file(&self.config_path, &content)
    }

    pub(crate) async fn access_token(&self) -> Result<String> {
        if let Ok(token) = std::env::var("MESH_ACCESS_TOKEN")
            && !token.trim().is_empty()
        {
            return Ok(token);
        }
        let auth = self
            .read_config()?
            .and_then(|config| config.auth)
            .ok_or_else(|| miette!("not logged in. Run `mesh login` first."))?;
        if !token_expired(&auth) {
            return Ok(auth.access_token);
        }
        // Only one task may refresh at a time. Whoever loses the race re-reads
        // the config inside the lock and finds the token the winner just wrote.
        let _guard = self.refresh_lock.lock().await;
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

    pub(crate) async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<AuthTokens> {
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
                    ("code_verifier", code_verifier),
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
        let policy = if tool_route_is_destructive(&route) {
            RetryPolicy::NonIdempotent
        } else {
            RetryPolicy::Idempotent
        };
        let url = join_url(&self.mcp_base, &route)?;
        let body = if payload.is_object() {
            payload
        } else {
            json!({})
        };
        self.request_json_with_retry(Method::POST, url, Some(token), Some(body), None, policy)
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
        self.request_json_with_retry(method, url, token, None, form, RetryPolicy::Idempotent)
            .await
    }

    pub(crate) async fn request_json_with_retry(
        &self,
        method: Method,
        url: Url,
        token: Option<String>,
        json_body: Option<Value>,
        form: Option<&[(&str, &str)]>,
        policy: RetryPolicy,
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
                Err(error) if should_retry(&error, attempt, self.retries, policy) => {
                    let delay = retry_delay(&error, attempt);
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
        let retry_after = retry_after_from_headers(response.headers());
        let text = response.text().await.map_err(RequestError::Transport)?;
        let parsed = parse_maybe_json(&text);
        if !status.is_success() {
            return Err(RequestError::Status {
                status,
                url,
                body: parsed,
                retry_after,
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

/// Whether a tool route writes me.sh data. Backed by the same
/// `command_spec` `destructive` catalog that drives plan_audit's write/read
/// classification, so there is a single source of truth. Accepts bare
/// (`search`), slash (`/search`), and full (`/tools/v2/search`) route forms.
/// Routes not in the catalog (and malformed routes) are treated as writes so
/// an unknown route is never blind-retried after it may have committed.
pub(crate) fn tool_route_is_destructive(route: &str) -> bool {
    static CATALOG: OnceLock<BTreeMap<String, bool>> = OnceLock::new();
    let Ok(full) = tool_route_url_path(route) else {
        return true;
    };
    let Some(route_path) = full.strip_prefix("/tools/v2") else {
        return true;
    };
    CATALOG
        .get_or_init(plan_audit_route_catalog)
        .get(route_path)
        .copied()
        .unwrap_or(true)
}

/// Write `content` to `path` so no other user can ever read the tokens: on
/// unix the bytes go into a same-directory temp file created with mode 0o600,
/// which is then renamed over the target. This closes the window where the
/// old `fs::write` + chmod sequence left the file world-readable, and makes
/// the update atomic for concurrent readers. Non-unix keeps plain `fs::write`.
pub(crate) fn write_secret_file(path: &Path, content: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("mesh-config");
        let temp_path = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
        let result = (|| -> io::Result<()> {
            // A stale temp file can only be our own (same path + pid) crash
            // leftover; clear it so `create_new` below cannot get stuck.
            match fs::remove_file(&temp_path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temp_path)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temp_path, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        result
            .into_diagnostic()
            .wrap_err_with(|| format!("writing {}", path.display()))
    }
    #[cfg(not(unix))]
    {
        fs::write(path, content)
            .into_diagnostic()
            .wrap_err_with(|| format!("writing {}", path.display()))
    }
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

    fn temp_config_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "meshx-runtime-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_runtime(config_path: PathBuf) -> Runtime {
        let legacy_config_paths = legacy_config_paths_for(&config_path);
        Runtime {
            http: HttpClient::new(),
            config_path,
            legacy_config_paths,
            api_base: API_BASE.to_string(),
            mcp_base: MCP_BASE.to_string(),
            timeout: Duration::from_secs(5),
            retries: 0,
            refresh_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn auth_tokens(expires_at: u64) -> AuthTokens {
        AuthTokens {
            access_token: "access-token-value".to_string(),
            refresh_token: "refresh-token-value".to_string(),
            expires_in: 3600,
            expires_at,
            token_type: Some("Bearer".to_string()),
            scope: Some("read write".to_string()),
        }
    }

    #[test]
    fn tool_route_is_destructive_follows_the_command_spec_catalog() {
        assert!(!tool_route_is_destructive("search"));
        assert!(!tool_route_is_destructive("/search"));
        assert!(!tool_route_is_destructive("/tools/v2/search"));
        assert!(!tool_route_is_destructive("/get-contact"));
        assert!(!tool_route_is_destructive("/get-groups"));

        assert!(tool_route_is_destructive("/create-contact"));
        assert!(tool_route_is_destructive("/note"));
        assert!(tool_route_is_destructive("/update-group"));
        assert!(tool_route_is_destructive("/merge-contacts"));
    }

    #[test]
    fn tool_route_is_destructive_treats_unknown_and_malformed_routes_as_writes() {
        assert!(tool_route_is_destructive("/definitely-not-a-route"));
        assert!(tool_route_is_destructive(""));
        assert!(tool_route_is_destructive("/tools/v1/search"));
    }

    #[test]
    fn write_secret_file_writes_owner_only_without_leftovers() {
        let dir = temp_config_dir("secret-file");
        let path = dir.join("mesh.json");

        write_secret_file(&path, "{\"first\":true}").unwrap();
        write_secret_file(&path, "{\"second\":true}").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"second\":true}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "token file must be owner-only");
        }
        let leftovers = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name != "mesh.json")
            .collect::<Vec<_>>();
        assert_eq!(leftovers, Vec::<String>::new());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn read_config_migrates_legacy_config_and_removes_the_source() {
        let dir = temp_config_dir("legacy-migration");
        let runtime = test_runtime(dir.join("mesh.json"));
        let legacy_path = &runtime.legacy_config_paths[0];
        let config = MeshConfig {
            auth: Some(auth_tokens(u64::MAX)),
            user: None,
        };
        fs::write(legacy_path, serde_json::to_string(&config).unwrap()).unwrap();

        let migrated = runtime.read_config().unwrap().expect("config migrates");

        assert_eq!(
            migrated.auth.map(|auth| auth.access_token),
            Some("access-token-value".to_string())
        );
        assert!(runtime.config_path.exists(), "new config file is written");
        assert!(
            !legacy_path.exists(),
            "legacy plaintext token file is removed after migration"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn runtime_clones_share_the_refresh_lock() {
        let runtime = test_runtime(temp_config_dir("shared-lock").join("mesh.json"));
        let clone = runtime.clone();

        assert!(Arc::ptr_eq(&runtime.refresh_lock, &clone.refresh_lock));
    }

    #[tokio::test]
    async fn access_token_returns_unexpired_token_without_refreshing() {
        let dir = temp_config_dir("access-token");
        let runtime = test_runtime(dir.join("mesh.json"));
        runtime
            .write_config(&MeshConfig {
                auth: Some(auth_tokens(now_millis().saturating_add(3_600_000))),
                user: None,
            })
            .unwrap();

        // retries = 0 and no reachable server: any refresh attempt would fail,
        // so success proves the fresh token is served straight from disk.
        let token = runtime.access_token().await.unwrap();

        assert_eq!(token, "access-token-value");
        fs::remove_dir_all(&dir).unwrap();
    }
}
