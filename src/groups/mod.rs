mod read;
mod write;
use crate::prelude::*;
pub(crate) use read::*;
pub(crate) use write::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum GroupAuditSelector {
    Id(u64),
    Starred,
}

impl GroupAuditSelector {
    pub(crate) fn parse_with_flag(value: &str, flag: &str) -> Result<Self> {
        let normalized = value.trim().to_ascii_lowercase();
        if normalized == "starred" {
            return Ok(Self::Starred);
        }
        parse_contact_id(&normalized)
            .map(Self::Id)
            .into_diagnostic()
            .wrap_err_with(|| format!("{flag} must be a group ID or starred; got {value}"))
    }

    pub(crate) fn as_value(&self) -> Value {
        match self {
            Self::Id(id) => Value::Number(Number::from(*id)),
            Self::Starred => Value::String("starred".to_string()),
        }
    }

    pub(crate) fn matches_group(&self, group: &Value) -> bool {
        match self {
            Self::Id(id) => {
                record_id(group).and_then(|value| parse_contact_id(&value).ok()) == Some(*id)
            }
            Self::Starred => group_name(group)
                .and_then(|value| normalize_group_name_key(&value))
                .is_some_and(|value| value == "starred"),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GroupAuditOptions {
    pub(crate) snapshot_dir: Option<PathBuf>,
    pub(crate) query: Option<String>,
    pub(crate) group_ids: Vec<GroupAuditSelector>,
    pub(crate) member_counts: bool,
    pub(crate) issues_only: bool,
    pub(crate) concurrency: usize,
    pub(crate) top: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct GroupResolveOptions {
    pub(crate) query: Option<String>,
    pub(crate) group_ids: Vec<GroupAuditSelector>,
    pub(crate) all: bool,
    pub(crate) one: bool,
    pub(crate) candidate_limit: usize,
    pub(crate) member_counts: bool,
    pub(crate) concurrency: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct GroupOverlapOptions {
    pub(crate) query: Option<String>,
    pub(crate) group_ids: Vec<GroupAuditSelector>,
    pub(crate) all: bool,
    pub(crate) min_overlap: usize,
    pub(crate) min_jaccard: f64,
    pub(crate) top: Option<usize>,
    pub(crate) page_size: usize,
    pub(crate) concurrency: usize,
}

#[derive(Clone, Debug)]
pub(crate) enum GroupCompareTarget {
    Query(String),
    Selector(GroupAuditSelector),
}

#[derive(Clone, Debug)]
pub(crate) struct GroupCompareOptions {
    pub(crate) left: GroupCompareTarget,
    pub(crate) right: GroupCompareTarget,
    pub(crate) include_fields: Vec<String>,
    pub(crate) page_size: usize,
    pub(crate) concurrency: usize,
    pub(crate) id_limit: Option<usize>,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct GroupMembersOptions {
    pub(crate) query: Option<String>,
    pub(crate) group_ids: Vec<GroupAuditSelector>,
    pub(crate) all_groups: bool,
    pub(crate) include_fields: Vec<String>,
    pub(crate) limit_per_group: Option<usize>,
    pub(crate) page_size: usize,
    pub(crate) concurrency: usize,
    pub(crate) flat: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GroupSyncMode {
    Replace,
    AddOnly,
    RemoveOnly,
}

impl GroupSyncMode {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "replace" | "sync" => Ok(Self::Replace),
            "add-only" | "add" => Ok(Self::AddOnly),
            "remove-only" | "remove" => Ok(Self::RemoveOnly),
            other => err(format!(
                "--mode must be replace, add-only, or remove-only; got {other}"
            )),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Replace => "replace",
            Self::AddOnly => "add-only",
            Self::RemoveOnly => "remove-only",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GroupSyncOptions {
    pub(crate) group_id: u64,
    pub(crate) target_ids: Vec<u64>,
    pub(crate) search_payload: Option<Map<String, Value>>,
    pub(crate) mode: GroupSyncMode,
    pub(crate) page_size: usize,
    pub(crate) chunk_size: usize,
    pub(crate) concurrency: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct GroupSyncPlan {
    pub(crate) group_id: u64,
    pub(crate) group_name: String,
    pub(crate) group: Value,
    pub(crate) mode: GroupSyncMode,
    pub(crate) page_size: usize,
    pub(crate) chunk_size: usize,
    pub(crate) target_source: String,
    pub(crate) search_payload: Option<Map<String, Value>>,
    pub(crate) search_target_count: Option<usize>,
    pub(crate) current_ids: Vec<u64>,
    pub(crate) target_ids: Vec<u64>,
    pub(crate) add_ids: Vec<u64>,
    pub(crate) remove_ids: Vec<u64>,
    pub(crate) add_actions: Vec<GroupApplyAction>,
    pub(crate) remove_actions: Vec<GroupApplyAction>,
}

#[derive(Clone, Debug)]
pub(crate) struct GroupBulkMembershipOptions {
    pub(crate) kind: GroupApplyKind,
    pub(crate) command: &'static str,
    pub(crate) query: Option<String>,
    pub(crate) target_group_ids: Vec<GroupAuditSelector>,
    pub(crate) all_groups: bool,
    pub(crate) one: bool,
    pub(crate) group_limit: Option<usize>,
    pub(crate) target_ids: Vec<u64>,
    pub(crate) search_payload: Option<Map<String, Value>>,
    pub(crate) page_size: usize,
    pub(crate) target_limit: Option<usize>,
    pub(crate) chunk_size: usize,
    pub(crate) concurrency: usize,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct GroupBulkMembershipAction {
    pub(crate) row: usize,
    pub(crate) kind: GroupApplyKind,
    pub(crate) group_id: u64,
    pub(crate) group_name: String,
    pub(crate) route: &'static str,
    pub(crate) payload: Map<String, Value>,
}

#[derive(Clone, Debug)]
pub(crate) struct GroupBulkMembershipPlan {
    pub(crate) kind: GroupApplyKind,
    pub(crate) command: &'static str,
    pub(crate) group_source: String,
    pub(crate) target_source: String,
    pub(crate) query: Option<String>,
    pub(crate) target_group_ids: Vec<GroupAuditSelector>,
    pub(crate) selected_groups: Vec<Value>,
    pub(crate) skipped_special_groups: Vec<Value>,
    pub(crate) explicit_ids: Vec<u64>,
    pub(crate) search_payload: Option<Map<String, Value>>,
    pub(crate) search_exported_count: Option<usize>,
    pub(crate) search_match_count: Option<usize>,
    pub(crate) target_ids: Vec<u64>,
    pub(crate) actions: Vec<GroupBulkMembershipAction>,
    pub(crate) page_size: usize,
    pub(crate) target_limit: Option<usize>,
    pub(crate) group_limit: Option<usize>,
    pub(crate) chunk_size: usize,
    pub(crate) concurrency: usize,
}

impl GroupAuditOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let group_ids = group_audit_selectors_from_matches(matches)?;
        let snapshot_dir = matches.get_one::<String>("snapshot-dir").map(PathBuf::from);
        let member_counts = matches.get_flag("member-counts");
        if snapshot_dir.is_some() && member_counts {
            return err(
                "groups:audit --member-counts requires live me.sh; snapshots do not store group memberships",
            );
        }
        Ok(Self {
            snapshot_dir,
            query: matches.get_one::<String>("query").cloned(),
            group_ids,
            member_counts,
            issues_only: matches.get_flag("issues-only"),
            concurrency: contact_fetch_concurrency(matches, "concurrency")?,
            top: optional_positive_usize_from_matches(matches, "top")?
                .unwrap_or(SNAPSHOT_STATS_TOP_DEFAULT),
        })
    }
}

impl GroupResolveOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let query = matches
            .get_one::<String>("query")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let group_ids = group_audit_selectors_from_matches(matches)?;
        let all = matches.get_flag("all");
        if !all && query.is_none() && group_ids.is_empty() {
            return err("groups:resolve requires --query, --group-ids, or --all");
        }
        Ok(Self {
            query,
            group_ids,
            all,
            one: matches.get_flag("one"),
            candidate_limit: optional_usize_from_matches(matches, "candidate-limit")?
                .unwrap_or(CONTACT_RESOLVE_CANDIDATE_LIMIT_DEFAULT),
            member_counts: matches.get_flag("member-counts"),
            concurrency: contact_fetch_concurrency(matches, "concurrency")?,
        })
    }
}

impl GroupOverlapOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let query = matches
            .get_one::<String>("query")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let group_ids = group_audit_selectors_from_matches(matches)?;
        let all = matches.get_flag("all");
        if !all && query.is_none() && group_ids.is_empty() {
            return err("groups:overlap requires --query, --group-ids, or --all");
        }
        Ok(Self {
            query,
            group_ids,
            all,
            min_overlap: optional_nonnegative_usize_from_matches(matches, "min-overlap")?
                .unwrap_or(1),
            min_jaccard: optional_ratio_from_matches(matches, "min-jaccard")?.unwrap_or(0.0),
            top: optional_positive_usize_from_matches(matches, "top")?,
            page_size: optional_usize_from_matches(matches, "page-size")?
                .unwrap_or(SEARCH_LIMIT_MAX),
            concurrency: contact_fetch_concurrency(matches, "concurrency")?,
        })
    }
}

impl GroupCompareTarget {
    pub(crate) fn from_matches(
        matches: &ArgMatches,
        query_key: &str,
        selector_key: &str,
        selector_flag: &str,
    ) -> Result<Self> {
        let query = matches
            .get_one::<String>(query_key)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let selector = matches
            .get_one::<String>(selector_key)
            .map(|value| GroupAuditSelector::parse_with_flag(value, selector_flag))
            .transpose()?;
        match (query, selector) {
            (Some(query), None) => Ok(Self::Query(query)),
            (None, Some(selector)) => Ok(Self::Selector(selector)),
            (None, None) => err(format!(
                "groups:compare requires exactly one of --{selector_key} or --{query_key}"
            )),
            (Some(_), Some(_)) => err(format!(
                "groups:compare accepts --{selector_key} or --{query_key}, not both"
            )),
        }
    }

    pub(crate) fn as_value(&self) -> Value {
        match self {
            Self::Query(query) => json!({"query": query}),
            Self::Selector(selector) => json!({"group_id": selector.as_value()}),
        }
    }

    pub(crate) fn matches_group(&self, group: &Value) -> bool {
        match self {
            Self::Query(query) => group_name_matches_query(group, query),
            Self::Selector(selector) => selector.matches_group(group),
        }
    }
}

impl GroupCompareOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let all_ids = matches.get_flag("all-ids");
        let id_limit = optional_nonnegative_usize_from_matches(matches, "id-limit")?;
        if all_ids && id_limit.is_some() {
            return err("groups:compare accepts --all-ids or --id-limit, not both");
        }
        Ok(Self {
            left: GroupCompareTarget::from_matches(
                matches,
                "left-query",
                "left-group-id",
                "--left-group-id",
            )?,
            right: GroupCompareTarget::from_matches(
                matches,
                "right-query",
                "right-group-id",
                "--right-group-id",
            )?,
            include_fields: include_fields_from_matches(matches, "include-fields")?,
            page_size: optional_usize_from_matches(matches, "page-size")?
                .unwrap_or(SEARCH_LIMIT_MAX),
            concurrency: contact_fetch_concurrency(matches, "concurrency")?,
            id_limit: if all_ids {
                None
            } else {
                Some(id_limit.unwrap_or(GROUP_COMPARE_ID_LIMIT_DEFAULT))
            },
            flat: matches.get_flag("flat"),
        })
    }
}

impl GroupMembersOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let group_ids = group_audit_selectors_from_matches(matches)?;
        let query = matches.get_one::<String>("query").cloned();
        let all_groups = matches.get_flag("all-groups");
        if !all_groups && query.is_none() && group_ids.is_empty() {
            return err("groups:members requires --group-ids, --query, or --all-groups");
        }
        Ok(Self {
            query,
            group_ids,
            all_groups,
            include_fields: include_fields_from_matches(matches, "include-fields")?,
            limit_per_group: optional_nonnegative_usize_from_matches(matches, "limit-per-group")?,
            page_size: optional_usize_from_matches(matches, "page-size")?
                .unwrap_or(SEARCH_LIMIT_MAX),
            concurrency: contact_fetch_concurrency(matches, "concurrency")?,
            flat: matches.get_flag("flat"),
        })
    }
}

