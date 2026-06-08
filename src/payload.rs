use crate::prelude::*;

/// Ergonomic insert for JSON object payloads: `payload.set("limit", 0)` instead
/// of `payload.insert("limit".to_string(), Value::Number(Number::from(0)))`.
/// `Into<Value>` covers integers, strings, bools, and `Value` itself, so call
/// sites stop hand-constructing `Value::*` wrappers.
pub(crate) trait MapExt {
    fn set(&mut self, key: &str, value: impl Into<Value>);
}

impl MapExt for Map<String, Value> {
    fn set(&mut self, key: &str, value: impl Into<Value>) {
        self.insert(key.to_string(), value.into());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InputFormat {
    Auto,
    Json,
    Jsonl,
    Csv,
    Tsv,
}

impl InputFormat {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "auto" => Ok(Self::Auto),
            "json" => Ok(Self::Json),
            "jsonl" | "ndjson" => Ok(Self::Jsonl),
            "csv" => Ok(Self::Csv),
            "tsv" => Ok(Self::Tsv),
            other => err(format!(
                "--input-format must be auto, json, jsonl, csv, or tsv; got {other}"
            )),
        }
    }

    pub(crate) fn resolve(self, path: &Path, text: &str) -> Self {
        if self != Self::Auto {
            return self;
        }
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref()
        {
            Some("json") => Self::Json,
            Some("jsonl" | "ndjson") => Self::Jsonl,
            Some("tsv") => Self::Tsv,
            Some("csv") => Self::Csv,
            _ if text.trim_start().starts_with('[') || text.trim_start().starts_with('{') => {
                Self::Json
            }
            _ => Self::Csv,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Json => "json",
            Self::Jsonl => "jsonl",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ApplyKind {
    Create,
    Update,
    Archive,
    Restore,
    Note,
}

impl ApplyKind {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "create" | "contacts:create" => Ok(Self::Create),
            "update" | "contacts:update" => Ok(Self::Update),
            "archive" | "contacts:archive" => Ok(Self::Archive),
            "restore" | "contacts:restore" => Ok(Self::Restore),
            "note" | "notes:create" | "create-note" | "create_note" => Ok(Self::Note),
            other => err(format!(
                "action must be create, update, archive, restore, or note; got {other}"
            )),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Archive => "archive",
            Self::Restore => "restore",
            Self::Note => "note",
        }
    }

    pub(crate) fn route(self) -> &'static str {
        match self {
            Self::Create => route::CREATE_CONTACT,
            Self::Update => route::UPDATE_CONTACT,
            Self::Archive => route::ARCHIVE_CONTACT,
            Self::Restore => route::RESTORE_CONTACT,
            Self::Note => route::NOTE,
        }
    }
}

pub(crate) fn single_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn truncate_chars(value: &str, max: usize) -> String {
    let mut output = value.chars().take(max).collect::<String>();
    if value.chars().count() > max {
        output.push_str("...");
    }
    output
}

pub(crate) fn read_apply_rows(
    text: &str,
    input_format: InputFormat,
    command: &'static str,
) -> Result<Vec<Map<String, Value>>> {
    match input_format {
        InputFormat::Json => read_apply_json_rows(text, command),
        InputFormat::Jsonl => read_apply_jsonl_rows(text, command),
        InputFormat::Csv => read_apply_delimited_rows(text, b',', command),
        InputFormat::Tsv => read_apply_delimited_rows(text, b'\t', command),
        InputFormat::Auto => unreachable!("auto input format must be resolved before reading"),
    }
}

