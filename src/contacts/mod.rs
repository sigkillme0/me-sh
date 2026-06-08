mod audit;
mod graph;
mod read;
mod write;
use crate::prelude::*;
pub(crate) use audit::*;
pub(crate) use graph::*;
pub(crate) use read::*;
pub(crate) use write::*;

#[derive(Clone, Debug)]
pub(crate) struct ContactResolveOptions {
    pub(crate) payload: Map<String, Value>,
    pub(crate) candidate_limit: usize,
    pub(crate) one: bool,
    pub(crate) all: bool,
}

impl ContactResolveOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let spec = search_command_spec();
        let mut payload = parse_payload(&spec, matches)?;
        payload.remove("limit");
        let all = matches.get_flag("all");
        if !all && !contacts_resolve_has_search_filter(&payload) {
            return err("contacts:resolve requires at least one search filter, or --all");
        }
        Ok(Self {
            payload,
            candidate_limit: optional_usize_from_matches(matches, "candidate-limit")?
                .unwrap_or(CONTACT_RESOLVE_CANDIDATE_LIMIT_DEFAULT),
            one: matches.get_flag("one"),
            all,
        })
    }
}

pub(crate) fn min_confidence_from_matches(matches: &ArgMatches) -> Result<u64> {
    let raw = matches
        .get_one::<String>("min-confidence")
        .map(String::as_str)
        .unwrap_or("60");
    let value = raw
        .parse::<u64>()
        .into_diagnostic()
        .wrap_err("--min-confidence must be an integer from 0 to 100")?;
    if value > 100 {
        return err("--min-confidence must be at most 100");
    }
    Ok(value)
}

pub(crate) fn live_search_count_dry_run_payload(payload: Option<&Map<String, Value>>) -> Value {
    let mut payload = payload.cloned().unwrap_or_default();
    payload.set("limit", 0_u64);
    Value::Object(payload)
}

pub(crate) fn live_search_page_dry_run_payload(payload: Option<&Map<String, Value>>) -> Value {
    let mut payload = payload.cloned().unwrap_or_default();
    payload.insert(
        "limit".to_string(),
        Value::Number(Number::from(SEARCH_LIMIT_MAX as u64)),
    );
    payload.insert(
        "exclude_contact_ids".to_string(),
        Value::String("accumulated from prior pages".to_string()),
    );
    Value::Object(payload)
}

pub(crate) fn remove_nested_source(report: &mut Value) {
    if let Some(object) = report.as_object_mut() {
        object.remove("source");
    }
}

pub(crate) fn object_values_by_aliases<'a>(
    object: &'a Map<String, Value>,
    aliases: &[&str],
) -> Vec<&'a Value> {
    object
        .iter()
        .filter(|(key, _)| aliases.iter().any(|alias| key_matches(key, &[*alias])))
        .map(|(_, value)| value)
        .collect()
}

pub(crate) fn object_string_by_aliases(
    object: &Map<String, Value>,
    aliases: &[&str],
) -> Option<String> {
    object_values_by_aliases(object, aliases)
        .into_iter()
        .filter_map(value_string)
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

pub(crate) fn contact_set_definitions(
    options: &ContactSetsOptions,
) -> Result<Vec<ContactSegmentDefinition>> {
    contact_segment_definitions_from_input(&options.input, options.input_format, "contacts:sets")
}

pub(crate) fn row_value_by_aliases<'a>(
    row: &'a Map<String, Value>,
    aliases: &[String],
) -> Option<&'a Value> {
    row.iter()
        .find(|(key, _)| {
            aliases
                .iter()
                .any(|alias| key_matches(key, &[alias.as_str()]))
        })
        .map(|(_, value)| value)
}

pub(crate) fn contacts_for_input(
    path: &Path,
    requested_format: InputFormat,
    label: &'static str,
) -> Result<(Value, Vec<Value>)> {
    let text = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", path.display()))?;
    let input_format = requested_format.resolve(path, &text);
    let rows = read_apply_rows(&text, input_format, label)?;
    let contacts = rows.into_iter().map(Value::Object).collect::<Vec<_>>();
    Ok((
        json!({
            "type": "input",
            "path": path.display().to_string(),
            "input_format": input_format.as_str(),
            "analyzed_count": contacts.len(),
        }),
        contacts,
    ))
}

pub(crate) fn contact_name(contact: &Value) -> Option<String> {
    first_contact_string(contact, &["name", "displayName", "display_name"])
}