impl GroupSyncOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let group_id = matches
            .get_one::<String>("group-id")
            .expect("required by clap")
            .parse::<u64>()
            .into_diagnostic()
            .wrap_err("--group-id must be a positive integer")?;
        if group_id == 0 {
            return err("--group-id must be a positive integer");
        }
        let empty = matches.get_flag("empty");
        let target_ids = group_sync_target_ids_from_matches(matches)?;
        let from_search = matches.get_flag("from-search");
        let all_search = matches.get_flag("all-search");
        if all_search && !from_search {
            return err("groups:sync --all-search requires --from-search");
        }
        if empty && (!target_ids.is_empty() || from_search) {
            return err(
                "groups:sync accepts --empty, explicit desired IDs, or --from-search; choose one target source",
            );
        }
        let search_payload = if from_search {
            Some(group_sync_search_payload_from_matches(matches, all_search)?)
        } else {
            None
        };
        if !empty && target_ids.is_empty() && search_payload.is_none() {
            return err("groups:sync requires --contact-ids, --input, --from-search, or --empty");
        }
        let chunk_size = optional_usize_from_matches(matches, "chunk-size")?.unwrap_or(500);
        Ok(Self {
            group_id,
            target_ids,
            search_payload,
            mode: GroupSyncMode::parse(
                matches
                    .get_one::<String>("mode")
                    .map(String::as_str)
                    .unwrap_or("replace"),
            )?,
            page_size: optional_usize_from_matches(matches, "page-size")?
                .unwrap_or(SEARCH_LIMIT_MAX),
            chunk_size,
            concurrency: contact_fetch_concurrency(matches, "concurrency")?,
        })
    }
}

