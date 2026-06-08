use crate::prelude::*;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct MeshConfig {
    pub(crate) auth: Option<AuthTokens>,
    pub(crate) user: Option<MeshUser>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct AuthTokens {
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    pub(crate) expires_in: u64,
    #[serde(default)]
    pub(crate) expires_at: u64,
    pub(crate) token_type: Option<String>,
    pub(crate) scope: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct MeshUser {
    pub(crate) id: Option<Value>,
    pub(crate) email: Option<String>,
    pub(crate) first_name: Option<String>,
    pub(crate) last_name: Option<String>,
}

pub(crate) fn read_config_file(path: &Path) -> Result<Option<MeshConfig>> {
    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content)
            .into_diagnostic()
            .wrap_err_with(|| format!("parsing {}", path.display()))
            .map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error)
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {}", path.display())),
    }
}

pub(crate) fn default_config_path() -> PathBuf {
    let home = BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".config").join(CONFIG_FILE)
}

pub(crate) fn legacy_config_paths_for(config_path: &Path) -> Vec<PathBuf> {
    LEGACY_CONFIG_FILES
        .iter()
        .map(|file| {
            config_path
                .parent()
                .map(|parent| parent.join(file))
                .unwrap_or_else(|| PathBuf::from(file))
        })
        .collect()
}

pub(crate) fn token_expired(auth: &AuthTokens) -> bool {
    now_millis().saturating_add(60_000) >= auth.expires_at
}

pub(crate) fn tokens_from_value(value: Value) -> Result<AuthTokens> {
    let mut auth: AuthTokens = serde_json::from_value(value)
        .into_diagnostic()
        .wrap_err("decoding OAuth token response")?;
    if auth.expires_at == 0 {
        auth.expires_at = now_millis().saturating_add(auth.expires_in.saturating_mul(1000));
    }
    Ok(auth)
}

pub(crate) fn user_to_map(user: MeshUser) -> Map<String, Value> {
    serde_json::to_value(user)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

pub(crate) fn redact_config_value(config: &MeshConfig) -> Value {
    let mut value = serde_json::to_value(config).unwrap_or(Value::Null);
    if let Some(auth) = value.get_mut("auth").and_then(Value::as_object_mut) {
        for key in ["access_token", "refresh_token"] {
            if let Some(Value::String(token)) = auth.get_mut(key) {
                *token = redact_token(token);
            }
        }
    }
    value
}

pub(crate) fn redact_token(token: &str) -> String {
    let char_count = token.chars().count();
    if char_count <= 12 {
        return "<redacted>".to_string();
    }
    let prefix = token.chars().take(6).collect::<String>();
    let suffix = token.chars().skip(char_count - 4).collect::<String>();
    format!("{prefix}...{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_token_keeps_ascii_shape() {
        assert_eq!(redact_token("abcdefghijklmnop"), "abcdef...mnop");
    }

    #[test]
    fn redact_token_handles_non_ascii_tokens() {
        let token = "😀😁😂😃😄😅😆😉😊😋😎😍😘";

        assert_eq!(redact_token(token), "😀😁😂😃😄😅...😋😎😍😘");
    }
}
