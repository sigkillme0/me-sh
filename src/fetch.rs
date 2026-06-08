use crate::prelude::*;

pub(crate) async fn fetch_contacts(
    runtime: &Runtime,
    ids: &[u64],
    concurrency: usize,
) -> Result<Vec<Value>> {
    let mut contacts = Vec::with_capacity(ids.len());
    fetch_contacts_each(runtime, ids, concurrency, |_, value| {
        contacts.push(value);
        Ok(())
    })
    .await?;
    Ok(contacts)
}

pub(crate) async fn fetch_contacts_each<F>(
    runtime: &Runtime,
    ids: &[u64],
    concurrency: usize,
    mut on_contact: F,
) -> Result<()>
where
    F: FnMut(u64, Value) -> Result<()>,
{
    if concurrency == 0 {
        return err("contact fetch concurrency must be greater than zero");
    }
    for chunk in ids.chunks(concurrency) {
        let mut handles = Vec::with_capacity(chunk.len());
        for id in chunk {
            let runtime = runtime.clone();
            let id = *id;
            handles.push((
                id,
                tokio::spawn(async move {
                    runtime
                        .call_tool(route::GET_CONTACT, json!({ "contact_id": id }))
                        .await
                }),
            ));
        }

        let mut chunk_values = Vec::with_capacity(handles.len());
        let mut first_error = None;
        for (id, handle) in handles {
            let result = handle
                .await
                .into_diagnostic()
                .wrap_err_with(|| format!("joining contact fetch task {id}"))
                .and_then(|result| result.wrap_err_with(|| format!("fetching contact {id}")));
            match result {
                Ok(value) => chunk_values.push((id, value)),
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        for (id, value) in chunk_values {
            on_contact(id, value)?;
        }
    }
    Ok(())
}

pub(crate) fn normalize_full_contact(id: u64, data: Value) -> Value {
    match data {
        Value::Object(object) if object_contact_id(&object) == Some(id) => Value::Object(object),
        other => json!({
            "id": id,
            "missing": other.is_null(),
            "contact": other,
        }),
    }
}

fn object_contact_id(object: &Map<String, Value>) -> Option<u64> {
    object.get("id").and_then(|value| match value {
        Value::Number(number) => number.as_u64(),
        Value::String(value) => parse_contact_id(value).ok(),
        _ => None,
    })
}

pub(crate) async fn live_contact_records(
    runtime: &Runtime,
    page_size: usize,
    keep_values: bool,
) -> Result<(BTreeMap<String, String>, BTreeMap<String, Value>, usize)> {
    let mut records = BTreeMap::new();
    let mut values = BTreeMap::new();
    let count = export_all_contacts_each(runtime, Map::new(), page_size, |row| {
        insert_record_value(
            "live search contact",
            row,
            &mut records,
            keep_values.then_some(&mut values),
        )
    })
    .await?;
    Ok((records, values, count))
}

pub(crate) async fn live_group_records(
    runtime: &Runtime,
    keep_values: bool,
) -> Result<(BTreeMap<String, String>, BTreeMap<String, Value>, usize)> {
    let data = runtime.call_tool(route::GET_GROUPS, json!({})).await?;
    let mut records = BTreeMap::new();
    let mut values = BTreeMap::new();
    for row in snapshot_group_rows_from_response(&data)? {
        insert_record_value(
            "live group",
            row,
            &mut records,
            keep_values.then_some(&mut values),
        )?;
    }
    let count = records.len();
    Ok((records, values, count))
}

pub(crate) async fn live_full_contact_records(
    runtime: &Runtime,
    ids: &[u64],
    concurrency: usize,
    keep_values: bool,
) -> Result<(BTreeMap<String, String>, BTreeMap<String, Value>)> {
    let mut records = BTreeMap::new();
    let mut values = BTreeMap::new();
    fetch_contacts_each(runtime, ids, concurrency, |id, contact| {
        let row = normalize_full_contact(id, contact);
        insert_record_value(
            "live full contact",
            row,
            &mut records,
            keep_values.then_some(&mut values),
        )
    })
    .await?;
    Ok((records, values))
}

pub(crate) fn insert_record_value(
    label: &str,
    value: Value,
    records: &mut BTreeMap<String, String>,
    values: Option<&mut BTreeMap<String, Value>>,
) -> Result<()> {
    let id = record_id(&value).ok_or_else(|| miette!("{label} did not include top-level id"))?;
    if records.contains_key(&id) {
        return err(format!("{label} contains duplicate record ID {id}"));
    }
    let hash = record_hash(&value)?;
    records.insert(id.clone(), hash);
    if let Some(values) = values {
        values.insert(id, value);
    }
    Ok(())
}

pub(crate) fn filter_records_by_ids<T: Clone>(
    records: &BTreeMap<String, T>,
    ids: &[u64],
) -> BTreeMap<String, T> {
    ids.iter()
        .filter_map(|id| {
            let key = id.to_string();
            records.get(&key).cloned().map(|value| (key, value))
        })
        .collect()
}

pub(crate) fn append_exclude_contact_ids(
    payload: &mut Map<String, Value>,
    ids: &[u64],
) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let mut all = exclude_contact_ids_from_payload(payload.get("exclude_contact_ids"))?;
    all.extend_from_slice(ids);
    all = dedupe_ids(all);
    payload.set("exclude_contact_ids", json!(all));
    Ok(())
}

pub(crate) fn exclude_contact_ids_from_payload(value: Option<&Value>) -> Result<Vec<u64>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    match value {
        Value::Array(items) => items.iter().map(parse_u64_value).collect(),
        other => parse_u64_value(other).map(|id| vec![id]),
    }
}