impl GroupBulkMembershipOptions {
    pub(crate) fn from_matches(
        matches: &ArgMatches,
        kind: GroupApplyKind,
        command: &'static str,
    ) -> Result<Self> {
        if !matches!(kind, GroupApplyKind::Add | GroupApplyKind::Remove) {
            return err("group bulk membership action must be add or remove");
        }
        let query = matches
            .get_one::<String>("query")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let target_group_ids =
            group_audit_selectors_from_matches_flag(matches, "target-group-ids")?;
        if target_group_ids
            .iter()
            .any(|selector| matches!(selector, GroupAuditSelector::Starred))
        {
            return err(format!("{command} cannot write the special Starred group"));
        }
        let all_groups = matches.get_flag("all-groups");
        if !all_groups && query.is_none() && target_group_ids.is_empty() {
            return err(format!(
                "{command} requires --target-group-ids, --query, or --all-groups"
            ));
        }

        let target_ids = target_contact_ids_from_matches(matches, "contact-ids")?;
        let from_search = matches.get_flag("from-search");
        let all_search = matches.get_flag("all-search");
        if all_search && !from_search {
            return err(format!("{command} --all-search requires --from-search"));
        }
        let search_payload = if from_search {
            Some(search_target_payload_from_matches(
                matches, all_search, command,
            )?)
        } else {
            None
        };
        if target_ids.is_empty() && search_payload.is_none() {
            return err(format!(
                "{command} requires --contact-ids, --input, or --from-search"
            ));
        }

        Ok(Self {
            kind,
            command,
            query,
            target_group_ids,
            all_groups,
            one: matches.get_flag("one"),
            group_limit: optional_positive_usize_from_matches(matches, "group-limit")?,
            target_ids,
            search_payload,
            page_size: optional_usize_from_matches(matches, "page-size")?
                .unwrap_or(SEARCH_LIMIT_MAX),
            target_limit: optional_positive_usize_from_matches(matches, "target-limit")?,
            chunk_size: optional_usize_from_matches(matches, "chunk-size")?.unwrap_or(500),
            concurrency: contact_fetch_concurrency(matches, "concurrency")?,
            flat: matches.get_flag("flat"),
        })
    }
}

