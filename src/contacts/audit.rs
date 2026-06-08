use crate::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum DedupeSignal {
    Email,
    Phone,
    Linkedin,
    Name,
}

impl DedupeSignal {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        Self::parse_with_flag(value, "by")
    }

    pub(crate) fn parse_with_flag(value: &str, flag: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "email" | "emails" => Ok(Self::Email),
            "phone" | "phones" | "phone-number" | "phone_numbers" => Ok(Self::Phone),
            "linkedin" | "linkedin-url" | "linkedin_url" => Ok(Self::Linkedin),
            "name" | "names" => Ok(Self::Name),
            other => err(format!(
                "--{flag} must contain email, phone, linkedin, or name; got {other}"
            )),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Phone => "phone",
            Self::Linkedin => "linkedin",
            Self::Name => "name",
        }
    }

    pub(crate) fn confidence(self) -> u64 {
        match self {
            Self::Email => 100,
            Self::Phone => 95,
            Self::Linkedin => 95,
            Self::Name => 60,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ContactQualityOptions {
    pub(crate) issue_limit: usize,
    pub(crate) top: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct ContactFacetsOptions {
    pub(crate) facets: Vec<ContactFacetKind>,
    pub(crate) top: usize,
    pub(crate) min_count: usize,
    pub(crate) sample_limit: usize,
    pub(crate) include_empty: bool,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ContactPivotOptions {
    pub(crate) row_facet: ContactFacetKind,
    pub(crate) col_facet: ContactFacetKind,
    pub(crate) top_rows: usize,
    pub(crate) top_cols: usize,
    pub(crate) min_count: usize,
    pub(crate) sample_limit: usize,
    pub(crate) include_empty: bool,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ContactOverviewOptions {
    pub(crate) facets: Vec<ContactFacetKind>,
    pub(crate) dedupe_signals: Vec<DedupeSignal>,
    pub(crate) min_confidence: u64,
    pub(crate) candidate_limit: usize,
    pub(crate) issue_limit: usize,
    pub(crate) top: usize,
    pub(crate) min_count: usize,
    pub(crate) sample_limit: usize,
    pub(crate) include_empty: bool,
    pub(crate) flat: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum ContactFacetKind {
    EmailDomain,
    Company,
    Title,
    Location,
    Integration,
    Channel,
    NameInitial,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ContactFacetBucket {
    pub(crate) value: String,
    pub(crate) count: u64,
    pub(crate) sample_contacts: Vec<Value>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ContactPivotCell {
    pub(crate) count: u64,
    pub(crate) sample_contacts: Vec<Value>,
}

impl ContactQualityOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let issue_limit =
            optional_nonnegative_usize_from_matches(matches, "issue-limit")?.unwrap_or(50);
        if issue_limit > SEARCH_LIMIT_MAX {
            return err(format!("--issue-limit must be at most {SEARCH_LIMIT_MAX}"));
        }
        Ok(Self {
            issue_limit,
            top: optional_positive_usize_from_matches(matches, "top")?
                .unwrap_or(SNAPSHOT_STATS_TOP_DEFAULT),
        })
    }
}

impl ContactFacetsOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let facets = contact_facets_from_matches(matches)?;
        let sample_limit =
            optional_nonnegative_usize_from_matches(matches, "sample-limit")?.unwrap_or(5);
        if sample_limit > SEARCH_LIMIT_MAX {
            return err(format!("--sample-limit must be at most {SEARCH_LIMIT_MAX}"));
        }
        Ok(Self {
            facets,
            top: optional_positive_usize_from_matches(matches, "top")?
                .unwrap_or(SNAPSHOT_STATS_TOP_DEFAULT),
            min_count: optional_positive_usize_from_matches(matches, "min-count")?.unwrap_or(1),
            sample_limit,
            include_empty: matches.get_flag("include-empty"),
            flat: matches.get_flag("flat"),
        })
    }
}

impl ContactPivotOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let row_facet = ContactFacetKind::parse_with_flag(
            matches
                .get_one::<String>("rows")
                .map(String::as_str)
                .unwrap_or("integration"),
            "rows",
        )?;
        let col_facet = ContactFacetKind::parse_with_flag(
            matches
                .get_one::<String>("cols")
                .map(String::as_str)
                .unwrap_or("channel"),
            "cols",
        )?;
        let sample_limit =
            optional_nonnegative_usize_from_matches(matches, "sample-limit")?.unwrap_or(3);
        if sample_limit > SEARCH_LIMIT_MAX {
            return err(format!("--sample-limit must be at most {SEARCH_LIMIT_MAX}"));
        }
        let top_rows = optional_positive_usize_from_matches(matches, "top-rows")?.unwrap_or(10);
        if top_rows > SEARCH_LIMIT_MAX {
            return err(format!("--top-rows must be at most {SEARCH_LIMIT_MAX}"));
        }
        let top_cols = optional_positive_usize_from_matches(matches, "top-cols")?.unwrap_or(10);
        if top_cols > SEARCH_LIMIT_MAX {
            return err(format!("--top-cols must be at most {SEARCH_LIMIT_MAX}"));
        }
        Ok(Self {
            row_facet,
            col_facet,
            top_rows,
            top_cols,
            min_count: optional_positive_usize_from_matches(matches, "min-count")?.unwrap_or(1),
            sample_limit,
            include_empty: matches.get_flag("include-empty"),
            flat: matches.get_flag("flat"),
        })
    }
}

impl ContactOverviewOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let issue_limit =
            optional_nonnegative_usize_from_matches(matches, "issue-limit")?.unwrap_or(20);
        if issue_limit > SEARCH_LIMIT_MAX {
            return err(format!("--issue-limit must be at most {SEARCH_LIMIT_MAX}"));
        }
        let sample_limit =
            optional_nonnegative_usize_from_matches(matches, "sample-limit")?.unwrap_or(3);
        if sample_limit > SEARCH_LIMIT_MAX {
            return err(format!("--sample-limit must be at most {SEARCH_LIMIT_MAX}"));
        }
        let candidate_limit =
            optional_positive_usize_from_matches(matches, "candidate-limit")?.unwrap_or(10);
        if candidate_limit > SEARCH_LIMIT_MAX {
            return err(format!(
                "--candidate-limit must be at most {SEARCH_LIMIT_MAX}"
            ));
        }
        Ok(Self {
            facets: contact_overview_facets_from_matches(matches)?,
            dedupe_signals: contact_overview_dedupe_signals_from_matches(matches)?,
            min_confidence: min_confidence_from_matches(matches)?,
            candidate_limit,
            issue_limit,
            top: optional_positive_usize_from_matches(matches, "top")?
                .unwrap_or(SNAPSHOT_STATS_TOP_DEFAULT),
            min_count: optional_positive_usize_from_matches(matches, "min-count")?.unwrap_or(1),
            sample_limit,
            include_empty: matches.get_flag("include-empty"),
            flat: matches.get_flag("flat"),
        })
    }

    pub(crate) fn quality_options(&self) -> ContactQualityOptions {
        ContactQualityOptions {
            issue_limit: self.issue_limit,
            top: self.top,
        }
    }

    pub(crate) fn facet_options(&self) -> ContactFacetsOptions {
        ContactFacetsOptions {
            facets: self.facets.clone(),
            top: self.top,
            min_count: self.min_count,
            sample_limit: self.sample_limit,
            include_empty: self.include_empty,
            flat: false,
        }
    }
}

impl ContactFacetKind {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        Self::parse_with_flag(value, "by")
    }

    pub(crate) fn parse_with_flag(value: &str, flag: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "email-domain" | "email-domains" | "domain" | "domains" => Ok(Self::EmailDomain),
            "company" | "companies" | "organization" | "org" => Ok(Self::Company),
            "title" | "titles" | "position" | "positions" | "role" | "roles" => Ok(Self::Title),
            "location" | "locations" | "place" | "places" => Ok(Self::Location),
            "integration" | "integrations" | "source" | "sources" => Ok(Self::Integration),
            "channel" | "channels" | "contact-channel" | "contact-channels" => Ok(Self::Channel),
            "name-initial" | "initial" | "initials" => Ok(Self::NameInitial),
            other => err(format!(
                "--{flag} must contain email-domain, company, title, location, integration, channel, or name-initial; got {other}"
            )),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::EmailDomain => "email-domain",
            Self::Company => "company",
            Self::Title => "title",
            Self::Location => "location",
            Self::Integration => "integration",
            Self::Channel => "channel",
            Self::NameInitial => "name-initial",
        }
    }
}

pub(crate) fn dedupe_signals_from_matches(matches: &ArgMatches) -> Result<Vec<DedupeSignal>> {
    parsed_unique_flag_values(
        matches,
        "by",
        &["email", "phone", "linkedin", "name"],
        DedupeSignal::parse,
        "contacts:dedupe needs at least one --by signal",
    )
}