pub(crate) fn total_from_search(data: &Value) -> Value {
    data.get("total")
        .cloned()
        .or_else(|| data.get("count").cloned())
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_full_contact_keeps_objects_with_ids() {
        let contact = json!({"id": 7, "name": "Ada"});

        assert_eq!(normalize_full_contact(7, contact.clone()), contact);
        assert_eq!(
            normalize_full_contact(42, json!({"id": "42", "name": "Ada"})),
            json!({"id": "42", "name": "Ada"})
        );
    }

    #[test]
    fn normalize_full_contact_wraps_missing_or_null_contacts() {
        assert_eq!(
            normalize_full_contact(42, Value::Null),
            json!({"id": 42, "missing": true, "contact": null})
        );
        assert_eq!(
            normalize_full_contact(42, json!({"name": "Ada"})),
            json!({"id": 42, "missing": false, "contact": {"name": "Ada"}})
        );
    }

    #[test]
    fn normalize_full_contact_wraps_contacts_with_unusable_ids() {
        assert_eq!(
            normalize_full_contact(42, json!({"id": null, "name": "Ada"})),
            json!({"id": 42, "missing": false, "contact": {"id": null, "name": "Ada"}})
        );
        assert_eq!(
            normalize_full_contact(42, json!({"id": true, "name": "Ada"})),
            json!({"id": 42, "missing": false, "contact": {"id": true, "name": "Ada"}})
        );
    }

    #[test]
    fn normalize_full_contact_wraps_contacts_for_other_ids() {
        assert_eq!(
            normalize_full_contact(42, json!({"id": 7, "name": "Grace"})),
            json!({"id": 42, "missing": false, "contact": {"id": 7, "name": "Grace"}})
        );
    }

    #[test]
    fn insert_record_value_stores_hash_and_optional_value() -> Result<()> {
        let value = json!({"id": "c-1", "name": "Ada"});
        let expected_hash = record_hash(&value)?;
        let mut records = BTreeMap::new();
        let mut values = BTreeMap::new();

        insert_record_value("contact", value.clone(), &mut records, Some(&mut values))?;

        assert_eq!(records.get("c-1"), Some(&expected_hash));
        assert_eq!(values.get("c-1"), Some(&value));
        Ok(())
    }

    #[test]
    fn insert_record_value_requires_top_level_id() {
        let error = insert_record_value(
            "contact",
            json!({"name": "Ada"}),
            &mut BTreeMap::new(),
            None,
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("contact did not include top-level id"));
    }

    #[test]
    fn insert_record_value_rejects_duplicate_ids() -> Result<()> {
        let first = json!({"id": 1, "name": "first"});
        let second = json!({"id": 1, "name": "second"});
        let mut records = BTreeMap::new();
        let mut values = BTreeMap::new();

        insert_record_value(
            "live contact",
            first.clone(),
            &mut records,
            Some(&mut values),
        )?;
        let error = insert_record_value("live contact", second, &mut records, Some(&mut values))
            .expect_err("duplicate live IDs should not be collapsed");

        assert!(error.to_string().contains("duplicate record ID 1"));
        assert_eq!(records.get("1"), Some(&record_hash(&first)?));
        assert_eq!(values.get("1"), Some(&first));
        Ok(())
    }

    #[test]
    fn filter_records_by_ids_keeps_matching_numeric_string_keys() {
        let records = BTreeMap::from([
            ("1".to_string(), "one".to_string()),
            ("2".to_string(), "two".to_string()),
            ("3".to_string(), "three".to_string()),
        ]);

        assert_eq!(
            filter_records_by_ids(&records, &[3, 99, 1]),
            BTreeMap::from([
                ("1".to_string(), "one".to_string()),
                ("3".to_string(), "three".to_string())
            ])
        );
    }

    #[test]
    fn append_exclude_contact_ids_merges_and_dedupes_ids() -> Result<()> {
        let mut payload = Map::from_iter([("exclude_contact_ids".to_string(), json!([2, 1]))]);

        append_exclude_contact_ids(&mut payload, &[1, 3, 2])?;

        assert_eq!(payload.get("exclude_contact_ids"), Some(&json!([2, 1, 3])));
        Ok(())
    }

    #[test]
    fn append_exclude_contact_ids_leaves_payload_unchanged_for_empty_input() -> Result<()> {
        let mut payload = Map::from_iter([("exclude_contact_ids".to_string(), json!("bad"))]);

        append_exclude_contact_ids(&mut payload, &[])?;

        assert_eq!(payload.get("exclude_contact_ids"), Some(&json!("bad")));
        Ok(())
    }

    #[test]
    fn exclude_contact_ids_from_payload_accepts_scalar_and_array_values() -> Result<()> {
        assert_eq!(exclude_contact_ids_from_payload(None)?, Vec::<u64>::new());
        assert_eq!(exclude_contact_ids_from_payload(Some(&json!(7)))?, vec![7]);
        assert_eq!(
            exclude_contact_ids_from_payload(Some(&json!(["7", 8])))?,
            vec![7, 8]
        );
        Ok(())
    }

    #[test]
    fn exclude_contact_ids_from_payload_rejects_invalid_values() {
        let error = exclude_contact_ids_from_payload(Some(&json!([1, null])))
            .unwrap_err()
            .to_string();

        assert!(error.contains("contact ID values must be positive integers"));
    }

    #[test]
    fn total_from_search_prefers_total_then_count() {
        assert_eq!(
            total_from_search(&json!({"total": 5, "count": 3})),
            json!(5)
        );
        assert_eq!(total_from_search(&json!({"count": 3})), json!(3));
        assert_eq!(total_from_search(&json!({"items": []})), Value::Null);
    }
}