pub(crate) fn read_apply_json_rows(
    text: &str,
    command: &'static str,
) -> Result<Vec<Map<String, Value>>> {
    let value: Value = serde_json::from_str(text)
        .into_diagnostic()
        .wrap_err_with(|| format!("{command} JSON input must be valid JSON"))?;
    match value {
        Value::Array(items) => items
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                value.as_object().cloned().ok_or_else(|| {
                    miette!(
                        "{command} JSON row {} must be an object",
                        index.saturating_add(1)
                    )
                })
            })
            .collect(),
        Value::Object(mut object) => {
            for key in ["actions", "contacts", "rows"] {
                if let Some(Value::Array(items)) = object.remove(key) {
                    return items
                        .into_iter()
                        .enumerate()
                        .map(|(index, value)| {
                            value.as_object().cloned().ok_or_else(|| {
                                miette!(
                                    "{command} JSON {key} row {} must be an object",
                                    index.saturating_add(1)
                                )
                            })
                        })
                        .collect();
                }
            }
            Ok(vec![object])
        }
        _ => err(format!(
            "{command} JSON input must be an object or array of objects"
        )),
    }
}

pub(crate) fn read_apply_jsonl_rows(
    text: &str,
    command: &'static str,
) -> Result<Vec<Map<String, Value>>> {
    let mut rows = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(trimmed)
            .into_diagnostic()
            .wrap_err_with(|| format!("{command} JSONL line {} is invalid", line_index + 1))?;
        let Value::Object(object) = value else {
            return err(format!(
                "{command} JSONL line {} must be an object",
                line_index + 1
            ));
        };
        rows.push(object);
    }
    Ok(rows)
}

pub(crate) fn read_apply_delimited_rows(
    text: &str,
    delimiter: u8,
    command: &'static str,
) -> Result<Vec<Map<String, Value>>> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .from_reader(text.as_bytes());
    let headers = reader
        .headers()
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {command} headers"))?
        .iter()
        .map(str::trim)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {command} record"))?;
        let mut row = Map::new();
        for (header, cell) in headers.iter().zip(record.iter()) {
            let header = header.trim();
            let cell = cell.trim();
            if !header.is_empty() && !cell.is_empty() {
                row.insert(header.to_string(), Value::String(cell.to_string()));
            }
        }
        if !row.is_empty() {
            rows.push(row);
        }
    }
    Ok(rows)
}

pub(crate) fn search_target_payload_from_matches(
    matches: &ArgMatches,
    all_search: bool,
    command: &str,
) -> Result<Map<String, Value>> {
    let spec = search_command_spec();
    let mut payload = parse_payload(&spec, matches)?;
    payload.remove("limit");
    payload.remove("include_fields");
    if !contacts_resolve_has_search_filter(&payload) && !all_search {
        return err(format!(
            "{command} --from-search requires at least one search filter, or --all-search"
        ));
    }
    Ok(payload)
}

pub(crate) fn parse_payload(
    spec: &CommandSpec,
    matches: &ArgMatches,
) -> Result<Map<String, Value>> {
    let mut payload = Map::new();
    for option in &spec.options {
        let values = collect_values(matches, option.flag);
        let value = coerce_option(option, &values)?;
        if let Some(value) = value {
            payload.insert(camel_to_snake(option.name), value);
        } else if option.required {
            return err(format!("missing required option --{}", option.flag));
        }
    }
    Ok(nest_payload(payload, spec.nested))
}

pub(crate) fn normalize_apply_key(key: &str) -> String {
    key.chars()
        .filter(|ch| *ch != '_' && *ch != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

pub(crate) fn key_matches(key: &str, aliases: &[&str]) -> bool {
    let key = normalize_apply_key(key);
    aliases
        .iter()
        .any(|alias| key == normalize_apply_key(alias))
}

pub(crate) fn row_string(row: &Map<String, Value>, aliases: &[&str]) -> Option<String> {
    row.iter()
        .find(|(key, _)| key_matches(key, aliases))
        .and_then(|(_, value)| value_string(value))
}

pub(crate) fn row_value<'a>(row: &'a Map<String, Value>, aliases: &[&str]) -> Option<&'a Value> {
    row.iter()
        .find(|(key, _)| key_matches(key, aliases))
        .map(|(_, value)| value)
}

pub(crate) fn value_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Array(_) | Value::Object(_) => Some(cell_string(value)),
    }
}

pub(crate) fn string_array_from_value(value: &Value) -> Vec<String> {
    match value {
        Value::Array(_) => restore_strings(Some(value)),
        Value::String(value) => split_list_value(value),
        Value::Null => Vec::new(),
        other => vec![cell_string(other)],
    }
    .into_iter()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
    .collect()
}