pub(crate) fn group_id_key(key: &str) -> bool {
    key_matches(key, &["group-id", "groupId", "id", "group"])
}

pub(crate) fn group_title_key(key: &str) -> bool {
    key_matches(key, &["title", "name"])
}

pub(crate) fn group_add_key(key: &str) -> bool {
    key_matches(
        key,
        &[
            "add-contact-ids",
            "addContactIds",
            "add",
            "add-ids",
            "add-members",
        ],
    )
}

pub(crate) fn group_remove_key(key: &str) -> bool {
    key_matches(
        key,
        &[
            "remove-contact-ids",
            "removeContactIds",
            "remove",
            "remove-ids",
            "remove-members",
        ],
    )
}

pub(crate) fn group_contact_ids_key(key: &str) -> bool {
    key_matches(key, &["contact-ids", "contactIds", "contacts", "members"])
}

pub(crate) fn group_create_payload(row: &Map<String, Value>) -> Result<Map<String, Value>> {
    let title = row_string(row, &["title", "name"])
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| miette!("groups:apply create rows require title or name"))?;
    let mut payload = Map::new();
    payload.set("title", title);
    Ok(payload)
}

pub(crate) fn group_update_payload(row: &Map<String, Value>) -> Result<Map<String, Value>> {
    let group_id = group_id_from_row(row)?;
    let mut payload = Map::new();
    payload.insert(
        "group_id".to_string(),
        Value::Number(Number::from(group_id)),
    );
    if let Some(title) = row_string(row, &["title", "name"])
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        payload.set("title", title);
    }
    let add_ids = group_ids_from_row(row, true)?;
    if !add_ids.is_empty() {
        payload.set("add_contact_ids", json!(add_ids));
    }
    let remove_ids = group_ids_from_row(row, false)?;
    if !remove_ids.is_empty() {
        payload.set("remove_contact_ids", json!(remove_ids));
    }
    if payload.len() == 1 {
        return err(
            "groups:apply update rows require title, add_contact_ids, or remove_contact_ids",
        );
    }
    Ok(payload)
}

