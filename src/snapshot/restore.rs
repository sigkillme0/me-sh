use crate::prelude::*;

#[derive(Clone, Copy, Debug)]
pub(crate) enum RestoreMode {
    Update,
    Create,
}

impl RestoreMode {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        match matches
            .get_one::<String>("mode")
            .map(String::as_str)
            .unwrap_or("update")
        {
            "update" => Ok(Self::Update),
            "create" => Ok(Self::Create),
            mode => err(format!("unknown restore mode {mode}")),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Update => "update",
            Self::Create => "create",
        }
    }

    pub(crate) fn route(self) -> &'static str {
        match self {
            Self::Update => route::UPDATE_CONTACT,
            Self::Create => route::CREATE_CONTACT,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RestoreAction {
    pub(crate) source_id: u64,
    pub(crate) route: &'static str,
    pub(crate) payload: Map<String, Value>,
    pub(crate) notes: Vec<Map<String, Value>>,
}

pub(crate) fn snapshot_restore_contacts(dir: &Path, ids: &[u64]) -> Result<Vec<Value>> {
    let verify = verify_snapshot(dir)?;
    if !verify.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return err("snapshot failed manifest verification");
    }
    if !snapshot_manifest_has_file(dir, "full_contacts")? {
        return err(
            "snapshot does not contain full_contacts. Recreate it with --full-contacts or --full-contact-ids.",
        );
    }

    let path = snapshot_manifest_file_path(dir, "full_contacts")?;
    let contacts = read_snapshot_jsonl_values_at_path(&path)?;
    let mut by_id = BTreeMap::new();
    for (index, contact) in contacts.iter().enumerate() {
        if let Some(id) = record_id(contact) {
            insert_snapshot_record(&mut by_id, id, contact, &path, format!("row {}", index + 1))?;
        }
    }

    if ids.is_empty() {
        return Ok(contacts);
    }

    let mut selected = Vec::with_capacity(ids.len());
    for id in ids {
        let key = id.to_string();
        let contact = by_id
            .get(&key)
            .ok_or_else(|| miette!("snapshot {} does not contain contact {id}", path.display()))?;
        selected.push((*contact).clone());
    }
    Ok(selected)
}

pub(crate) fn snapshot_restore_plan(
    contacts: &[Value],
    mode: RestoreMode,
    include_notes: bool,
) -> Result<Vec<RestoreAction>> {
    contacts
        .iter()
        .map(|contact| restore_action_from_contact(contact, mode, include_notes))
        .collect()
}

pub(crate) fn restore_action_from_contact(
    contact: &Value,
    mode: RestoreMode,
    include_notes: bool,
) -> Result<RestoreAction> {
    let source_id = contact_id_from_value(contact)
        .ok_or_else(|| miette!("snapshot contact is missing numeric id"))?;
    let mut payload = restore_payload_from_contact(contact)?;
    if matches!(mode, RestoreMode::Update) {
        payload.insert(
            "contact_id".to_string(),
            Value::Number(Number::from(source_id)),
        );
    }
    if payload.is_empty() || (payload.len() == 1 && payload.contains_key("contact_id")) {
        return err(format!(
            "snapshot contact {source_id} does not contain restorable contact fields"
        ));
    }
    Ok(RestoreAction {
        source_id,
        route: mode.route(),
        payload,
        notes: if include_notes {
            restore_notes_from_contact(contact)?
        } else {
            Vec::new()
        },
    })
}

pub(crate) async fn apply_snapshot_restore(
    runtime: &Runtime,
    mode: RestoreMode,
    include_notes: bool,
    actions: Vec<RestoreAction>,
) -> Result<Value> {
    let mut results = Vec::with_capacity(actions.len());
    for action in actions {
        let data = runtime
            .call_tool(action.route, Value::Object(action.payload.clone()))
            .await
            .wrap_err_with(|| format!("restoring contact {}", action.source_id))?;
        let target_id = if matches!(mode, RestoreMode::Update) {
            action.source_id
        } else {
            record_id(&data)
                .and_then(|id| id.parse::<u64>().ok())
                .ok_or_else(|| {
                    miette!("created contact for {} did not return id", action.source_id)
                })?
        };
        let mut note_results = Vec::with_capacity(action.notes.len());
        for note in &action.notes {
            let mut note_payload = note.clone();
            note_payload.insert(
                "contact_id".to_string(),
                Value::Number(Number::from(target_id)),
            );
            let note_result = runtime
                .call_tool(route::NOTE, Value::Object(note_payload))
                .await
                .wrap_err_with(|| format!("restoring note for contact {}", action.source_id))?;
            note_results.push(note_result);
        }
        results.push(json!({
            "source_id": action.source_id,
            "target_id": target_id,
            "route": format!("/tools/v2{}", action.route),
            "result_id": record_id(&data),
            "result": data,
            "notes_created": note_results.len(),
            "note_results": note_results,
        }));
    }
    let notes_created = results
        .iter()
        .filter_map(|result| result.get("notes_created").and_then(Value::as_u64))
        .sum::<u64>();
    Ok(json!({
        "mode": mode.as_str(),
        "include_notes": include_notes,
        "changed_count": results.len(),
        "notes_created": notes_created,
        "results": results,
    }))
}