pub(crate) fn json_value_from_input(value: &Value, field: &str) -> Result<Option<Value>> {
    match value {
        Value::Null => Ok(None),
        Value::Array(_) | Value::Object(_) => Ok(Some(value.clone())),
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                return Ok(None);
            }
            let parsed = serde_json::from_str::<Value>(text)
                .into_diagnostic()
                .wrap_err_with(|| format!("{field} must be valid JSON"))?;
            Ok(Some(parsed))
        }
        _ => err(format!("{field} must be a JSON array or object")),
    }
}

pub(crate) fn row_u64(row: &Map<String, Value>, aliases: &[&str]) -> Result<Option<u64>> {
    let Some(value) = row_value(row, aliases) else {
        return Ok(None);
    };
    parse_u64_value(value).map(Some)
}

pub(crate) fn row_u64_array(row: &Map<String, Value>, aliases: &[&str]) -> Result<Vec<u64>> {
    let Some(value) = row_value(row, aliases) else {
        return Ok(Vec::new());
    };
    match value {
        Value::Array(items) => items.iter().map(parse_u64_value).collect(),
        Value::String(text) => split_list_value(text)
            .into_iter()
            .map(|value| {
                parse_contact_id(&value)
                    .into_diagnostic()
                    .wrap_err("contact ID values must be positive integers")
            })
            .collect(),
        other => parse_u64_value(other).map(|value| vec![value]),
    }
}

pub(crate) fn parse_u64_value(value: &Value) -> Result<u64> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .ok_or_else(|| miette!("contact ID values must be positive integers")),
        Value::String(text) => parse_contact_id(text)
            .into_diagnostic()
            .wrap_err("contact ID values must be positive integers"),
        _ => err("contact ID values must be positive integers"),
    }
}

pub(crate) fn collect_values(matches: &ArgMatches, flag: &str) -> Vec<String> {
    matches
        .try_get_many::<String>(flag)
        .ok()
        .flatten()
        .map(|values| values.cloned().collect())
        .unwrap_or_default()
}

pub(crate) fn coerce_option(option: &OptionSpec, values: &[String]) -> Result<Option<Value>> {
    if values.is_empty() {
        return Ok(option.default.as_ref().map(DefaultValue::to_json));
    }

    let value = match option.kind {
        ValueKind::String => {
            let value = values.last().cloned().unwrap_or_default();
            validate_allowed(option, std::slice::from_ref(&value))?;
            Value::String(value)
        }
        ValueKind::Number => {
            let value = values.last().cloned().unwrap_or_default();
            parse_number_value(&value, option.flag)?
        }
        ValueKind::Boolean => {
            let value = values.last().map(String::as_str).unwrap_or("true");
            Value::Bool(parse_bool(value, option.flag)?)
        }
        ValueKind::ArrayString => {
            let mut parts = split_list_values(values);
            if option.flag == "include-fields" {
                parts = parts
                    .into_iter()
                    .map(|value| normalize_include_field(&value))
                    .collect();
            }
            validate_allowed(option, &parts)?;
            Value::Array(parts.into_iter().map(Value::String).collect())
        }
        ValueKind::ArrayNumber => {
            let parts = split_list_values(values);
            Value::Array(
                parts
                    .into_iter()
                    .map(|value| parse_number_value(&value, option.flag))
                    .collect::<Result<Vec<_>>>()?,
            )
        }
        ValueKind::ArrayMixed => {
            let parts = split_list_values(values);
            Value::Array(parts.into_iter().map(mixed_value).collect())
        }
        ValueKind::Json => {
            let text = values.last().map(String::as_str).unwrap_or("null");
            serde_json::from_str(text)
                .into_diagnostic()
                .wrap_err_with(|| format!("--{} must be valid JSON", option.flag))?
        }
    };
    Ok(Some(value))
}

pub(crate) fn split_list_values(values: &[String]) -> Vec<String> {
    values
        .iter()
        .flat_map(|value| split_list_value(value))
        .collect()
}