pub(crate) fn first_contact_string(contact: &Value, keys: &[&str]) -> Option<String> {
    contact.as_object().and_then(|object| {
        keys.iter()
            .filter_map(|key| object.get(*key))
            .filter_map(value_string)
            .map(|value| value.trim().to_string())
            .find(|value| !value.is_empty())
    })
}

pub(crate) fn contact_string_values(contact: &Value, keys: &[&str]) -> Vec<String> {
    let Some(object) = contact.as_object() else {
        return Vec::new();
    };
    let mut values = Vec::new();
    for key in keys {
        if let Some(value) = object.get(*key) {
            collect_contact_strings(value, &mut values);
        }
    }
    dedupe_strings(values)
}

pub(crate) fn collect_contact_strings(value: &Value, values: &mut Vec<String>) {
    match value {
        Value::String(value) => {
            let value = value.trim();
            if !value.is_empty() {
                values.push(value.to_string());
            }
        }
        Value::Number(_) | Value::Bool(_) => {
            let value = cell_string(value);
            if !value.is_empty() {
                values.push(value);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_contact_strings(item, values);
            }
        }
        Value::Object(object) => {
            for key in ["value", "url", "email", "phone", "phone_number", "linkedin"] {
                if let Some(value) = object.get(key) {
                    collect_contact_strings(value, values);
                }
            }
        }
        Value::Null => {}
    }
}

pub(crate) fn normalize_email_key(value: &str) -> Option<String> {
    let value = value
        .trim()
        .trim_start_matches("mailto:")
        .to_ascii_lowercase();
    let at = value.rfind('@')?;
    let local = value[..at]
        .trim_end_matches(|ch: char| !is_email_local_char(ch))
        .rsplit(|ch: char| !is_email_local_char(ch))
        .next()
        .unwrap_or_default();
    if local.is_empty() {
        return None;
    }
    let domain = email_domain_from_string(&value)?;
    Some(format!("{local}@{domain}"))
}

fn is_email_local_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '%' | '+' | '-')
}

pub(crate) fn normalize_phone_key(value: &str) -> Option<String> {
    let digits = value
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect::<String>();
    (digits.len() >= 7).then_some(digits)
}

pub(crate) fn normalize_linkedin_key(value: &str) -> Option<String> {
    let mut value = value.trim().to_ascii_lowercase();
    if let Some(index) = value.find('#') {
        value.truncate(index);
    }
    if let Some(index) = value.find('?') {
        value.truncate(index);
    }
    for prefix in ["https://", "http://"] {
        if let Some(stripped) = value.strip_prefix(prefix) {
            value = stripped.to_string();
        }
    }
    if let Some(stripped) = value.strip_prefix("www.") {
        value = stripped.to_string();
    }
    while value.ends_with('/') {
        value.pop();
    }
    (value == "linkedin.com" || value.starts_with("linkedin.com/")).then_some(value)
}

pub(crate) fn normalize_name_key(value: &str) -> Option<String> {
    let mut normalized = String::new();
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            normalized.push(ch);
        } else if ch.is_whitespace() && !normalized.ends_with(' ') {
            normalized.push(' ');
        }
    }
    let normalized = normalized.trim().to_string();
    (normalized.chars().count() >= 2).then_some(normalized)
}

pub(crate) fn first_object_string(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| object.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn nested_name_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(value)) => {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        }
        Some(Value::Object(object)) => first_object_string(object, &["name", "title"]),
        _ => None,
    }
}

pub(crate) fn insert_string_array(
    payload: &mut Map<String, Value>,
    key: &str,
    values: Vec<String>,
) {
    if !values.is_empty() {
        payload.insert(
            key.to_string(),
            Value::Array(values.into_iter().map(Value::String).collect()),
        );
    }
}

pub(crate) async fn write_bulk_contacts_json_array<W: Write>(
    runtime: &Runtime,
    ids: &[u64],
    concurrency: usize,
    pretty: bool,
    writer: &mut W,
) -> Result<()> {
    writer.write_all(b"[").into_diagnostic()?;
    let mut first = true;
    fetch_contacts_each(runtime, ids, concurrency, |_, row| {
        write_json_array_row(writer, &row, &mut first, pretty)
    })
    .await?;
    if pretty && !first {
        writer.write_all(b"\n").into_diagnostic()?;
    }
    writer.write_all(b"]\n").into_diagnostic()?;
    Ok(())
}