pub(crate) fn contact_facets_from_matches(matches: &ArgMatches) -> Result<Vec<ContactFacetKind>> {
    parsed_unique_flag_values(
        matches,
        "by",
        &["email-domain", "company", "title", "location"],
        ContactFacetKind::parse,
        "contacts:facets needs at least one --by facet",
    )
}

pub(crate) fn contact_overview_facets_from_matches(
    matches: &ArgMatches,
) -> Result<Vec<ContactFacetKind>> {
    parsed_unique_flag_values(
        matches,
        "facets",
        &[
            "email-domain",
            "company",
            "title",
            "location",
            "integration",
            "channel",
        ],
        |value| ContactFacetKind::parse_with_flag(value, "facets"),
        "contacts:overview needs at least one --facets value",
    )
}

pub(crate) fn contact_map_facets_from_matches(
    matches: &ArgMatches,
) -> Result<Vec<ContactFacetKind>> {
    parsed_unique_flag_values(
        matches,
        "by",
        &["email-domain", "company", "integration", "channel"],
        |value| ContactFacetKind::parse_with_flag(value, "by"),
        "contacts:map needs at least one --by facet",
    )
}

pub(crate) fn contact_map_include_fields_for_facet(
    facet: ContactFacetKind,
) -> &'static [&'static str] {
    match facet {
        ContactFacetKind::EmailDomain => &["emails"],
        ContactFacetKind::Company | ContactFacetKind::Title => &["work_history"],
        ContactFacetKind::Location => &["location"],
        ContactFacetKind::Integration => &["integrations"],
        ContactFacetKind::Channel => &["emails", "phone_numbers", "social_links"],
        ContactFacetKind::NameInitial => &[],
    }
}

pub(crate) fn contact_overview_dedupe_signals_from_matches(
    matches: &ArgMatches,
) -> Result<Vec<DedupeSignal>> {
    parsed_unique_flag_values(
        matches,
        "dedupe-by",
        &["email", "phone", "linkedin", "name"],
        |value| DedupeSignal::parse_with_flag(value, "dedupe-by"),
        "contacts:overview needs at least one --dedupe-by signal",
    )
}

fn parsed_unique_flag_values<T>(
    matches: &ArgMatches,
    flag: &str,
    defaults: &[&str],
    mut parse: impl FnMut(&str) -> Result<T>,
    empty_error: &str,
) -> Result<Vec<T>>
where
    T: Copy + Ord,
{
    let raw = collect_values(matches, flag);
    let values = if raw.is_empty() {
        defaults.iter().map(|value| (*value).to_string()).collect()
    } else {
        split_list_values(&raw)
    };
    let mut seen = BTreeSet::new();
    let mut output = Vec::new();
    for value in values {
        let parsed = parse(&value)?;
        if seen.insert(parsed) {
            output.push(parsed);
        }
    }
    if output.is_empty() {
        return err(empty_error);
    }
    Ok(output)
}

pub(crate) fn dedupe_source_label(input: Option<&str>, snapshot_dir: Option<&str>) -> &'static str {
    if input.is_some() {
        "input"
    } else if snapshot_dir.is_some() {
        "snapshot"
    } else {
        "live"
    }
}

pub(crate) fn dedupe_dry_run_plan(
    input: Option<&str>,
    snapshot_dir: Option<&str>,
    live_payload: Option<&Map<String, Value>>,
) -> Value {
    if let Some(input) = input {
        json!([
            {"local_file": input, "purpose": "read contacts from local file"},
            {"local": "dedupe", "purpose": "normalize selected signals and group contacts with matching keys"}
        ])
    } else if let Some(snapshot_dir) = snapshot_dir {
        json!([
            {"local_file": format!("{snapshot_dir}/manifest.json"), "purpose": "verify snapshot hashes"},
            {"local_file": "full-contacts.jsonl or contacts.jsonl", "purpose": "read snapshot contacts"},
            {"local": "dedupe", "purpose": "normalize selected signals and group contacts with matching keys"}
        ])
    } else {
        json!([
            {"route": "/tools/v2/search", "payload": live_search_count_dry_run_payload(live_payload), "purpose": "count matching contacts"},
            {"route": "/tools/v2/search", "payload": live_search_page_dry_run_payload(live_payload), "page_size": SEARCH_LIMIT_MAX, "purpose": "fetch matching search rows"},
            {"local": "dedupe", "purpose": "normalize selected signals and group contacts with matching keys"}
        ])
    }
}

pub(crate) fn contact_quality_dry_run_plan(
    input: Option<&str>,
    snapshot_dir: Option<&str>,
    live_payload: Option<&Map<String, Value>>,
) -> Value {
    if let Some(input) = input {
        json!([
            {"local_file": input, "purpose": "read contacts from local file"},
            {"local": "quality", "purpose": "score completeness, invalid fields, duplicate signals, and actionable contact issues"}
        ])
    } else if let Some(snapshot_dir) = snapshot_dir {
        json!([
            {"local_file": format!("{snapshot_dir}/manifest.json"), "purpose": "verify snapshot hashes"},
            {"local_file": "full-contacts.jsonl or contacts.jsonl", "purpose": "read snapshot contacts"},
            {"local": "quality", "purpose": "score completeness, invalid fields, duplicate signals, and actionable contact issues"}
        ])
    } else {
        json!([
            {"route": "/tools/v2/search", "payload": live_search_count_dry_run_payload(live_payload), "purpose": "count matching contacts"},
            {"route": "/tools/v2/search", "payload": live_search_page_dry_run_payload(live_payload), "page_size": SEARCH_LIMIT_MAX, "purpose": "fetch matching search rows without writes"},
            {"local": "quality", "purpose": "score completeness, invalid fields, duplicate signals, and actionable contact issues"}
        ])
    }
}

pub(crate) fn contact_facets_dry_run_plan(
    input: Option<&str>,
    snapshot_dir: Option<&str>,
    live_payload: Option<&Map<String, Value>>,
) -> Value {
    if let Some(input) = input {
        json!([
            {"local_file": input, "purpose": "read contacts from local file"},
            {"local": "facets", "purpose": "aggregate selected contact facets without writes"}
        ])
    } else if let Some(snapshot_dir) = snapshot_dir {
        json!([
            {"local_file": format!("{snapshot_dir}/manifest.json"), "purpose": "verify snapshot hashes"},
            {"local_file": "full-contacts.jsonl or contacts.jsonl", "purpose": "read snapshot contacts"},
            {"local": "facets", "purpose": "aggregate selected contact facets without writes"}
        ])
    } else {
        json!([
            {"route": "/tools/v2/search", "payload": live_search_count_dry_run_payload(live_payload), "purpose": "count matching contacts"},
            {"route": "/tools/v2/search", "payload": live_search_page_dry_run_payload(live_payload), "page_size": SEARCH_LIMIT_MAX, "purpose": "fetch matching search rows without writes"},
            {"local": "facets", "purpose": "aggregate selected contact facets without writes"}
        ])
    }
}

pub(crate) fn contact_pivot_dry_run_plan(
    input: Option<&str>,
    snapshot_dir: Option<&str>,
    live_payload: Option<&Map<String, Value>>,
    options: &ContactPivotOptions,
) -> Value {
    let load = if let Some(input) = input {
        json!([
            {"local_file": input, "purpose": "read contacts from local file"}
        ])
    } else if let Some(snapshot_dir) = snapshot_dir {
        json!([
            {"local_file": format!("{snapshot_dir}/manifest.json"), "purpose": "verify snapshot hashes"},
            {"local_file": "full-contacts.jsonl or contacts.jsonl", "purpose": "read snapshot contacts"}
        ])
    } else {
        json!([
            {"route": "/tools/v2/search", "payload": live_search_count_dry_run_payload(live_payload), "purpose": "count matching contacts"},
            {"route": "/tools/v2/search", "payload": live_search_page_dry_run_payload(live_payload), "page_size": SEARCH_LIMIT_MAX, "purpose": "fetch matching search rows without writes"}
        ])
    };
    json!({
        "source": dedupe_source_label(input, snapshot_dir),
        "filters": live_payload.as_ref().map(|payload| Value::Object((*payload).clone())).unwrap_or(Value::Null),
        "options": contact_pivot_options_value(options),
        "plan": {
            "load": load,
            "local": [
                {"name": "pivot", "purpose": "cross-tab selected contact facets without writes"}
            ]
        },
    })
}