pub(crate) fn split_list_value(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    if let Some(items) = json_array_strings(trimmed) {
        return items;
    }
    trimmed
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn json_array_strings(trimmed: &str) -> Option<Vec<String>> {
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return None;
    }
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(trimmed) else {
        return None;
    };
    Some(
        items
            .into_iter()
            .map(|item| match item {
                Value::String(value) => value,
                other => cell_string(&other),
            })
            .collect(),
    )
}

pub(crate) fn normalize_include_field(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase().replace('-', "_");
    match normalized.as_str() {
        "email" | "email_address" | "email_addresses" => "emails".to_string(),
        "phone" | "phones" | "phone_number" => "phone_numbers".to_string(),
        "linkedin" | "linked_in" | "social" | "socials" | "social_link" => {
            "social_links".to_string()
        }
        "work" => "work_history".to_string(),
        "education" => "education_history".to_string(),
        "interaction" | "interactions" => "interaction_history".to_string(),
        "message" | "messages" | "text" | "texts" | "text_message" | "text_messages" => {
            "message_history".to_string()
        }
        "event" | "events" => "event_history".to_string(),
        "note" => "notes".to_string(),
        "integration" => "integrations".to_string(),
        _ => normalized,
    }
}

pub(crate) fn include_fields_from_matches(matches: &ArgMatches, flag: &str) -> Result<Vec<String>> {
    let raw = split_list_values(&collect_values(matches, flag));
    let mut seen = BTreeSet::new();
    let mut fields = Vec::new();
    let mut invalid = Vec::new();
    for value in raw {
        let normalized = normalize_include_field(&value);
        if !SEARCH_INCLUDE_FIELDS.contains(&normalized.as_str()) {
            invalid.push(value);
            continue;
        }
        if seen.insert(normalized.clone()) {
            fields.push(normalized);
        }
    }
    if !invalid.is_empty() {
        return err(format!("--{flag} invalid value(s): {}", invalid.join(", ")));
    }
    Ok(fields)
}

pub(crate) fn parse_list_numbers(values: &[String], flag: &str) -> Result<Vec<u64>> {
    split_list_values(values)
        .into_iter()
        .map(|value| {
            parse_contact_id(&value)
                .into_diagnostic()
                .wrap_err_with(|| format!("--{flag} must contain only numbers"))
        })
        .collect()
}

pub(crate) fn parse_contact_id(value: &str) -> std::result::Result<u64, std::num::ParseIntError> {
    value.trim().parse::<u64>()
}

pub(crate) fn contact_ids_from_matches(matches: &ArgMatches, flag: &str) -> Result<Vec<u64>> {
    let mut values = collect_values(matches, flag);
    if let Some(path) = matches
        .try_get_one::<String>("input")
        .ok()
        .flatten()
        .map(String::as_str)
    {
        let text = fs::read_to_string(path)
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {path}"))?;
        values.extend(ids_from_text(&text));
    }
    let ids = parse_list_numbers(&values, flag)?;
    if ids.is_empty() {
        return err(format!("provide --{flag} or --input with at least one ID"));
    }
    Ok(ids)
}

pub(crate) fn optional_ids_from_matches(matches: &ArgMatches, flag: &str) -> Result<Vec<u64>> {
    parse_list_numbers(&collect_values(matches, flag), flag)
}

pub(crate) fn optional_usize_from_matches(
    matches: &ArgMatches,
    flag: &str,
) -> Result<Option<usize>> {
    optional_usize_with_bounds(matches, flag, false, Some(SEARCH_LIMIT_MAX))
}

pub(crate) fn optional_positive_usize_from_matches(
    matches: &ArgMatches,
    flag: &str,
) -> Result<Option<usize>> {
    optional_usize_with_bounds(matches, flag, false, None)
}

pub(crate) fn optional_nonnegative_usize_from_matches(
    matches: &ArgMatches,
    flag: &str,
) -> Result<Option<usize>> {
    optional_usize_with_bounds(matches, flag, true, None)
}