pub(crate) fn restore_payload_from_contact(contact: &Value) -> Result<Map<String, Value>> {
    let mut payload = Map::new();
    if let Some((first, last)) = restore_contact_name(contact) {
        payload.set("first_name", first);
        if let Some(last) = last {
            payload.set("last_name", last);
        }
    }
    insert_string_array(
        &mut payload,
        "email",
        restore_strings(contact.get("emails")),
    );
    insert_string_array(
        &mut payload,
        "phone",
        restore_strings(contact.get("phone_numbers")),
    );
    if let Some(linkedin) = restore_linkedin(contact) {
        payload.set("linkedin", linkedin);
    }
    if let Some(locations) = restore_locations(contact.get("location")) {
        payload.insert("locations".to_string(), locations);
    }
    if let Some(birthday) = contact
        .get("birthday")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload.set("birthday", birthday.to_string());
    }
    if let Some((title, organization)) = restore_current_work(contact) {
        if let Some(title) = title {
            payload.set("title", title);
        }
        if let Some(organization) = organization {
            payload.set("organization", organization);
        }
    }
    Ok(payload)
}

pub(crate) fn restore_notes_from_contact(contact: &Value) -> Result<Vec<Map<String, Value>>> {
    let Some(Value::Array(notes)) = contact.get("notes") else {
        return Ok(Vec::new());
    };
    let mut restored = Vec::new();
    for note in notes {
        if let Some(payload) = restore_note_payload(note) {
            restored.push(payload);
        }
    }
    Ok(restored)
}

pub(crate) fn restore_note_payload(note: &Value) -> Option<Map<String, Value>> {
    let mut payload = Map::new();
    let content = match note {
        Value::String(value) => value.trim().to_string(),
        Value::Object(object) => first_object_string(object, &["content", "text", "body", "note"])?,
        _ => return None,
    };
    if content.is_empty() {
        return None;
    }
    payload.set("content", content);
    if let Value::Object(object) = note
        && let Some(reminder_date) = first_object_string(
            object,
            &["reminder_date", "reminderDate", "reminder", "reminder_at"],
        )
    {
        payload.set("reminder_date", reminder_date);
    }
    Some(payload)
}

pub(crate) fn restore_contact_name(contact: &Value) -> Option<(String, Option<String>)> {
    let name = contact
        .get("name")
        .or_else(|| contact.get("displayName"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let mut parts = name.split_whitespace();
    let first = parts.next()?.to_string();
    let last = parts.collect::<Vec<_>>().join(" ");
    Some((first, (!last.is_empty()).then_some(last)))
}

pub(crate) fn restore_strings(value: Option<&Value>) -> Vec<String> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    let mut values = Vec::new();
    for item in items {
        let value = match item {
            Value::String(value) => Some(value.as_str()),
            Value::Object(object) => ["value", "url", "email", "phone", "phone_number"]
                .into_iter()
                .filter_map(|key| object.get(key).and_then(Value::as_str))
                .next(),
            _ => None,
        };
        if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
            values.push(value.to_string());
        }
    }
    dedupe_strings(values)
}

pub(crate) fn restore_linkedin(contact: &Value) -> Option<String> {
    restore_strings(contact.get("social_links"))
        .into_iter()
        .find(|value| normalize_linkedin_key(value).is_some())
}

pub(crate) fn restore_locations(value: Option<&Value>) -> Option<Value> {
    match value {
        Some(Value::Array(items)) if !items.is_empty() => Some(Value::Array(items.clone())),
        Some(Value::Object(object)) if !object.is_empty() => {
            Some(Value::Array(vec![Value::Object(object.clone())]))
        }
        _ => None,
    }
}