pub(crate) fn group_member_delta_payload(
    row: &Map<String, Value>,
    add: bool,
) -> Result<Map<String, Value>> {
    let group_id = group_id_from_row(row)?;
    let contact_ids = group_ids_from_row(row, add)?;
    if contact_ids.is_empty() {
        return err(if add {
            "groups:apply add rows require contact_ids or add_contact_ids"
        } else {
            "groups:apply remove rows require contact_ids or remove_contact_ids"
        });
    }
    let mut payload = Map::new();
    payload.insert(
        "group_id".to_string(),
        Value::Number(Number::from(group_id)),
    );
    payload.insert(
        if add {
            "add_contact_ids".to_string()
        } else {
            "remove_contact_ids".to_string()
        },
        json!(contact_ids),
    );
    Ok(payload)
}

pub(crate) fn group_id_from_row(row: &Map<String, Value>) -> Result<u64> {
    row_u64(row, &["group-id", "groupId", "id", "group"])?
        .ok_or_else(|| miette!("groups:apply rows require group_id or id"))
}

pub(crate) fn group_ids_from_row(row: &Map<String, Value>, add: bool) -> Result<Vec<u64>> {
    let primary = if add {
        row_u64_array(
            row,
            &[
                "add-contact-ids",
                "addContactIds",
                "add",
                "add-ids",
                "add-members",
            ],
        )?
    } else {
        row_u64_array(
            row,
            &[
                "remove-contact-ids",
                "removeContactIds",
                "remove",
                "remove-ids",
                "remove-members",
            ],
        )?
    };
    if !primary.is_empty() {
        return Ok(primary);
    }
    row_u64_array(row, &["contact-ids", "contactIds", "contacts", "members"])
}