fn optional_usize_with_bounds(
    matches: &ArgMatches,
    flag: &str,
    allow_zero: bool,
    max: Option<usize>,
) -> Result<Option<usize>> {
    let Some(raw) = matches.get_one::<String>(flag) else {
        return Ok(None);
    };
    let label = if allow_zero {
        "non-negative"
    } else {
        "positive"
    };
    let value = raw
        .parse::<usize>()
        .into_diagnostic()
        .wrap_err_with(|| format!("--{flag} must be a {label} integer"))?;
    if !allow_zero && value == 0 {
        return err(format!("--{flag} must be greater than zero"));
    }
    if let Some(max) = max
        && value > max
    {
        return err(format!("--{flag} must be at most {max}"));
    }
    Ok(Some(value))
}

pub(crate) fn optional_ratio_from_matches(matches: &ArgMatches, flag: &str) -> Result<Option<f64>> {
    let Some(raw) = matches.get_one::<String>(flag) else {
        return Ok(None);
    };
    let value = raw
        .parse::<f64>()
        .into_diagnostic()
        .wrap_err_with(|| format!("--{flag} must be a number from 0 to 1"))?;
    if !(0.0..=1.0).contains(&value) || !value.is_finite() {
        return err(format!("--{flag} must be a finite number from 0 to 1"));
    }
    Ok(Some(value))
}

pub(crate) fn contact_fetch_concurrency(matches: &ArgMatches, flag: &str) -> Result<usize> {
    Ok(
        optional_usize_with_bounds(matches, flag, false, Some(CONTACT_FETCH_CONCURRENCY_MAX))?
            .unwrap_or(CONTACT_FETCH_CONCURRENCY_DEFAULT),
    )
}

pub(crate) fn dedupe_ids(ids: Vec<u64>) -> Vec<u64> {
    let mut seen = BTreeSet::new();
    ids.into_iter().filter(|id| seen.insert(*id)).collect()
}

pub(crate) fn ids_from_text(text: &str) -> Vec<String> {
    let trimmed = text.trim();
    if let Some(items) = json_array_strings(trimmed) {
        return items;
    }
    trimmed
        .split(|ch: char| ch == ',' || ch.is_whitespace())
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn parse_number_value(value: &str, flag: &str) -> Result<Value> {
    if let Some(number) = integer_number_from_text(value) {
        return Ok(number);
    }
    let number = value
        .parse::<f64>()
        .into_diagnostic()
        .wrap_err_with(|| format!("--{flag} must be a number"))?;
    number_value(number).ok_or_else(|| miette!("--{flag} must be a finite number"))
}

fn integer_number_from_text(value: &str) -> Option<Value> {
    if let Ok(number) = value.parse::<i64>() {
        return Some(Value::Number(Number::from(number)));
    }
    if let Ok(number) = value.parse::<u64>() {
        return Some(Value::Number(Number::from(number)));
    }
    None
}

pub(crate) fn number_value(value: f64) -> Option<Value> {
    if value.fract() == 0.0 && value <= i64::MAX as f64 && value >= i64::MIN as f64 {
        Some(Value::Number(Number::from(value as i64)))
    } else {
        Number::from_f64(value).map(Value::Number)
    }
}

pub(crate) fn parse_bool(value: &str, flag: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "y" | "on" => Ok(true),
        "false" | "0" | "no" | "n" | "off" => Ok(false),
        _ => err(format!("--{flag} must be true or false")),
    }
}

pub(crate) fn mixed_value(value: String) -> Value {
    if let Some(number) = integer_number_from_text(&value) {
        return number;
    }
    if let Ok(number) = value.parse::<f64>()
        && !value.trim().is_empty()
        && let Some(number) = number_value(number)
    {
        return number;
    }
    Value::String(value)
}