pub(crate) fn contact_overview_dry_run_plan(
    input: Option<&str>,
    snapshot_dir: Option<&str>,
    live_payload: Option<&Map<String, Value>>,
    options: &ContactOverviewOptions,
) -> Value {
    let load = if let Some(input) = input {
        json!([
            {"local_file": input, "purpose": "read contacts from local file"}
        ])
    } else if let Some(snapshot_dir) = snapshot_dir {
        json!([
            {"local_file": format!("{snapshot_dir}/manifest.json"), "purpose": "verify snapshot hashes"},
            {"local_file": "full-contacts.jsonl or contacts.jsonl", "purpose": "read snapshot contacts"}
        ])
    } else {
        json!([
            {"route": "/tools/v2/search", "payload": live_search_count_dry_run_payload(live_payload), "purpose": "count matching contacts"},
            {"route": "/tools/v2/search", "payload": live_search_page_dry_run_payload(live_payload), "page_size": SEARCH_LIMIT_MAX, "purpose": "fetch matching search rows without writes"}
        ])
    };
    json!({
        "source": dedupe_source_label(input, snapshot_dir),
        "filters": live_payload.as_ref().map(|payload| Value::Object((*payload).clone())).unwrap_or(Value::Null),
        "options": contact_overview_options_value(options),
        "plan": {
            "load": load,
            "local": [
                {"name": "quality", "purpose": "score completeness, invalid fields, duplicate signals, and actionable contact issues"},
                {"name": "dedupe", "purpose": "normalize selected signals and return likely duplicate candidate groups"},
                {"name": "facets", "purpose": "aggregate selected contact facet buckets"}
            ]
        },
    })
}

pub(crate) async fn contacts_for_dedupe_live(
    runtime: &Runtime,
    mut payload: Map<String, Value>,
) -> Result<(Value, Vec<Value>)> {
    payload.remove("limit");
    let data = export_all_contacts(runtime, payload.clone(), SEARCH_LIMIT_MAX).await?;
    let contacts = rows_from_value(&data)
        .into_iter()
        .map(Value::Object)
        .collect::<Vec<_>>();
    Ok((
        json!({
            "type": "live",
            "matched_count": contacts.len(),
            "analyzed_count": contacts.len(),
            "filters": Value::Object(payload),
            "pagination": "exclude_contact_ids",
        }),
        contacts,
    ))
}