pub(crate) fn group_duplicate_name_counts(groups: &[Value]) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    for group in groups {
        if let Some(name) = group_name(group).and_then(|value| normalize_group_name_key(&value)) {
            *counts.entry(name).or_default() += 1;
        }
    }
    counts
}

pub(crate) fn target_contact_ids_from_matches(
    matches: &ArgMatches,
    flag: &str,
) -> Result<Vec<u64>> {
    let mut values = collect_values(matches, flag);
    if let Some(path) = matches.get_one::<String>("input").map(String::as_str) {
        let text = fs::read_to_string(path)
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {path}"))?;
        values.extend(ids_from_text(&text));
    }
    Ok(dedupe_ids(parse_list_numbers(&values, flag)?))
}

pub(crate) fn group_is_special_starred(group: &Value) -> bool {
    group_name(group)
        .and_then(|value| normalize_group_name_key(&value))
        .is_some_and(|value| value == "starred")
}

pub(crate) fn group_name(group: &Value) -> Option<String> {
    ["name", "title"]
        .into_iter()
        .filter_map(|key| group.get(key))
        .map(cell_string)
        .find(|value| !value.trim().is_empty())
}

pub(crate) fn normalize_group_name_key(value: &str) -> Option<String> {
    let normalized = value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}

pub(crate) fn group_name_matches_query(group: &Value, query: &str) -> bool {
    let Some(query) = normalize_group_name_key(query) else {
        return true;
    };
    group_name(group)
        .and_then(|value| normalize_group_name_key(&value))
        .is_some_and(|name| name.contains(&query))
}

pub(crate) fn compare_groups_by_name_then_id(left: &Value, right: &Value) -> std::cmp::Ordering {
    group_name(left)
        .unwrap_or_default()
        .to_lowercase()
        .cmp(&group_name(right).unwrap_or_default().to_lowercase())
        .then_with(|| compare_record_ids(left, right))
}