pub(crate) fn validate_allowed(option: &OptionSpec, values: &[String]) -> Result<()> {
    if option.allowed.is_empty() {
        return Ok(());
    }
    let invalid = values
        .iter()
        .filter(|value| !option.allowed.contains(&value.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if invalid.is_empty() {
        Ok(())
    } else {
        err(format!(
            "--{} invalid value(s): {}",
            option.flag,
            invalid.join(", ")
        ))
    }
}

pub(crate) fn nest_payload(
    mut payload: Map<String, Value>,
    nested: &[NestedPrefix],
) -> Map<String, Value> {
    for group in nested {
        let mut object = Map::new();
        for suffix in group.suffixes {
            let source_key = format!("{}_{}", group.prefix, suffix);
            if let Some(value) = payload.remove(&source_key) {
                object.insert((*suffix).to_string(), value);
            }
        }
        if !object.is_empty() {
            payload.insert(group.prefix.to_string(), Value::Object(object));
        }
    }
    payload
}

pub(crate) fn camel_to_snake(value: &str) -> String {
    let mut result = String::with_capacity(value.len() + 4);
    for ch in value.chars() {
        if ch.is_ascii_uppercase() {
            result.push('_');
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}

pub(crate) fn parse_json_object(text: &str, label: &str) -> Result<Map<String, Value>> {
    match serde_json::from_str::<Value>(text)
        .into_diagnostic()
        .wrap_err_with(|| format!("{label} must be valid JSON"))?
    {
        Value::Object(object) => Ok(object),
        _ => err(format!("{label} must be a JSON object")),
    }
}

pub(crate) fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

pub(crate) fn system_time_unix_ms(time: SystemTime) -> Result<u64> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .into_diagnostic()
        .wrap_err("system time is before the Unix epoch")?;
    duration
        .as_millis()
        .try_into()
        .into_diagnostic()
        .wrap_err("system time is too large")
}

pub(crate) fn elapsed_millis(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

pub(crate) fn move_temp_output(temp_path: &Path, output_path: &Path, label: &str) -> Result<()> {
    if let Err(error) = fs::rename(temp_path, output_path) {
        cleanup_export_spool_best_effort(temp_path);
        return Err(error)
            .into_diagnostic()
            .wrap_err_with(|| format!("moving {} to {label}", temp_path.display()));
    }
    Ok(())
}

pub(crate) fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

pub(crate) fn canonical_json_value(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(canonical_json_value).collect()),
        Value::Object(object) => {
            let mut entries = object.iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            let mut canonical = Map::new();
            for (key, value) in entries {
                canonical.insert(key.clone(), canonical_json_value(value));
            }
            Value::Object(canonical)
        }
        other => other.clone(),
    }
}

pub(crate) fn write_jsonl_row<W: Write>(writer: &mut W, row: &Value) -> Result<()> {
    serde_json::to_writer(&mut *writer, row)
        .into_diagnostic()
        .wrap_err("serializing JSONL row")?;
    writer.write_all(b"\n").into_diagnostic()?;
    Ok(())
}

pub(crate) fn write_jsonl_row_hashed<W: Write>(
    writer: &mut W,
    row: &Value,
    hasher: &mut Sha256,
    bytes: &mut usize,
) -> Result<()> {
    let line = serde_json::to_vec(row)
        .into_diagnostic()
        .wrap_err("serializing JSONL row")?;
    writer.write_all(&line).into_diagnostic()?;
    writer.write_all(b"\n").into_diagnostic()?;
    hasher.update(&line);
    hasher.update(b"\n");
    *bytes = bytes.saturating_add(line.len()).saturating_add(1);
    Ok(())
}

pub(crate) fn validate_merge_ids(ids: &[u64]) -> Result<()> {
    if ids.len() < 2 || ids.len() > 10 {
        return err("contacts:merge requires 2 to 10 --contact-ids");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Arg, Command};

    fn matches_with_count(value: Option<&str>) -> ArgMatches {
        let mut args = vec!["mesh"];
        if let Some(value) = value {
            args.extend(["--count", value]);
        }
        Command::new("mesh")
            .arg(Arg::new("count").long("count"))
            .try_get_matches_from(args)
            .unwrap()
    }

    fn assert_error_contains<T: std::fmt::Debug>(result: Result<T>, needle: &str) {
        let error = result.unwrap_err();
        let message = error.to_string();

        assert!(
            message.contains(needle),
            "expected error to contain {needle:?}, got {message:?}"
        );
    }

    #[test]
    fn split_list_value_accepts_json_array_values() {
        let values = split_list_value(r#"[1, "two", true, {"a":1}]"#);

        assert_eq!(values, vec!["1", "two", "true", r#"{"a":1}"#]);
    }

    #[test]
    fn split_list_value_falls_back_to_commas() {
        let values = split_list_value(" one, two words, , three ");

        assert_eq!(values, vec!["one", "two words", "three"]);
    }

    #[test]
    fn ids_from_text_accepts_json_array_values() {
        let values = ids_from_text(r#"[1, "2", true, {"id":3}]"#);

        assert_eq!(values, vec!["1", "2", "true", r#"{"id":3}"#]);
    }

    #[test]
    fn ids_from_text_splits_commas_and_whitespace() {
        let values = ids_from_text("1, 2\n3\t4");

        assert_eq!(values, vec!["1", "2", "3", "4"]);
    }

    #[test]
    fn parse_number_value_preserves_large_integer_text() -> Result<()> {
        let value = parse_number_value("9007199254740993", "contact-id")?;

        assert_eq!(value, json!(9007199254740993_u64));
        Ok(())
    }

    #[test]
    fn parse_number_value_keeps_decimal_values() -> Result<()> {
        let value = parse_number_value("0.75", "min-score")?;

        assert_eq!(value, json!(0.75));
        Ok(())
    }

    #[test]
    fn mixed_value_preserves_large_integer_text() {
        let value = mixed_value("9007199254740993".to_string());

        assert_eq!(value, json!(9007199254740993_u64));
    }

    #[test]
    fn optional_usize_from_matches_accepts_positive_values_up_to_search_limit() {
        assert_eq!(
            optional_usize_from_matches(&matches_with_count(None), "count").unwrap(),
            None
        );
        assert_eq!(
            optional_usize_from_matches(&matches_with_count(Some("1")), "count").unwrap(),
            Some(1)
        );
        assert_eq!(
            optional_usize_from_matches(&matches_with_count(Some("1000")), "count").unwrap(),
            Some(1000)
        );
    }

    #[test]
    fn optional_usize_from_matches_rejects_zero_and_values_over_search_limit() {
        assert_error_contains(
            optional_usize_from_matches(&matches_with_count(Some("0")), "count"),
            "greater than zero",
        );
        assert_error_contains(
            optional_usize_from_matches(&matches_with_count(Some("1001")), "count"),
            "at most 1000",
        );
    }

    #[test]
    fn optional_positive_usize_from_matches_does_not_cap_at_search_limit() {
        assert_eq!(
            optional_positive_usize_from_matches(&matches_with_count(Some("1001")), "count")
                .unwrap(),
            Some(1001)
        );
        assert_error_contains(
            optional_positive_usize_from_matches(&matches_with_count(Some("0")), "count"),
            "greater than zero",
        );
    }

    #[test]
    fn optional_nonnegative_usize_from_matches_allows_zero() {
        assert_eq!(
            optional_nonnegative_usize_from_matches(&matches_with_count(None), "count").unwrap(),
            None
        );
        assert_eq!(
            optional_nonnegative_usize_from_matches(&matches_with_count(Some("0")), "count")
                .unwrap(),
            Some(0)
        );
    }

    #[test]
    fn contact_fetch_concurrency_uses_default_and_accepts_max() {
        assert_eq!(
            contact_fetch_concurrency(&matches_with_count(None), "count").unwrap(),
            CONTACT_FETCH_CONCURRENCY_DEFAULT
        );
        assert_eq!(
            contact_fetch_concurrency(&matches_with_count(Some("16")), "count").unwrap(),
            16
        );
    }

    #[test]
    fn contact_fetch_concurrency_rejects_zero_and_values_over_max() {
        assert_error_contains(
            contact_fetch_concurrency(&matches_with_count(Some("0")), "count"),
            "greater than zero",
        );
        assert_error_contains(
            contact_fetch_concurrency(&matches_with_count(Some("17")), "count"),
            "at most 16",
        );
    }
}