pub(crate) fn restore_current_work(contact: &Value) -> Option<(Option<String>, Option<String>)> {
    let Value::Array(items) = contact.get("work_history")? else {
        return None;
    };
    let work = items.iter().find_map(Value::as_object)?;
    let title = first_object_string(work, &["title", "position", "role"]);
    let organization = first_object_string(work, &["organization", "company", "name"])
        .or_else(|| nested_name_string(work.get("company")))
        .or_else(|| nested_name_string(work.get("organization")));
    if title.is_none() && organization.is_none() {
        None
    } else {
        Some((title, organization))
    }
}

pub(crate) fn restore_action_value(action: &RestoreAction) -> Value {
    json!({
        "source_id": action.source_id,
        "route": format!("/tools/v2{}", action.route),
        "payload": action.payload.clone(),
        "notes": action.notes.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

    fn temp_restore_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "meshx-restore-{label}-{}-{}",
            std::process::id(),
            NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn write_full_contacts_snapshot(dir: &Path, content: &str) -> Result<()> {
        write_full_contacts_snapshot_at(dir, "full-contacts.jsonl", content)
    }

    fn write_full_contacts_snapshot_at(dir: &Path, path: &str, content: &str) -> Result<()> {
        fs::create_dir_all(dir).into_diagnostic()?;
        let file_path = safe_snapshot_file_path(dir, path)?;
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).into_diagnostic()?;
        }
        fs::write(file_path, content).into_diagnostic()?;
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_string(&json!({
                "files": {
                    "full_contacts": {
                        "path": path,
                        "bytes": content.len() as u64,
                        "sha256": sha256_hex(content.as_bytes()),
                    }
                }
            }))
            .into_diagnostic()?,
        )
        .into_diagnostic()
    }

    #[test]
    fn snapshot_restore_contacts_reads_full_contacts_from_manifest_path() -> Result<()> {
        let dir = temp_restore_dir("manifest-path");
        write_full_contacts_snapshot_at(
            &dir,
            "data/full-contacts.jsonl",
            "{\"id\":7,\"name\":\"nested\"}\n",
        )?;

        let contacts = snapshot_restore_contacts(&dir, &[7])?;

        fs::remove_dir_all(&dir).ok();
        assert_eq!(contacts, vec![json!({"id":7,"name":"nested"})]);
        Ok(())
    }

    #[test]
    fn snapshot_restore_contacts_rejects_duplicate_full_contact_ids() -> Result<()> {
        let dir = temp_restore_dir("duplicate-full-contact-ids");
        write_full_contacts_snapshot(
            &dir,
            "{\"id\":1,\"name\":\"first\"}\n{\"id\":1,\"name\":\"second\"}\n",
        )?;

        let error = snapshot_restore_contacts(&dir, &[])
            .expect_err("duplicate full_contacts IDs should not be restored ambiguously");

        fs::remove_dir_all(&dir).ok();
        assert!(error.to_string().contains("duplicate record ID 1"));
        Ok(())
    }

    #[test]
    fn snapshot_restore_contacts_rejects_duplicate_selected_ids() -> Result<()> {
        let dir = temp_restore_dir("duplicate-selected-ids");
        write_full_contacts_snapshot(
            &dir,
            "{\"id\":1,\"name\":\"first\"}\n{\"id\":1,\"name\":\"second\"}\n",
        )?;

        let error = snapshot_restore_contacts(&dir, &[1])
            .expect_err("duplicate selected ID should not use the last matching row");

        fs::remove_dir_all(&dir).ok();
        assert!(error.to_string().contains("duplicate record ID 1"));
        Ok(())
    }

    #[test]
    fn restore_linkedin_returns_linkedin_link() {
        let contact = json!({
            "social_links": ["https://www.linkedin.com/in/ada/?trk=profile"],
        });

        assert_eq!(
            restore_linkedin(&contact),
            Some("https://www.linkedin.com/in/ada/?trk=profile".to_string())
        );
    }

    #[test]
    fn restore_linkedin_rejects_non_linkedin_hosts() {
        let contact = json!({
            "social_links": [
                "https://notlinkedin.com/in/ada",
                {"url": "https://example.com/linkedin/ada"}
            ],
        });

        assert_eq!(restore_linkedin(&contact), None);
    }
}