fn compare_record_ids(left: &Value, right: &Value) -> std::cmp::Ordering {
    let left_id = record_id(left).unwrap_or_default();
    let right_id = record_id(right).unwrap_or_default();
    match (
        parse_contact_id(&left_id).ok(),
        parse_contact_id(&right_id).ok(),
    ) {
        (Some(left), Some(right)) => left.cmp(&right),
        _ => left_id.cmp(&right_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(values: &[(&str, Value)]) -> Map<String, Value> {
        values
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect()
    }

    #[test]
    fn group_name_prefers_first_non_blank_name_or_title() {
        assert_eq!(
            group_name(&json!({"name": "  ", "title": "Family"})),
            Some("Family".to_string())
        );
        assert_eq!(
            group_name(&json!({"name": "Friends", "title": "Ignored"})),
            Some("Friends".to_string())
        );
        assert_eq!(group_name(&json!({"name": "  "})), None);
    }

    #[test]
    fn normalize_group_name_key_collapses_whitespace_and_case() {
        assert_eq!(
            normalize_group_name_key("  Sales\tTeam\nWest  "),
            Some("sales team west".to_string())
        );
        assert_eq!(normalize_group_name_key(" \t "), None);
    }

    #[test]
    fn compare_groups_by_name_then_id_orders_numeric_id_ties_numerically() {
        let mut groups = [
            json!({"id": "10", "name": "Team"}),
            json!({"id": "2", "name": "team"}),
        ];

        groups.sort_by(compare_groups_by_name_then_id);

        assert_eq!(record_id(&groups[0]).as_deref(), Some("2"));
        assert_eq!(record_id(&groups[1]).as_deref(), Some("10"));
    }

    #[test]
    fn group_audit_selector_parses_ids_and_starred() -> Result<()> {
        assert_eq!(
            GroupAuditSelector::parse_with_flag(" 42 ", "--group-ids")?,
            GroupAuditSelector::Id(42)
        );
        assert_eq!(
            GroupAuditSelector::parse_with_flag("StArReD", "--group-ids")?,
            GroupAuditSelector::Starred
        );
        assert!(GroupAuditSelector::parse_with_flag("not-a-group", "--group-ids").is_err());
        Ok(())
    }

    #[test]
    fn group_audit_selector_matches_group_ids_and_starred_names() {
        assert!(GroupAuditSelector::Id(42).matches_group(&json!({"id": "42"})));
        assert!(GroupAuditSelector::Starred.matches_group(&json!({"name": " starred "})));
        assert!(!GroupAuditSelector::Starred.matches_group(&json!({"name": "starred contacts"})));
    }

    #[test]
    fn group_sync_mode_parses_aliases() -> Result<()> {
        assert_eq!(GroupSyncMode::parse("sync")?, GroupSyncMode::Replace);
        assert_eq!(GroupSyncMode::parse("ADD")?, GroupSyncMode::AddOnly);
        assert_eq!(
            GroupSyncMode::parse("remove-only")?,
            GroupSyncMode::RemoveOnly
        );
        assert_eq!(GroupSyncMode::RemoveOnly.as_str(), "remove-only");
        assert!(GroupSyncMode::parse("merge").is_err());
        Ok(())
    }

    #[test]
    fn group_create_payload_trims_title() -> Result<()> {
        let payload = group_create_payload(&row(&[("name", json!("  Family  "))]))?;

        assert_eq!(Value::Object(payload), json!({"title": "Family"}));
        assert!(group_create_payload(&row(&[("title", json!("  "))])).is_err());
        Ok(())
    }

    #[test]
    fn group_update_payload_builds_title_and_member_deltas() -> Result<()> {
        let payload = group_update_payload(&row(&[
            ("groupId", json!("7")),
            ("name", json!("  Friends  ")),
            ("add", json!("1, 2")),
            ("removeContactIds", json!(["3", 4])),
        ]))?;

        assert_eq!(
            Value::Object(payload),
            json!({
                "group_id": 7,
                "title": "Friends",
                "add_contact_ids": [1, 2],
                "remove_contact_ids": [3, 4],
            })
        );
        assert!(group_update_payload(&row(&[("group-id", json!(7))])).is_err());
        Ok(())
    }

    #[test]
    fn group_member_delta_payload_uses_specific_or_contact_ids() -> Result<()> {
        let add_payload = group_member_delta_payload(
            &row(&[("group-id", json!(9)), ("contact-ids", json!("[5, 6]"))]),
            true,
        )?;
        let remove_payload = group_member_delta_payload(
            &row(&[
                ("group-id", json!(9)),
                ("contact-ids", json!([5])),
                ("remove-members", json!([7, "8"])),
            ]),
            false,
        )?;

        assert_eq!(
            Value::Object(add_payload),
            json!({"group_id": 9, "add_contact_ids": [5, 6]})
        );
        assert_eq!(
            Value::Object(remove_payload),
            json!({"group_id": 9, "remove_contact_ids": [7, 8]})
        );
        assert!(group_member_delta_payload(&row(&[("group-id", json!(9))]), true).is_err());
        Ok(())
    }

    #[test]
    fn group_duplicate_name_counts_uses_normalized_names() {
        let counts = group_duplicate_name_counts(&[
            json!({"name": " Friends "}),
            json!({"title": "friends"}),
            json!({"name": "Family"}),
            json!({"name": "  "}),
        ]);

        assert_eq!(counts.get("friends"), Some(&2));
        assert_eq!(counts.get("family"), Some(&1));
        assert!(!counts.contains_key(""));
    }

    #[test]
    fn group_is_special_starred_requires_exact_normalized_name() {
        assert!(group_is_special_starred(&json!({"title": " STARRED "})));
        assert!(!group_is_special_starred(
            &json!({"name": "starred contacts"})
        ));
    }
}