pub(crate) fn contact_quality_report(
    source: Value,
    contacts: Vec<Value>,
    options: ContactQualityOptions,
) -> Result<Value> {
    let duplicates = contact_quality_duplicates(&contacts, options.top);
    let duplicate_issues = duplicates
        .get("by_contact")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut field_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut issue_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut warning_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut email_domains = BTreeMap::new();
    let mut scored = Vec::new();
    let mut score_total = 0_u64;

    for (index, contact) in contacts.iter().enumerate() {
        let duplicate_labels = duplicate_issues
            .get(&index.to_string())
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let record = contact_quality_record(contact, duplicate_labels)?;
        let score = record
            .get("score")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        score_total += score;
        if let Some(fields) = record.get("fields").and_then(Value::as_object) {
            for (field, present) in fields {
                if present.as_bool().unwrap_or(false) {
                    *field_counts.entry(field.clone()).or_default() += 1;
                }
            }
        }
        for issue in record
            .get("issues")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            *issue_counts.entry(issue.to_string()).or_default() += 1;
        }
        for warning in record
            .get("warnings")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            *warning_counts.entry(warning.to_string()).or_default() += 1;
        }
        for domain in contact_quality_email_domains(contact) {
            *email_domains.entry(domain).or_default() += 1;
        }
        scored.push(record);
    }

    scored.sort_by(|left, right| {
        let left_score = left
            .get("score")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let right_score = right
            .get("score")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let left_issues = left
            .get("issue_count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let right_issues = right
            .get("issue_count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        left_score
            .cmp(&right_score)
            .then_with(|| right_issues.cmp(&left_issues))
            .then_with(|| {
                left.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(right.get("id").and_then(Value::as_str).unwrap_or_default())
            })
    });

    let issue_contact_count = scored
        .iter()
        .filter(|record| {
            record
                .get("issue_count")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                > 0
        })
        .count();
    let warning_contact_count = scored
        .iter()
        .filter(|record| {
            record
                .get("warning_count")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                > 0
        })
        .count();
    let average_score = if contacts.is_empty() {
        0.0
    } else {
        (score_total as f64) / (contacts.len() as f64)
    };
    let issue_rows = scored
        .into_iter()
        .filter(|record| {
            record
                .get("issue_count")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                > 0
                || record
                    .get("warning_count")
                    .and_then(Value::as_u64)
                    .unwrap_or_default()
                    > 0
        })
        .take(options.issue_limit)
        .collect::<Vec<_>>();

    Ok(json!({
        "source": source,
        "analyzed_count": contacts.len(),
        "summary": {
            "average_score": average_score,
            "issue_contact_count": issue_contact_count,
            "warning_contact_count": warning_contact_count,
            "total_issues": issue_counts.values().sum::<u64>(),
            "total_warnings": warning_counts.values().sum::<u64>(),
            "field_coverage": contact_quality_coverage(&field_counts, contacts.len() as u64),
            "top_issues": top_count_entries(&issue_counts, options.top),
            "top_warnings": top_count_entries(&warning_counts, options.top),
            "top_email_domains": top_count_entries(&email_domains, options.top),
            "duplicate_group_count": duplicates.get("group_count").cloned().unwrap_or(Value::Null),
        },
        "duplicates": duplicates.get("groups").cloned().unwrap_or(Value::Array(Vec::new())),
        "issue_limit": options.issue_limit,
        "issues": issue_rows,
    }))
}

pub(crate) fn contact_quality_record(
    contact: &Value,
    duplicate_labels: Vec<String>,
) -> Result<Value> {
    let id = record_id(contact).unwrap_or_default();
    let name = contact_name(contact);
    let raw_emails = contact_string_values(contact, &["emails", "email"]);
    let emails = raw_emails
        .iter()
        .filter_map(|value| normalize_email_key(value))
        .collect::<Vec<_>>();
    let raw_phones = contact_string_values(contact, &["phone_numbers", "phone", "phones"]);
    let phones = raw_phones
        .iter()
        .filter_map(|value| normalize_phone_key(value))
        .collect::<Vec<_>>();
    let raw_linkedin = contact_string_values(contact, &["social_links", "linkedin"]);
    let linkedins = raw_linkedin
        .iter()
        .filter_map(|value| normalize_linkedin_key(value))
        .collect::<Vec<_>>();
    let has_work = row_has_any_data(contact, &["work_history", "title", "organization"]);
    let has_location = row_has_any_data(contact, &["location", "locations"]);
    let has_activity = row_has_any_data(
        contact,
        &[
            "interaction_history",
            "email_history",
            "event_history",
            "message_history",
            "notes",
        ],
    );
    let is_thin = contact_quality_is_thin_record(contact);
    let knows_channels = contact_quality_has_any_key(
        contact,
        &[
            "emails",
            "email",
            "phone_numbers",
            "phone",
            "phones",
            "social_links",
            "linkedin",
        ],
    );
    let knows_work =
        contact_quality_has_any_key(contact, &["work_history", "title", "organization"]);
    let knows_location = contact_quality_has_any_key(contact, &["location", "locations"]);
    let knows_activity = contact_quality_has_any_key(
        contact,
        &[
            "interaction_history",
            "email_history",
            "event_history",
            "message_history",
            "notes",
        ],
    );
    let mut issues = Vec::new();
    let mut warnings = Vec::new();
    let mut score = 100_i64;

    if name
        .as_ref()
        .and_then(|value| normalize_name_key(value))
        .is_none()
    {
        contact_quality_push_issue(&mut issues, &mut score, "missing_name", 25);
    }
    if knows_channels && emails.is_empty() && phones.is_empty() && linkedins.is_empty() {
        contact_quality_push_issue(&mut issues, &mut score, "missing_contact_channel", 35);
    }
    if !raw_emails.is_empty() && emails.len() < raw_emails.len() {
        contact_quality_push_issue(&mut issues, &mut score, "invalid_email", 15);
    }
    if !raw_phones.is_empty() && phones.len() < raw_phones.len() {
        contact_quality_push_issue(&mut issues, &mut score, "invalid_phone", 10);
    }
    if knows_work && !has_work {
        contact_quality_push_issue(&mut issues, &mut score, "missing_work", 10);
    }
    if knows_location && !has_location {
        contact_quality_push_issue(&mut issues, &mut score, "missing_location", 5);
    }
    if knows_activity && !has_activity {
        contact_quality_push_issue(&mut issues, &mut score, "missing_activity", 5);
    }
    if !duplicate_labels.is_empty() {
        for label in &duplicate_labels {
            contact_quality_push_issue(&mut issues, &mut score, label, 15);
        }
    }
    if is_thin {
        warnings.push("thin_search_row".to_string());
    }
    score = score.clamp(0, 100);

    Ok(json!({
        "id": id,
        "name": name.unwrap_or_default(),
        "score": score as u64,
        "issue_count": issues.len(),
        "warning_count": warnings.len(),
        "issues": issues,
        "warnings": warnings,
        "fields": {
            "name": !contact_name(contact).unwrap_or_default().trim().is_empty(),
            "email": !emails.is_empty(),
            "phone": !phones.is_empty(),
            "linkedin": !linkedins.is_empty(),
            "work": has_work,
            "location": has_location,
            "activity": has_activity,
        },
        "counts": {
            "emails": emails.len(),
            "phones": phones.len(),
            "linkedin": linkedins.len(),
        },
        "summary": dedupe_contact_summary(contact),
    }))
}

pub(crate) fn contact_quality_push_issue(
    issues: &mut Vec<String>,
    score: &mut i64,
    issue: &str,
    penalty: i64,
) {
    if !issues.iter().any(|existing| existing == issue) {
        issues.push(issue.to_string());
        *score -= penalty;
    }
}

pub(crate) fn contact_quality_is_thin_record(contact: &Value) -> bool {
    !row_has_any_data(
        contact,
        &[
            "emails",
            "email",
            "phone_numbers",
            "phone",
            "phones",
            "social_links",
            "linkedin",
            "work_history",
            "interaction_history",
        ],
    )
}

pub(crate) fn contact_quality_has_any_key(contact: &Value, keys: &[&str]) -> bool {
    let Some(object) = contact.as_object() else {
        return false;
    };
    keys.iter().any(|key| object.contains_key(*key))
}

pub(crate) fn contact_quality_email_domains(contact: &Value) -> BTreeSet<String> {
    contact_string_values(contact, &["emails", "email"])
        .into_iter()
        .filter_map(|value| normalize_email_key(&value))
        .filter_map(|value| email_domain_from_string(&value))
        .collect()
}

pub(crate) fn contact_quality_coverage(counts: &BTreeMap<String, u64>, rows: u64) -> Value {
    let fields = [
        "name", "email", "phone", "linkedin", "work", "location", "activity",
    ];
    let mut coverage = Map::new();
    for field in fields {
        let count = counts.get(field).copied().unwrap_or_default();
        let percent = if rows == 0 {
            0.0
        } else {
            (count as f64) * 100.0 / (rows as f64)
        };
        coverage.insert(
            field.to_string(),
            json!({ "count": count, "percent": percent }),
        );
    }
    Value::Object(coverage)
}

pub(crate) fn contact_quality_duplicates(contacts: &[Value], top: usize) -> Value {
    let signals = [
        DedupeSignal::Email,
        DedupeSignal::Phone,
        DedupeSignal::Linkedin,
        DedupeSignal::Name,
    ];
    let summaries = contacts
        .iter()
        .map(dedupe_contact_summary)
        .collect::<Vec<_>>();
    let mut groups = Vec::new();
    let mut by_contact: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for signal in signals {
        let mut buckets: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (index, contact) in contacts.iter().enumerate() {
            for key in dedupe_signal_keys(contact, signal) {
                buckets.entry(key).or_default().push(index);
            }
        }
        for (key, indexes) in buckets {
            if indexes.len() < 2 {
                continue;
            }
            let label = format!("duplicate_{}", signal.as_str());
            let contact_ids = indexes
                .iter()
                .filter_map(|index| summaries[*index].get("id").and_then(Value::as_str))
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            for index in &indexes {
                by_contact
                    .entry(index.to_string())
                    .or_default()
                    .insert(label.clone());
            }
            groups.push(json!({
                "signal": signal.as_str(),
                "key": key,
                "contact_count": indexes.len(),
                "contact_ids": contact_ids,
            }));
        }
    }
    groups.sort_by(|left, right| {
        let left_count = left
            .get("contact_count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let right_count = right
            .get("contact_count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        right_count
            .cmp(&left_count)
            .then_with(|| {
                left.get("signal")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(
                        right
                            .get("signal")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
            })
            .then_with(|| {
                left.get("key")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(right.get("key").and_then(Value::as_str).unwrap_or_default())
            })
    });
    let group_count = groups.len();
    groups.truncate(top);
    let by_contact = by_contact
        .into_iter()
        .map(|(key, values)| {
            (
                key,
                Value::Array(values.into_iter().map(Value::String).collect()),
            )
        })
        .collect::<Map<_, _>>();
    json!({
        "group_count": group_count,
        "groups": groups,
        "by_contact": by_contact,
    })
}

pub(crate) fn contact_quality_table_rows(report: &Value) -> Value {
    let Some(rows) = report.get("issues").and_then(Value::as_array) else {
        return Value::Array(Vec::new());
    };
    Value::Array(
        rows.iter()
            .map(|row| {
                json!({
                    "score": row.get("score").cloned().unwrap_or(Value::Null),
                    "id": row.get("id").cloned().unwrap_or(Value::Null),
                    "name": row.get("name").cloned().unwrap_or(Value::Null),
                    "issues": row.get("issues").map(cell_string).unwrap_or_default(),
                    "warnings": row.get("warnings").map(cell_string).unwrap_or_default(),
                })
            })
            .collect(),
    )
}

pub(crate) fn contact_facets_report(
    source: Value,
    contacts: Vec<Value>,
    options: ContactFacetsOptions,
) -> Value {
    let contact_count = contacts.len() as u64;
    let facets = options
        .facets
        .iter()
        .map(|facet| contact_facet_report(*facet, &contacts, &options, contact_count))
        .collect::<Vec<_>>();
    let returned_bucket_count = facets
        .iter()
        .filter_map(|facet| facet.get("returned_bucket_count").and_then(Value::as_u64))
        .sum::<u64>();
    json!({
        "source": source,
        "analyzed_count": contacts.len(),
        "options": {
            "facets": options.facets.iter().map(|facet| facet.as_str()).collect::<Vec<_>>(),
            "top": options.top,
            "min_count": options.min_count,
            "sample_limit": options.sample_limit,
            "include_empty": options.include_empty,
        },
        "summary": {
            "contact_count": contacts.len(),
            "facet_count": options.facets.len(),
            "returned_bucket_count": returned_bucket_count,
        },
        "facets": facets,
    })
}

pub(crate) fn contact_facet_report(
    facet: ContactFacetKind,
    contacts: &[Value],
    options: &ContactFacetsOptions,
    contact_count: u64,
) -> Value {
    let mut buckets: BTreeMap<String, ContactFacetBucket> = BTreeMap::new();
    let mut missing_contact_count = 0_u64;
    let mut value_contact_count = 0_u64;

    for contact in contacts {
        let mut values = contact_facet_values(contact, facet);
        if values.is_empty() {
            missing_contact_count += 1;
            if options.include_empty {
                values.push("(empty)".to_string());
            }
        }
        for value in values {
            value_contact_count += 1;
            let key = contact_facet_bucket_key(&value);
            let bucket = buckets.entry(key).or_insert_with(|| ContactFacetBucket {
                value,
                ..Default::default()
            });
            bucket.count += 1;
            if bucket.sample_contacts.len() < options.sample_limit {
                bucket.sample_contacts.push(contact_facet_sample(contact));
            }
        }
    }

    let unique_value_count = buckets.len() as u64;
    let mut rows = buckets
        .into_iter()
        .filter(|(_, bucket)| bucket.count >= options.min_count as u64)
        .map(|(_, bucket)| {
            let ContactFacetBucket {
                value,
                count,
                sample_contacts,
            } = bucket;
            let percent = if contact_count == 0 {
                0.0
            } else {
                (count as f64) * 100.0 / (contact_count as f64)
            };
            (
                count,
                contact_facet_bucket_key(&value),
                json!({
                    "facet": facet.as_str(),
                    "value": value,
                    "count": count,
                    "percent": percent,
                    "sample_contacts": sample_contacts,
                }),
            )
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    let matched_bucket_count = rows.len() as u64;
    rows.truncate(options.top);
    let rows = rows.into_iter().map(|(_, _, row)| row).collect::<Vec<_>>();

    json!({
        "facet": facet.as_str(),
        "contact_count": contact_count,
        "value_contact_count": value_contact_count,
        "unique_value_count": unique_value_count,
        "matched_bucket_count": matched_bucket_count,
        "returned_bucket_count": rows.len(),
        "missing_contact_count": missing_contact_count,
        "rows": rows,
    })
}

pub(crate) fn contact_facets_table_rows(report: &Value) -> Value {
    let mut output = Vec::new();
    let Some(facets) = report.get("facets").and_then(Value::as_array) else {
        return Value::Array(output);
    };
    for facet in facets {
        let facet_name = facet
            .get("facet")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let unique_value_count = facet
            .get("unique_value_count")
            .cloned()
            .unwrap_or(Value::Null);
        let missing_contact_count = facet
            .get("missing_contact_count")
            .cloned()
            .unwrap_or(Value::Null);
        let Some(rows) = facet.get("rows").and_then(Value::as_array) else {
            continue;
        };
        if rows.is_empty() {
            output.push(json!({
                "facet": facet_name,
                "value": Value::Null,
                "count": 0,
                "percent": 0.0,
                "unique_value_count": unique_value_count,
                "missing_contact_count": missing_contact_count,
                "sample_contact_ids": "",
                "sample_contact_names": "",
            }));
            continue;
        }
        for row in rows {
            output.push(json!({
                "facet": facet_name,
                "value": row.get("value").cloned().unwrap_or(Value::Null),
                "count": row.get("count").cloned().unwrap_or(Value::Null),
                "percent": row.get("percent").cloned().unwrap_or(Value::Null),
                "unique_value_count": unique_value_count,
                "missing_contact_count": missing_contact_count,
                "sample_contact_ids": contact_facets_sample_cell(row, "id"),
                "sample_contact_names": contact_facets_sample_cell(row, "name"),
            }));
        }
    }
    Value::Array(output)
}

pub(crate) fn contact_facets_sample_cell(row: &Value, key: &str) -> String {
    row.get("sample_contacts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|sample| sample.get(key).and_then(Value::as_str))
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn contact_pivot_report(
    source: Value,
    contacts: Vec<Value>,
    options: ContactPivotOptions,
) -> Value {
    let contact_count = contacts.len();
    let mut row_totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut col_totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut row_labels: BTreeMap<String, String> = BTreeMap::new();
    let mut col_labels: BTreeMap<String, String> = BTreeMap::new();
    let mut cells: BTreeMap<(String, String), ContactPivotCell> = BTreeMap::new();
    let mut contributing_contact_count = 0_u64;
    let mut total_pair_count = 0_u64;

    for contact in &contacts {
        let row_values =
            contact_pivot_axis_values(contact, options.row_facet, options.include_empty);
        let col_values =
            contact_pivot_axis_values(contact, options.col_facet, options.include_empty);
        if row_values.is_empty() || col_values.is_empty() {
            continue;
        }
        contributing_contact_count += 1;
        for row_value in &row_values {
            let row_key = contact_facet_bucket_key(row_value);
            row_labels
                .entry(row_key.clone())
                .or_insert_with(|| row_value.clone());
            for col_value in &col_values {
                let col_key = contact_facet_bucket_key(col_value);
                col_labels
                    .entry(col_key.clone())
                    .or_insert_with(|| col_value.clone());
                total_pair_count += 1;
                *row_totals.entry(row_key.clone()).or_default() += 1;
                *col_totals.entry(col_key.clone()).or_default() += 1;
                let cell = cells.entry((row_key.clone(), col_key)).or_default();
                cell.count += 1;
                if cell.sample_contacts.len() < options.sample_limit {
                    cell.sample_contacts.push(contact_facet_sample(contact));
                }
            }
        }
    }

    let returned_rows = contact_pivot_top_keys(&row_totals, options.top_rows);
    let returned_cols = contact_pivot_top_keys(&col_totals, options.top_cols);
    let mut returned_col_values = BTreeSet::new();
    let mut row_reports = Vec::new();
    let mut returned_cell_count = 0_u64;

    for row_key in returned_rows {
        let row_total = row_totals.get(&row_key).copied().unwrap_or_default();
        let row_value = row_labels.get(&row_key).cloned().unwrap_or(row_key.clone());
        let mut cell_reports = Vec::new();
        for col_key in &returned_cols {
            let Some(cell) = cells.get(&(row_key.clone(), col_key.clone())) else {
                continue;
            };
            if cell.count < options.min_count as u64 {
                continue;
            }
            let col_total = col_totals.get(col_key).copied().unwrap_or_default();
            let col_value = col_labels.get(col_key).cloned().unwrap_or(col_key.clone());
            returned_cell_count += 1;
            returned_col_values.insert(col_key.clone());
            cell_reports.push(json!({
                "col": col_value,
                "col_total_count": col_total,
                "count": cell.count,
                "percent_of_row": contact_pivot_percent(cell.count, row_total),
                "percent_of_col": contact_pivot_percent(cell.count, col_total),
                "percent_of_total": contact_pivot_percent(cell.count, total_pair_count),
                "sample_contacts": cell.sample_contacts.clone(),
            }));
        }
        if !cell_reports.is_empty() {
            row_reports.push(json!({
                "row": row_value,
                "total_count": row_total,
                "percent_of_total": contact_pivot_percent(row_total, total_pair_count),
                "cells": cell_reports,
            }));
        }
    }

    let columns = returned_cols
        .into_iter()
        .filter(|col_key| returned_col_values.contains(col_key))
        .map(|col_key| {
            let col_total = col_totals.get(&col_key).copied().unwrap_or_default();
            let col_value = col_labels.get(&col_key).cloned().unwrap_or(col_key);
            json!({
                "col": col_value,
                "total_count": col_total,
                "percent_of_total": contact_pivot_percent(col_total, total_pair_count),
            })
        })
        .collect::<Vec<_>>();
    let matched_cell_count = cells
        .values()
        .filter(|cell| cell.count >= options.min_count as u64)
        .count() as u64;

    json!({
        "source": source,
        "analyzed_count": contact_count,
        "options": contact_pivot_options_value(&options),
        "summary": {
            "contact_count": contact_count,
            "contributing_contact_count": contributing_contact_count,
            "row_value_count": row_totals.len(),
            "col_value_count": col_totals.len(),
            "cell_count": cells.len(),
            "matched_cell_count": matched_cell_count,
            "returned_row_count": row_reports.len(),
            "returned_col_count": columns.len(),
            "returned_cell_count": returned_cell_count,
            "total_pair_count": total_pair_count,
        },
        "columns": columns,
        "rows": row_reports,
    })
}

pub(crate) fn contact_pivot_axis_values(
    contact: &Value,
    facet: ContactFacetKind,
    include_empty: bool,
) -> Vec<String> {
    let mut values = contact_facet_values(contact, facet);
    if values.is_empty() && include_empty {
        values.push("(empty)".to_string());
    }
    values
}

pub(crate) fn contact_pivot_top_keys(totals: &BTreeMap<String, u64>, limit: usize) -> Vec<String> {
    let mut rows = totals
        .iter()
        .map(|(value, count)| (*count, contact_facet_bucket_key(value), value.clone()))
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    rows.truncate(limit);
    rows.into_iter().map(|(_, _, value)| value).collect()
}

pub(crate) fn contact_pivot_percent(count: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (count as f64) * 100.0 / (total as f64)
    }
}

pub(crate) fn contact_pivot_options_value(options: &ContactPivotOptions) -> Value {
    json!({
        "row_facet": options.row_facet.as_str(),
        "col_facet": options.col_facet.as_str(),
        "top_rows": options.top_rows,
        "top_cols": options.top_cols,
        "min_count": options.min_count,
        "sample_limit": options.sample_limit,
        "include_empty": options.include_empty,
    })
}

pub(crate) fn contact_pivot_flat_rows(report: &Value) -> Value {
    let mut rows = Vec::new();
    let source_type = report
        .get("source")
        .and_then(|source| source.get("type"))
        .cloned()
        .unwrap_or(Value::Null);
    let options = report.get("options").unwrap_or(&Value::Null);
    let row_facet = options.get("row_facet").cloned().unwrap_or(Value::Null);
    let col_facet = options.get("col_facet").cloned().unwrap_or(Value::Null);

    if let Some(pivot_rows) = report.get("rows").and_then(Value::as_array) {
        for pivot_row in pivot_rows {
            let row_value = pivot_row.get("row").cloned().unwrap_or(Value::Null);
            let row_total_count = pivot_row.get("total_count").cloned().unwrap_or(Value::Null);
            let Some(cells) = pivot_row.get("cells").and_then(Value::as_array) else {
                continue;
            };
            for cell in cells {
                rows.push(json!({
                    "row_type": "pivot_cell",
                    "row_facet": row_facet.clone(),
                    "row_value": row_value.clone(),
                    "row_total_count": row_total_count.clone(),
                    "col_facet": col_facet.clone(),
                    "col_value": cell.get("col").cloned().unwrap_or(Value::Null),
                    "col_total_count": cell.get("col_total_count").cloned().unwrap_or(Value::Null),
                    "count": cell.get("count").cloned().unwrap_or(Value::Null),
                    "percent_of_row": cell.get("percent_of_row").cloned().unwrap_or(Value::Null),
                    "percent_of_col": cell.get("percent_of_col").cloned().unwrap_or(Value::Null),
                    "percent_of_total": cell.get("percent_of_total").cloned().unwrap_or(Value::Null),
                    "sample_contact_ids": contact_facets_sample_cell(cell, "id"),
                    "sample_contact_names": contact_facets_sample_cell(cell, "name"),
                    "source_type": source_type.clone(),
                }));
            }
        }
    }
    Value::Array(rows)
}

pub(crate) fn contact_overview_report(
    source: Value,
    contacts: Vec<Value>,
    options: ContactOverviewOptions,
) -> Result<Value> {
    let analyzed_count = contacts.len();
    let mut quality =
        contact_quality_report(source.clone(), contacts.clone(), options.quality_options())?;
    let mut dedupe = dedupe_contacts(
        source.clone(),
        contacts.clone(),
        &options.dedupe_signals,
        options.min_confidence,
        Some(options.candidate_limit),
    );
    let mut facets = contact_facets_report(source.clone(), contacts, options.facet_options());
    remove_nested_source(&mut quality);
    remove_nested_source(&mut dedupe);
    remove_nested_source(&mut facets);

    Ok(json!({
        "source": source,
        "analyzed_count": analyzed_count,
        "options": contact_overview_options_value(&options),
        "summary": contact_overview_summary(analyzed_count, &quality, &dedupe, &facets, &options),
        "quality": quality,
        "dedupe": dedupe,
        "facets": facets,
    }))
}

pub(crate) fn contact_overview_options_value(options: &ContactOverviewOptions) -> Value {
    json!({
        "facets": options.facets.iter().map(|facet| facet.as_str()).collect::<Vec<_>>(),
        "dedupe_by": options.dedupe_signals.iter().map(|signal| signal.as_str()).collect::<Vec<_>>(),
        "min_confidence": options.min_confidence,
        "candidate_limit": options.candidate_limit,
        "issue_limit": options.issue_limit,
        "top": options.top,
        "min_count": options.min_count,
        "sample_limit": options.sample_limit,
        "include_empty": options.include_empty,
    })
}

pub(crate) fn contact_overview_summary(
    analyzed_count: usize,
    quality: &Value,
    dedupe: &Value,
    facets: &Value,
    options: &ContactOverviewOptions,
) -> Value {
    let quality_summary = quality.get("summary").unwrap_or(&Value::Null);
    let facet_summary = facets.get("summary").unwrap_or(&Value::Null);
    json!({
        "contact_count": analyzed_count,
        "average_quality_score": quality_summary.get("average_score").cloned().unwrap_or(Value::Null),
        "issue_contact_count": quality_summary.get("issue_contact_count").cloned().unwrap_or(Value::Null),
        "warning_contact_count": quality_summary.get("warning_contact_count").cloned().unwrap_or(Value::Null),
        "total_issues": quality_summary.get("total_issues").cloned().unwrap_or(Value::Null),
        "total_warnings": quality_summary.get("total_warnings").cloned().unwrap_or(Value::Null),
        "duplicate_candidate_count": dedupe.get("candidate_count").cloned().unwrap_or(Value::Null),
        "duplicate_candidate_limit": options.candidate_limit,
        "facet_count": facet_summary.get("facet_count").cloned().unwrap_or(Value::Null),
        "returned_facet_bucket_count": facet_summary.get("returned_bucket_count").cloned().unwrap_or(Value::Null),
    })
}

pub(crate) fn contact_overview_flat_rows(report: &Value) -> Value {
    let mut rows = Vec::new();
    if let Some(summary) = report.get("summary") {
        let source_type = report
            .get("source")
            .and_then(|source| source.get("type"))
            .cloned()
            .unwrap_or(Value::Null);
        for metric in [
            "contact_count",
            "average_quality_score",
            "issue_contact_count",
            "warning_contact_count",
            "total_issues",
            "total_warnings",
            "duplicate_candidate_count",
            "returned_facet_bucket_count",
        ] {
            rows.push(json!({
                "row_type": "summary",
                "section": "overview",
                "metric": metric,
                "value": summary.get(metric).cloned().unwrap_or(Value::Null),
                "source_type": source_type.clone(),
            }));
        }
    }
    if let Some(issues) = report
        .get("quality")
        .and_then(|quality| quality.get("issues"))
        .and_then(Value::as_array)
    {
        for issue in issues {
            rows.push(json!({
                "row_type": "quality_issue",
                "section": "quality",
                "metric": "contact_issues",
                "value": issue.get("issues").map(cell_string).unwrap_or_default(),
                "count": issue.get("issue_count").cloned().unwrap_or(Value::Null),
                "score": issue.get("score").cloned().unwrap_or(Value::Null),
                "contact_id": issue.get("id").cloned().unwrap_or(Value::Null),
                "contact_name": issue.get("name").cloned().unwrap_or(Value::Null),
                "details": issue.get("warnings").map(cell_string).unwrap_or_default(),
            }));
        }
    }
    if let Some(candidates) = report
        .get("dedupe")
        .and_then(|dedupe| dedupe.get("candidates"))
        .and_then(Value::as_array)
    {
        for candidate in candidates {
            rows.push(json!({
                "row_type": "duplicate_candidate",
                "section": "dedupe",
                "metric": candidate.get("signal").cloned().unwrap_or(Value::Null),
                "value": candidate.get("key").cloned().unwrap_or(Value::Null),
                "count": candidate.get("contact_count").cloned().unwrap_or(Value::Null),
                "score": candidate.get("confidence").cloned().unwrap_or(Value::Null),
                "contact_ids": candidate.get("contact_ids").map(cell_string).unwrap_or_default(),
                "command": candidate.get("merge_plan_command").cloned().unwrap_or(Value::Null),
            }));
        }
    }
    if let Some(facets) = report
        .get("facets")
        .and_then(|facets| facets.get("facets"))
        .and_then(Value::as_array)
    {
        for facet in facets {
            let facet_name = facet
                .get("facet")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let unique_value_count = facet
                .get("unique_value_count")
                .cloned()
                .unwrap_or(Value::Null);
            let missing_contact_count = facet
                .get("missing_contact_count")
                .cloned()
                .unwrap_or(Value::Null);
            let Some(facet_rows) = facet.get("rows").and_then(Value::as_array) else {
                continue;
            };
            for row in facet_rows {
                rows.push(json!({
                    "row_type": "facet_bucket",
                    "section": "facets",
                    "metric": facet_name,
                    "value": row.get("value").cloned().unwrap_or(Value::Null),
                    "count": row.get("count").cloned().unwrap_or(Value::Null),
                    "percent": row.get("percent").cloned().unwrap_or(Value::Null),
                    "details": format!(
                        "unique_value_count={}; missing_contact_count={}",
                        cell_string(&unique_value_count),
                        cell_string(&missing_contact_count)
                    ),
                    "sample_contact_ids": contact_facets_sample_cell(row, "id"),
                    "sample_contact_names": contact_facets_sample_cell(row, "name"),
                }));
            }
        }
    }
    Value::Array(rows)
}

pub(crate) fn contact_facet_values(contact: &Value, facet: ContactFacetKind) -> Vec<String> {
    let values = match facet {
        ContactFacetKind::EmailDomain => contact_quality_email_domains(contact)
            .into_iter()
            .collect::<Vec<_>>(),
        ContactFacetKind::Company => contact_company_facet_values(contact),
        ContactFacetKind::Title => contact_work_facet_values(
            contact,
            &["title", "position", "role", "job_title", "headline"],
        ),
        ContactFacetKind::Location => contact_location_facet_values(contact),
        ContactFacetKind::Integration => contact_named_facet_values(
            contact,
            &["integrations", "integration"],
            &["name", "provider", "source", "type", "service", "value"],
        ),
        ContactFacetKind::Channel => contact_channel_facet_values(contact),
        ContactFacetKind::NameInitial => contact_name_initial_facet_values(contact),
    };
    normalize_contact_facet_values(values)
}

pub(crate) fn contact_company_facet_values(contact: &Value) -> Vec<String> {
    let mut output = contact_named_facet_values(
        contact,
        &["organization", "company", "company_name", "employer"],
        &["name", "title", "value"],
    );
    if let Some(object) = contact.as_object() {
        for value in object_values_by_aliases(object, &["work_history", "workHistory", "work"]) {
            collect_work_facet_strings(
                value,
                &[
                    "organization",
                    "company",
                    "company_name",
                    "employer",
                    "name",
                ],
                &mut output,
            );
        }
    }
    output
}

pub(crate) fn contact_work_facet_values(contact: &Value, aliases: &[&str]) -> Vec<String> {
    let mut output = contact_named_facet_values(contact, aliases, &["name", "title", "value"]);
    if let Some(object) = contact.as_object() {
        for value in object_values_by_aliases(object, &["work_history", "workHistory", "work"]) {
            collect_work_facet_strings(value, aliases, &mut output);
        }
    }
    output
}

pub(crate) fn contact_named_facet_values(
    contact: &Value,
    fields: &[&str],
    nested_aliases: &[&str],
) -> Vec<String> {
    let Some(object) = contact.as_object() else {
        return Vec::new();
    };
    let mut output = Vec::new();
    for value in object_values_by_aliases(object, fields) {
        collect_named_facet_strings(value, nested_aliases, &mut output);
    }
    output
}

pub(crate) fn collect_work_facet_strings(
    value: &Value,
    aliases: &[&str],
    output: &mut Vec<String>,
) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_work_facet_strings(item, aliases, output);
            }
        }
        Value::Object(object) => {
            for value in object_values_by_aliases(object, aliases) {
                collect_named_facet_strings(
                    value,
                    &[
                        "name",
                        "title",
                        "position",
                        "role",
                        "company",
                        "organization",
                        "value",
                    ],
                    output,
                );
            }
        }
        Value::String(_) | Value::Number(_) | Value::Bool(_) => {
            collect_named_facet_strings(value, &["value"], output);
        }
        Value::Null => {}
    }
}

pub(crate) fn collect_named_facet_strings(
    value: &Value,
    nested_aliases: &[&str],
    output: &mut Vec<String>,
) {
    match value {
        Value::String(value) => output.push(value.clone()),
        Value::Number(_) | Value::Bool(_) => output.push(cell_string(value)),
        Value::Array(items) => {
            for item in items {
                collect_named_facet_strings(item, nested_aliases, output);
            }
        }
        Value::Object(object) => {
            for value in object_values_by_aliases(object, nested_aliases) {
                collect_named_facet_strings(value, nested_aliases, output);
            }
        }
        Value::Null => {}
    }
}

pub(crate) fn contact_location_facet_values(contact: &Value) -> Vec<String> {
    let Some(object) = contact.as_object() else {
        return Vec::new();
    };
    let mut output = Vec::new();
    for value in object_values_by_aliases(object, &["location", "locations"]) {
        collect_location_facet_strings(value, &mut output);
    }
    output
}

pub(crate) fn collect_location_facet_strings(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::String(value) => output.push(value.clone()),
        Value::Array(items) => {
            for item in items {
                collect_location_facet_strings(item, output);
            }
        }
        Value::Object(object) => {
            if let Some(value) = object_string_by_aliases(
                object,
                &["name", "display_name", "displayName", "value", "address"],
            ) {
                output.push(value);
                return;
            }
            let parts = [
                object_string_by_aliases(object, &["city", "town", "locality"]),
                object_string_by_aliases(object, &["region", "state", "province"]),
                object_string_by_aliases(object, &["country", "country_name"]),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
            if !parts.is_empty() {
                output.push(parts.join(", "));
            }
        }
        Value::Number(_) | Value::Bool(_) | Value::Null => {}
    }
}

pub(crate) fn contact_channel_facet_values(contact: &Value) -> Vec<String> {
    let mut values = Vec::new();
    if contact_string_values(contact, &["emails", "email"])
        .into_iter()
        .any(|value| normalize_email_key(&value).is_some())
    {
        values.push("email".to_string());
    }
    if contact_string_values(contact, &["phone_numbers", "phone", "phones"])
        .into_iter()
        .any(|value| normalize_phone_key(&value).is_some())
    {
        values.push("phone".to_string());
    }
    if contact_string_values(contact, &["social_links", "linkedin"])
        .into_iter()
        .any(|value| normalize_linkedin_key(&value).is_some())
    {
        values.push("linkedin".to_string());
    }
    values
}

pub(crate) fn contact_name_initial_facet_values(contact: &Value) -> Vec<String> {
    contact_name(contact)
        .and_then(|name| {
            name.trim()
                .chars()
                .find(|ch| ch.is_alphanumeric())
                .map(|ch| ch.to_uppercase().collect::<String>())
        })
        .into_iter()
        .collect()
}

pub(crate) fn normalize_contact_facet_values(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut output = Vec::new();
    for value in values {
        let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.is_empty() {
            continue;
        }
        if seen.insert(contact_facet_bucket_key(&normalized)) {
            output.push(normalized);
        }
    }
    output
}

pub(crate) fn contact_facet_bucket_key(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub(crate) fn contact_facet_sample(contact: &Value) -> Value {
    let mut sample = Map::new();
    if let Some(id) = record_id(contact) {
        sample.set("id", id);
    }
    if let Some(name) = contact_name(contact) {
        sample.set("name", name);
    }
    Value::Object(sample)
}

pub(crate) fn contacts_for_dedupe_input(
    path: &Path,
    requested_format: InputFormat,
) -> Result<(Value, Vec<Value>)> {
    contacts_for_input(path, requested_format, "contacts:dedupe")
}

pub(crate) fn contacts_for_dedupe_snapshot(dir: &Path) -> Result<(Value, Vec<Value>)> {
    let verify = verify_snapshot(dir)?;
    if !verify.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return err("snapshot failed manifest verification");
    }
    let label = if snapshot_manifest_has_file(dir, "full_contacts")? {
        "full_contacts"
    } else {
        "contacts"
    };
    let entry = snapshot_manifest_file_entry(dir, label)?
        .ok_or_else(|| miette!("snapshot does not contain file {label}"))?;
    let path = entry
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| miette!("snapshot file entry {label} is missing path"))?;
    let contacts = read_snapshot_jsonl_values_at_path(&safe_snapshot_file_path(dir, path)?)?;
    Ok((
        json!({
            "type": "snapshot",
            "dir": dir.display().to_string(),
            "file": path,
            "file_label": label,
            "analyzed_count": contacts.len(),
        }),
        contacts,
    ))
}

pub(crate) fn dedupe_contacts(
    source: Value,
    contacts: Vec<Value>,
    signals: &[DedupeSignal],
    min_confidence: u64,
    limit: Option<usize>,
) -> Value {
    let summaries = contacts
        .iter()
        .map(dedupe_contact_summary)
        .collect::<Vec<_>>();
    let mut buckets: BTreeMap<(DedupeSignal, String), BTreeSet<usize>> = BTreeMap::new();
    for (index, contact) in contacts.iter().enumerate() {
        for signal in signals {
            for key in dedupe_signal_keys(contact, *signal) {
                buckets.entry((*signal, key)).or_default().insert(index);
            }
        }
    }

    let mut candidates = Vec::new();
    for ((signal, key), indexes) in buckets {
        if indexes.len() < 2 || signal.confidence() < min_confidence {
            continue;
        }
        let contact_ids = indexes
            .iter()
            .filter_map(|index| summaries[*index].get("id").and_then(Value::as_str))
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let merge_plan_command = if contact_ids.len() >= 2 {
            Value::String(format!(
                "mesh contacts:merge-plan --contact-ids {}",
                contact_ids.join(",")
            ))
        } else {
            Value::Null
        };
        let contacts = indexes
            .iter()
            .map(|index| summaries[*index].clone())
            .collect::<Vec<_>>();
        let value = json!({
            "signal": signal.as_str(),
            "key": key,
            "confidence": signal.confidence(),
            "contact_count": contacts.len(),
            "contact_ids": contact_ids,
            "merge_plan_command": merge_plan_command,
            "contacts": contacts,
        });
        candidates.push((
            signal.confidence(),
            value
                .get("contact_count")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            format!(
                "{}:{}",
                signal.as_str(),
                value.get("key").and_then(Value::as_str).unwrap_or_default()
            ),
            value,
        ));
    }
    candidates.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    if let Some(limit) = limit {
        candidates.truncate(limit);
    }
    let candidate_values = candidates
        .into_iter()
        .map(|(_, _, _, value)| value)
        .collect::<Vec<_>>();
    json!({
        "source": source,
        "signals": signals.iter().map(|signal| signal.as_str()).collect::<Vec<_>>(),
        "min_confidence": min_confidence,
        "analyzed_count": contacts.len(),
        "candidate_count": candidate_values.len(),
        "candidates": candidate_values,
    })
}

pub(crate) fn dedupe_signal_keys(contact: &Value, signal: DedupeSignal) -> BTreeSet<String> {
    match signal {
        DedupeSignal::Email => contact_string_values(contact, &["emails", "email"])
            .into_iter()
            .filter_map(|value| normalize_email_key(&value))
            .collect(),
        DedupeSignal::Phone => {
            contact_string_values(contact, &["phone_numbers", "phone", "phones"])
                .into_iter()
                .filter_map(|value| normalize_phone_key(&value))
                .collect()
        }
        DedupeSignal::Linkedin => contact_string_values(contact, &["social_links", "linkedin"])
            .into_iter()
            .filter_map(|value| normalize_linkedin_key(&value))
            .collect(),
        DedupeSignal::Name => contact_name(contact)
            .and_then(|value| normalize_name_key(&value))
            .into_iter()
            .collect(),
    }
}

pub(crate) fn dedupe_contact_summary(contact: &Value) -> Value {
    let mut summary = Map::new();
    if let Some(id) = record_id(contact) {
        summary.set("id", id);
    }
    if let Some(name) = contact_name(contact) {
        summary.set("name", name);
    }
    if let Some(url) = first_contact_string(contact, &["url"]) {
        summary.set("url", url);
    }
    let emails = contact_string_values(contact, &["emails", "email"])
        .into_iter()
        .filter_map(|value| normalize_email_key(&value))
        .collect::<Vec<_>>();
    if !emails.is_empty() {
        summary.insert(
            "emails".to_string(),
            Value::Array(emails.into_iter().map(Value::String).collect()),
        );
    }
    let phones = contact_string_values(contact, &["phone_numbers", "phone", "phones"])
        .into_iter()
        .filter_map(|value| normalize_phone_key(&value))
        .collect::<Vec<_>>();
    if !phones.is_empty() {
        summary.insert(
            "phones".to_string(),
            Value::Array(phones.into_iter().map(Value::String).collect()),
        );
    }
    let linkedins = contact_string_values(contact, &["social_links", "linkedin"])
        .into_iter()
        .filter_map(|value| normalize_linkedin_key(&value))
        .collect::<Vec<_>>();
    if !linkedins.is_empty() {
        summary.insert(
            "linkedin".to_string(),
            Value::Array(linkedins.into_iter().map(Value::String).collect()),
        );
    }
    Value::Object(summary)
}

pub(crate) fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.to_lowercase()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Arg, ArgAction, Command};

    fn matches_with_values(flag: &'static str, values: &[&str]) -> ArgMatches {
        let mut args = vec!["mesh".to_string()];
        let flag_arg = format!("--{flag}");
        for value in values {
            args.push(flag_arg.clone());
            args.push((*value).to_string());
        }
        Command::new("mesh")
            .arg(Arg::new(flag).long(flag).action(ArgAction::Append))
            .try_get_matches_from(args)
            .unwrap()
    }

    fn temp_snapshot_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "meshx-contact-audit-{label}-{}-{}",
            std::process::id(),
            now_millis()
        ))
    }

    fn write_snapshot_manifest_file(
        dir: &Path,
        label: &str,
        path: &str,
        content: &str,
    ) -> Result<()> {
        fs::create_dir_all(dir.join("data")).into_diagnostic()?;
        let file_path = safe_snapshot_file_path(dir, path)?;
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).into_diagnostic()?;
        }
        fs::write(file_path, content).into_diagnostic()?;
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_string(&json!({
                "files": {
                    label: {
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
    fn contacts_for_dedupe_snapshot_reads_full_contacts_from_manifest_path() -> Result<()> {
        let dir = temp_snapshot_dir("dedupe-manifest-full-contacts");
        let content = "{\"id\":7,\"name\":\"Ada\"}\n";
        write_snapshot_manifest_file(&dir, "full_contacts", "data/full-contacts.jsonl", content)?;

        let (source, contacts) = contacts_for_dedupe_snapshot(&dir)?;

        fs::remove_dir_all(&dir).ok();
        assert_eq!(source.get("file"), Some(&json!("data/full-contacts.jsonl")));
        assert_eq!(contacts, vec![json!({"id":7,"name":"Ada"})]);
        Ok(())
    }

    #[test]
    fn contacts_for_dedupe_snapshot_reads_contacts_from_manifest_path() -> Result<()> {
        let dir = temp_snapshot_dir("dedupe-manifest-contacts");
        let content = "{\"id\":8,\"name\":\"Grace\"}\n";
        write_snapshot_manifest_file(&dir, "contacts", "data/contacts.jsonl", content)?;

        let (source, contacts) = contacts_for_dedupe_snapshot(&dir)?;

        fs::remove_dir_all(&dir).ok();
        assert_eq!(source.get("file"), Some(&json!("data/contacts.jsonl")));
        assert_eq!(contacts, vec![json!({"id":8,"name":"Grace"})]);
        Ok(())
    }

    #[test]
    fn dedupe_signals_from_matches_defaults_to_all_signals() -> Result<()> {
        let signals = dedupe_signals_from_matches(&matches_with_values("by", &[]))?;

        assert_eq!(
            signals,
            vec![
                DedupeSignal::Email,
                DedupeSignal::Phone,
                DedupeSignal::Linkedin,
                DedupeSignal::Name
            ]
        );
        Ok(())
    }

    #[test]
    fn dedupe_signals_from_matches_splits_aliases_and_removes_duplicates() -> Result<()> {
        let signals = dedupe_signals_from_matches(&matches_with_values(
            "by",
            &["emails, phone-number", "linkedin_url,email"],
        ))?;

        assert_eq!(
            signals,
            vec![
                DedupeSignal::Email,
                DedupeSignal::Phone,
                DedupeSignal::Linkedin
            ]
        );
        Ok(())
    }

    #[test]
    fn contact_facets_from_matches_defaults_to_core_facets() -> Result<()> {
        let facets = contact_facets_from_matches(&matches_with_values("by", &[]))?;

        assert_eq!(
            facets,
            vec![
                ContactFacetKind::EmailDomain,
                ContactFacetKind::Company,
                ContactFacetKind::Title,
                ContactFacetKind::Location
            ]
        );
        Ok(())
    }

    #[test]
    fn contact_overview_facets_from_matches_uses_facets_flag_aliases() -> Result<()> {
        let facets = contact_overview_facets_from_matches(&matches_with_values(
            "facets",
            &["domains, org", "contact_channels,domains"],
        ))?;

        assert_eq!(
            facets,
            vec![
                ContactFacetKind::EmailDomain,
                ContactFacetKind::Company,
                ContactFacetKind::Channel
            ]
        );
        Ok(())
    }

    #[test]
    fn contact_facet_report_counts_case_variants_together() {
        let options = ContactFacetsOptions {
            facets: vec![ContactFacetKind::Company],
            top: 10,
            min_count: 1,
            sample_limit: 10,
            include_empty: false,
            flat: false,
        };
        let report = contact_facet_report(
            ContactFacetKind::Company,
            &[
                json!({"id": 1, "organization": "Acme"}),
                json!({"id": 2, "organization": "acme"}),
            ],
            &options,
            2,
        );

        assert_eq!(report.get("unique_value_count"), Some(&json!(1)));
        assert_eq!(report.pointer("/rows/0/count"), Some(&json!(2)));
    }

    #[test]
    fn contact_facet_report_counts_unicode_case_variants_together() {
        let options = ContactFacetsOptions {
            facets: vec![ContactFacetKind::Location],
            top: 10,
            min_count: 1,
            sample_limit: 10,
            include_empty: false,
            flat: false,
        };
        let report = contact_facet_report(
            ContactFacetKind::Location,
            &[
                json!({"id": 1, "location": "MÜNCHEN"}),
                json!({"id": 2, "location": "münchen"}),
            ],
            &options,
            2,
        );

        assert_eq!(report.get("unique_value_count"), Some(&json!(1)));
        assert_eq!(report.pointer("/rows/0/count"), Some(&json!(2)));
    }

    #[test]
    fn contact_pivot_report_counts_case_variants_together() {
        let options = ContactPivotOptions {
            row_facet: ContactFacetKind::Company,
            col_facet: ContactFacetKind::Channel,
            top_rows: 10,
            top_cols: 10,
            min_count: 1,
            sample_limit: 10,
            include_empty: false,
            flat: false,
        };
        let report = contact_pivot_report(
            json!({"type": "input"}),
            vec![
                json!({"id": 1, "organization": "Acme", "emails": ["a@example.invalid"]}),
                json!({"id": 2, "organization": "acme", "emails": ["b@example.invalid"]}),
            ],
            options,
        );

        assert_eq!(report.pointer("/summary/row_value_count"), Some(&json!(1)));
        assert_eq!(report.pointer("/summary/cell_count"), Some(&json!(1)));
        assert_eq!(report.pointer("/rows/0/total_count"), Some(&json!(2)));
        assert_eq!(report.pointer("/rows/0/cells/0/count"), Some(&json!(2)));
    }

    #[test]
    fn dedupe_signal_keys_reads_phones_alias() {
        let contact = json!({
            "phones": ["+1 (555) 123-4567"],
        });

        assert_eq!(
            dedupe_signal_keys(&contact, DedupeSignal::Phone),
            BTreeSet::from(["15551234567".to_string()])
        );
    }

    #[test]
    fn dedupe_contact_summary_reads_phones_alias() {
        let contact = json!({
            "id": 42,
            "phones": ["+1 (555) 123-4567"],
        });

        assert_eq!(
            dedupe_contact_summary(&contact).get("phones"),
            Some(&json!(["15551234567"]))
        );
    }

    #[test]
    fn contact_quality_record_does_not_mark_phones_alias_as_thin() -> Result<()> {
        let contact = json!({
            "id": 42,
            "name": "Ada Lovelace",
            "phones": ["+1 (555) 123-4567"],
        });

        let record = contact_quality_record(&contact, Vec::new())?;

        assert_eq!(record.get("warnings"), Some(&json!([])));
        assert_eq!(
            record
                .get("fields")
                .and_then(|fields| fields.get("phone"))
                .and_then(Value::as_bool),
            Some(true)
        );
        Ok(())
    }

    #[test]
    fn contact_quality_record_marks_empty_channel_fields_as_thin() -> Result<()> {
        let contact = json!({
            "id": 42,
            "name": "Ada Lovelace",
            "email": null,
            "phones": [],
        });

        let record = contact_quality_record(&contact, Vec::new())?;

        assert_eq!(record.get("warnings"), Some(&json!(["thin_search_row"])));
        assert_eq!(
            record
                .get("fields")
                .and_then(|fields| fields.get("email"))
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            record
                .get("fields")
                .and_then(|fields| fields.get("phone"))
                .and_then(Value::as_bool),
            Some(false)
        );
        Ok(())
    }
}