pub(crate) async fn write_json_array_contacts<W: Write>(
    runtime: &Runtime,
    payload: Map<String, Value>,
    page_size: usize,
    pretty: bool,
    writer: &mut W,
) -> Result<()> {
    writer.write_all(b"[").into_diagnostic()?;
    let mut first = true;
    export_all_contacts_each(runtime, payload, page_size, |row| {
        write_json_array_row(writer, &row, &mut first, pretty)
    })
    .await?;
    if pretty && !first {
        writer.write_all(b"\n").into_diagnostic()?;
    }
    writer.write_all(b"]\n").into_diagnostic()?;
    Ok(())
}

pub(crate) fn write_json_array_row<W: Write>(
    writer: &mut W,
    row: &Value,
    first: &mut bool,
    pretty: bool,
) -> Result<()> {
    if pretty {
        if *first {
            writer.write_all(b"\n").into_diagnostic()?;
        } else {
            writer.write_all(b",\n").into_diagnostic()?;
        }
        let text = serde_json::to_string_pretty(row)
            .into_diagnostic()
            .wrap_err("serializing JSON array row")?;
        for (index, line) in text.lines().enumerate() {
            if index != 0 {
                writer.write_all(b"\n").into_diagnostic()?;
            }
            writer.write_all(b"  ").into_diagnostic()?;
            writer.write_all(line.as_bytes()).into_diagnostic()?;
        }
    } else {
        if !*first {
            writer.write_all(b",").into_diagnostic()?;
        }
        serde_json::to_writer(&mut *writer, row)
            .into_diagnostic()
            .wrap_err("serializing compact JSON array row")?;
    }
    *first = false;
    Ok(())
}

pub(crate) fn collect_row_headers(row: &Value, headers: &mut BTreeSet<String>) -> Result<()> {
    let Value::Object(object) = row else {
        return err("me.sh search row was not an object");
    };
    headers.extend(object.keys().cloned());
    Ok(())
}

pub(crate) fn write_delimited_from_jsonl<W: Write>(
    spool_path: &Path,
    headers: &[String],
    delimiter: u8,
    writer: &mut W,
) -> Result<()> {
    if headers.is_empty() {
        return Ok(());
    }

    let file = fs::File::open(spool_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("opening {}", spool_path.display()))?;
    let reader = StdBufReader::new(file);
    let mut csv_writer = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(writer);
    csv_writer.write_record(headers).into_diagnostic()?;
    for (index, line) in reader.lines().enumerate() {
        let line = line
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {} line {}", spool_path.display(), index + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str::<Value>(&line)
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "{} line {} is not valid JSON",
                    spool_path.display(),
                    index + 1
                )
            })?;
        let Value::Object(row) = value else {
            return err(format!(
                "{} line {} is not a JSON object",
                spool_path.display(),
                index + 1
            ));
        };
        let record = headers
            .iter()
            .map(|header| row.get(header).map(cell_string).unwrap_or_default())
            .collect::<Vec<_>>();
        csv_writer.write_record(record).into_diagnostic()?;
    }
    csv_writer.flush().into_diagnostic()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_email_key_lowercases_mailto_addresses() {
        assert_eq!(
            normalize_email_key("mailto:Ada@Example.INVALID"),
            Some("ada@example.invalid".to_string())
        );
    }

    #[test]
    fn normalize_email_key_extracts_address_from_display_text() {
        assert_eq!(
            normalize_email_key("Ada Lovelace <Ada@Example.INVALID>"),
            Some("ada@example.invalid".to_string())
        );
    }

    #[test]
    fn normalize_email_key_stops_before_trailing_text() {
        assert_eq!(
            normalize_email_key("ada@example.invalid phone"),
            Some("ada@example.invalid".to_string())
        );
    }

    #[test]
    fn normalize_email_key_rejects_missing_or_invalid_addresses() {
        assert_eq!(normalize_email_key("not an email"), None);
        assert_eq!(normalize_email_key("@example.com"), None);
        assert_eq!(normalize_email_key("ada@example"), None);
        assert_eq!(normalize_email_key("ada@.com"), None);
        assert_eq!(normalize_email_key("ada@example."), None);
    }

    #[test]
    fn normalize_linkedin_key_normalizes_linkedin_urls() {
        assert_eq!(
            normalize_linkedin_key("https://www.linkedin.com/in/ada/?trk=profile"),
            Some("linkedin.com/in/ada".to_string())
        );
    }

    #[test]
    fn normalize_linkedin_key_rejects_non_linkedin_hosts() {
        assert_eq!(
            normalize_linkedin_key("https://notlinkedin.com/in/ada"),
            None
        );
        assert_eq!(
            normalize_linkedin_key("https://example.com/linkedin/ada"),
            None
        );
    }
}
