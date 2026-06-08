use crate::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RouteDoctorProfile {
    Core,
    Moments,
    All,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RouteProbeKind {
    SearchCount,
    ArrayRows,
    MomentDateWindow,
    MomentPaged,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RouteProbeTemplate {
    pub(crate) label: &'static str,
    pub(crate) route: &'static str,
    pub(crate) kind: RouteProbeKind,
}

#[derive(Clone, Debug)]
pub(crate) struct RouteDoctorOptions {
    pub(crate) profile: RouteDoctorProfile,
    pub(crate) route_selectors: Vec<String>,
    pub(crate) start: Option<String>,
    pub(crate) end: Option<String>,
    pub(crate) contact_ids: Vec<u64>,
    pub(crate) limit: usize,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct RouteProbeRequest {
    pub(crate) label: &'static str,
    pub(crate) route: &'static str,
    pub(crate) kind: RouteProbeKind,
    pub(crate) payload: Map<String, Value>,
}

impl RouteDoctorProfile {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "core" => Ok(Self::Core),
            "moments" | "activity" => Ok(Self::Moments),
            "all" => Ok(Self::All),
            other => err(format!(
                "--profile must be core, moments, or all; got {other}"
            )),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Moments => "moments",
            Self::All => "all",
        }
    }
}

impl RouteProbeKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::SearchCount => "search_count",
            Self::ArrayRows => "array_rows",
            Self::MomentDateWindow => "moment_date_window",
            Self::MomentPaged => "moment_paged",
        }
    }
}

impl RouteDoctorOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let options = Self {
            profile: RouteDoctorProfile::parse(
                matches
                    .get_one::<String>("profile")
                    .map(String::as_str)
                    .unwrap_or("core"),
            )?,
            route_selectors: split_list_values(&collect_values(matches, "routes")),
            start: matches.get_one::<String>("start").cloned(),
            end: matches.get_one::<String>("end").cloned(),
            contact_ids: dedupe_ids(optional_ids_from_matches(matches, "contact-ids")?),
            limit: optional_usize_from_matches(matches, "limit")?.unwrap_or(1),
            flat: matches.get_flag("flat"),
        };
        route_doctor_probe_requests(&options)?;
        Ok(options)
    }
}

pub(crate) fn routes_value() -> Value {
    Value::Array(
        command_specs()
            .into_iter()
            .map(|spec| {
                json!({
                    "command": spec.name,
                    "tool_name": spec.tool_name,
                    "route": spec.route_path,
                    "destructive": spec.destructive,
                    "description": spec.description,
                })
            })
            .collect(),
    )
}

pub(crate) async fn routes_doctor(
    runtime: &Runtime,
    options: &RouteDoctorOptions,
) -> Result<Value> {
    let probes = route_doctor_probe_requests(options)?;
    let started_at_unix_ms = now_millis();
    let started = Instant::now();
    let mut probe_rows = Vec::new();
    for probe in &probes {
        probe_rows.push(route_doctor_probe(runtime, probe).await);
    }
    let elapsed_ms = elapsed_millis(started);
    let ok_count = probe_rows
        .iter()
        .filter(|row| row.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .count();
    let error_count = probe_rows.len().saturating_sub(ok_count);

    Ok(json!({
        "source": "live",
        "profile": options.profile.as_str(),
        "started_at_unix_ms": started_at_unix_ms,
        "filters": route_doctor_filters(options),
        "summary": {
            "probe_count": probe_rows.len(),
            "ok_count": ok_count,
            "error_count": error_count,
            "elapsed_ms": elapsed_ms,
        },
        "probes": probe_rows,
    }))
}

pub(crate) async fn route_doctor_probe(runtime: &Runtime, probe: &RouteProbeRequest) -> Value {
    let started = Instant::now();
    match runtime
        .call_tool(probe.route, Value::Object(probe.payload.clone()))
        .await
    {
        Ok(data) => {
            let elapsed_ms = elapsed_millis(started);
            let (row_count, total, has_next) = route_probe_metrics(probe.kind, &data);
            let shape_error = route_probe_shape_error(probe.kind, &data);
            let ok = shape_error.is_none();
            json!({
                "label": probe.label,
                "route": format!("/tools/v2{}", probe.route),
                "route_path": probe.route,
                "kind": probe.kind.as_str(),
                "ok": ok,
                "status": if ok { "ok" } else { "shape_error" },
                "elapsed_ms": elapsed_ms,
                "row_count": row_count,
                "total": total,
                "has_next": has_next,
                "shape": value_shape(&data),
                "payload": Value::Object(probe.payload.clone()),
                "error": shape_error,
            })
        }
        Err(error) => json!({
            "label": probe.label,
            "route": format!("/tools/v2{}", probe.route),
            "route_path": probe.route,
            "kind": probe.kind.as_str(),
            "ok": false,
            "status": "error",
            "elapsed_ms": elapsed_millis(started),
            "row_count": Value::Null,
            "total": Value::Null,
            "has_next": Value::Null,
            "shape": Value::Null,
            "payload": Value::Object(probe.payload.clone()),
            "error": error.to_string(),
        }),
    }
}

pub(crate) fn routes_doctor_dry_run_plan(options: &RouteDoctorOptions) -> Result<Value> {
    let probes = route_doctor_probe_requests(options)?;
    Ok(json!({
        "source": "live",
        "profile": options.profile.as_str(),
        "filters": route_doctor_filters(options),
        "probe_count": probes.len(),
        "probes": probes.iter().map(|probe| json!({
            "label": probe.label,
            "route": format!("/tools/v2{}", probe.route),
            "route_path": probe.route,
            "kind": probe.kind.as_str(),
            "payload": Value::Object(probe.payload.clone()),
            "purpose": route_probe_purpose(probe.kind),
        })).collect::<Vec<_>>(),
    }))
}

pub(crate) fn route_doctor_probe_requests(
    options: &RouteDoctorOptions,
) -> Result<Vec<RouteProbeRequest>> {
    let templates = if options.route_selectors.is_empty() {
        ROUTE_DOCTOR_PROBE_TEMPLATES
            .iter()
            .copied()
            .filter(|template| route_probe_in_profile(*template, options.profile))
            .collect::<Vec<_>>()
    } else {
        route_probe_templates_from_selectors(&options.route_selectors)?
    };

    if templates
        .iter()
        .any(|template| template.kind == RouteProbeKind::MomentDateWindow)
        && (options.start.is_none() || options.end.is_none())
    {
        return err("routes:doctor notes, events, and emails probes require --start and --end");
    }

    templates
        .into_iter()
        .map(|template| {
            Ok(RouteProbeRequest {
                label: template.label,
                route: template.route,
                kind: template.kind,
                payload: route_probe_payload(template, options)?,
            })
        })
        .collect()
}

pub(crate) fn route_probe_templates_from_selectors(
    selectors: &[String],
) -> Result<Vec<RouteProbeTemplate>> {
    let mut templates = Vec::new();
    let mut seen = BTreeSet::new();
    for selector in selectors {
        let template = route_probe_template_for_selector(selector)?;
        if seen.insert(template.label) {
            templates.push(template);
        }
    }
    Ok(templates)
}

pub(crate) fn route_probe_template_for_selector(selector: &str) -> Result<RouteProbeTemplate> {
    ROUTE_DOCTOR_PROBE_TEMPLATES
        .iter()
        .copied()
        .find(|template| route_probe_selector_matches(selector, *template))
        .ok_or_else(|| {
            miette!(
                "unknown read-only probe {selector}; run `mesh routes:doctor --dry-run --profile all --start YYYY-MM-DD --end YYYY-MM-DD` to see known probes"
            )
        })
}

pub(crate) fn route_probe_selector_matches(selector: &str, template: RouteProbeTemplate) -> bool {
    let selector = normalize_route_probe_selector(selector);
    let label = normalize_route_probe_selector(template.label);
    let route = normalize_route_probe_selector(template.route);
    selector == label
        || selector == route
        || route
            .strip_prefix("moments_")
            .is_some_and(|short_route| selector == short_route)
        || route_probe_extra_alias_matches(&selector, template.label)
}

pub(crate) fn route_probe_extra_alias_matches(selector: &str, label: &str) -> bool {
    match label {
        "search_count" => matches!(selector, "search" | "contacts_search" | "search_contacts"),
        "groups" => matches!(selector, "get_groups" | "groups_list" | "list_groups"),
        _ => false,
    }
}

pub(crate) fn normalize_route_probe_selector(value: &str) -> String {
    let mut value = value.trim().to_ascii_lowercase();
    if value == "/tools/v2" {
        value.clear();
    } else if let Some(stripped) = value.strip_prefix("/tools/v2/") {
        value = stripped.to_string();
    }
    value
        .trim_start_matches('/')
        .chars()
        .map(|ch| match ch {
            '/' | '-' | ':' => '_',
            other => other,
        })
        .collect()
}

pub(crate) fn route_probe_in_profile(
    template: RouteProbeTemplate,
    profile: RouteDoctorProfile,
) -> bool {
    match profile {
        RouteDoctorProfile::Core => matches!(
            template.kind,
            RouteProbeKind::SearchCount | RouteProbeKind::ArrayRows
        ),
        RouteDoctorProfile::Moments => {
            matches!(
                template.kind,
                RouteProbeKind::SearchCount
                    | RouteProbeKind::ArrayRows
                    | RouteProbeKind::MomentPaged
            )
        }
        RouteDoctorProfile::All => true,
    }
}

pub(crate) fn route_probe_payload(
    template: RouteProbeTemplate,
    options: &RouteDoctorOptions,
) -> Result<Map<String, Value>> {
    let mut payload = route_probe_base_payload(options);
    match template.kind {
        RouteProbeKind::SearchCount => {
            payload.clear();
            payload.set("limit", 0);
        }
        RouteProbeKind::ArrayRows => {
            payload.clear();
        }
        RouteProbeKind::MomentDateWindow => {
            payload.insert(
                "start".to_string(),
                Value::String(
                    options
                        .start
                        .clone()
                        .ok_or_else(|| miette!("missing --start"))?,
                ),
            );
            payload.insert(
                "end".to_string(),
                Value::String(
                    options
                        .end
                        .clone()
                        .ok_or_else(|| miette!("missing --end"))?,
                ),
            );
        }
        RouteProbeKind::MomentPaged => {
            payload.insert(
                "limit".to_string(),
                Value::Number(Number::from(options.limit as u64)),
            );
            payload.set("page", 1);
        }
    }
    Ok(payload)
}

pub(crate) fn route_probe_base_payload(options: &RouteDoctorOptions) -> Map<String, Value> {
    let mut payload = Map::new();
    if !options.contact_ids.is_empty() {
        payload.set("contact_ids", json!(options.contact_ids));
    }
    payload
}

pub(crate) fn route_probe_metrics(kind: RouteProbeKind, data: &Value) -> (usize, Value, Value) {
    match kind {
        RouteProbeKind::SearchCount => (
            array_len_for_keys(data, &["results", "contacts", "items", "data"]).unwrap_or(0),
            total_from_search(data),
            Value::Null,
        ),
        RouteProbeKind::ArrayRows => (
            array_len_for_keys(data, &["groups", "results", "items", "data"]).unwrap_or(0),
            Value::Null,
            Value::Null,
        ),
        RouteProbeKind::MomentDateWindow | RouteProbeKind::MomentPaged => (
            array_len_for_keys(data, &["results", "items", "data"]).unwrap_or(0),
            Value::Null,
            Value::Bool(moment_response_has_next(data)),
        ),
    }
}

pub(crate) fn route_probe_shape_error(kind: RouteProbeKind, data: &Value) -> Option<&'static str> {
    match kind {
        RouteProbeKind::SearchCount => {
            if search_total_is_number(data) {
                None
            } else {
                Some("search_count response missing numeric total/count")
            }
        }
        RouteProbeKind::ArrayRows => {
            if array_len_for_keys(data, &["groups", "results", "items", "data"]).is_some() {
                None
            } else {
                Some("array_rows response missing array rows")
            }
        }
        RouteProbeKind::MomentDateWindow | RouteProbeKind::MomentPaged => {
            if array_len_for_keys(data, &["results", "items", "data"]).is_some() {
                None
            } else {
                Some("moment response missing array rows")
            }
        }
    }
}

pub(crate) fn search_total_is_number(data: &Value) -> bool {
    data.get("total")
        .or_else(|| data.get("count"))
        .is_some_and(Value::is_number)
}

pub(crate) fn array_len_for_keys(data: &Value, keys: &[&str]) -> Option<usize> {
    match data {
        Value::Array(items) => Some(items.len()),
        Value::Object(object) => keys
            .iter()
            .find_map(|key| object.get(*key).and_then(Value::as_array).map(Vec::len)),
        _ => None,
    }
}

pub(crate) fn route_doctor_filters(options: &RouteDoctorOptions) -> Value {
    json!({
        "routes": options.route_selectors,
        "start": options.start,
        "end": options.end,
        "contact_ids": options.contact_ids,
        "limit": options.limit,
    })
}

pub(crate) fn route_probe_purpose(kind: RouteProbeKind) -> &'static str {
    match kind {
        RouteProbeKind::SearchCount => "check /search count response without fetching contacts",
        RouteProbeKind::ArrayRows => "check list/read route response shape",
        RouteProbeKind::MomentDateWindow => "check date-window moment route response shape",
        RouteProbeKind::MomentPaged => "check page-one moment route response shape",
    }
}

pub(crate) fn route_doctor_flat_rows(data: &Value) -> Value {
    let rows = data
        .get("probes")
        .and_then(Value::as_array)
        .map(|probes| {
            probes
                .iter()
                .filter_map(route_doctor_flat_row)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Value::Array(rows)
}

pub(crate) fn route_doctor_flat_row(value: &Value) -> Option<Value> {
    let object = value.as_object()?;
    let mut row = Map::new();
    for key in [
        "label",
        "route",
        "route_path",
        "kind",
        "ok",
        "status",
        "elapsed_ms",
        "row_count",
        "total",
        "has_next",
        "shape",
        "error",
    ] {
        row.insert(
            key.to_string(),
            object.get(key).cloned().unwrap_or(Value::Null),
        );
    }
    Some(Value::Object(row))
}

pub(crate) fn value_shape(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "boolean".to_string(),
        Value::Number(_) => "number".to_string(),
        Value::String(_) => "string".to_string(),
        Value::Array(items) => format!("array[{}]", items.len()),
        Value::Object(object) => {
            let mut keys = object.keys().take(6).cloned().collect::<Vec<_>>();
            if object.len() > keys.len() {
                keys.push("...".to_string());
            }
            format!("object{{{}}}", keys.join(","))
        }
    }
}

pub(crate) fn schema_value(requested: Option<&str>) -> Result<Value> {
    let specs = command_specs();
    if let Some(name) = requested {
        let spec = specs
            .into_iter()
            .find(|spec| spec.name == name)
            .ok_or_else(|| miette!("unknown command {name}"))?;
        return Ok(command_spec_value(&spec));
    }
    Ok(Value::Array(specs.iter().map(command_spec_value).collect()))
}

pub(crate) fn command_spec_value(spec: &CommandSpec) -> Value {
    json!({
        "name": spec.name,
        "tool_name": spec.tool_name,
        "route_path": spec.route_path,
        "description": spec.description,
        "destructive": spec.destructive,
        "options": spec.options.iter().map(option_spec_value).collect::<Vec<_>>(),
        "nested": spec.nested.iter().map(|group| json!({
            "prefix": group.prefix,
            "suffixes": group.suffixes,
        })).collect::<Vec<_>>(),
    })
}

pub(crate) fn option_spec_value(option: &OptionSpec) -> Value {
    json!({
        "name": option.name,
        "flag": option.flag,
        "kind": option.kind,
        "description": option.description,
        "required": option.required,
        "default": option.default.as_ref().map(DefaultValue::to_json),
        "allowed": option.allowed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(contact_ids: Vec<u64>) -> RouteDoctorOptions {
        RouteDoctorOptions {
            profile: RouteDoctorProfile::All,
            route_selectors: Vec::new(),
            start: Some("2024-04-01".to_string()),
            end: Some("2024-04-30".to_string()),
            contact_ids,
            limit: 25,
            flat: false,
        }
    }

    fn template(kind: RouteProbeKind) -> RouteProbeTemplate {
        RouteProbeTemplate {
            label: "test",
            route: "/moments/test",
            kind,
        }
    }

    #[test]
    fn route_doctor_profile_parses_aliases() -> Result<()> {
        assert_eq!(RouteDoctorProfile::parse("core")?, RouteDoctorProfile::Core);
        assert_eq!(
            RouteDoctorProfile::parse("activity")?,
            RouteDoctorProfile::Moments
        );
        assert_eq!(RouteDoctorProfile::parse("ALL")?, RouteDoctorProfile::All);
        assert!(RouteDoctorProfile::parse("writes").is_err());
        Ok(())
    }

    #[test]
    fn route_probe_base_payload_includes_contact_ids_only_when_present() {
        assert_eq!(
            Value::Object(route_probe_base_payload(&options(vec![42, 7]))),
            json!({"contact_ids": [42, 7]})
        );
        assert_eq!(
            Value::Object(route_probe_base_payload(&options(Vec::new()))),
            json!({})
        );
    }

    #[test]
    fn route_probe_payload_builds_kind_specific_payloads() -> Result<()> {
        let options = options(vec![42]);

        assert_eq!(
            Value::Object(route_probe_payload(
                template(RouteProbeKind::SearchCount),
                &options
            )?),
            json!({"limit": 0})
        );
        assert_eq!(
            Value::Object(route_probe_payload(
                template(RouteProbeKind::ArrayRows),
                &options
            )?),
            json!({})
        );
        assert_eq!(
            Value::Object(route_probe_payload(
                template(RouteProbeKind::MomentDateWindow),
                &options
            )?),
            json!({
                "contact_ids": [42],
                "start": "2024-04-01",
                "end": "2024-04-30",
            })
        );
        assert_eq!(
            Value::Object(route_probe_payload(
                template(RouteProbeKind::MomentPaged),
                &options
            )?),
            json!({
                "contact_ids": [42],
                "limit": 25,
                "page": 1,
            })
        );
        Ok(())
    }

    #[test]
    fn route_probe_selector_normalizes_routes_and_aliases() {
        let groups = route_probe_template_for_selector("groups").expect("groups probe exists");
        let emails_recent = route_probe_template_for_selector("/tools/v2/moments/emails/recent")
            .expect("emails recent probe exists");

        assert_eq!(
            normalize_route_probe_selector("/tools/v2/moments/emails/recent"),
            "moments_emails_recent"
        );
        assert_eq!(
            normalize_route_probe_selector("/tools/v20/moments/emails/recent"),
            "tools_v20_moments_emails_recent"
        );
        assert_eq!(groups.label, "groups");
        assert_eq!(emails_recent.label, "emails_recent");
    }

    #[test]
    fn route_probe_metrics_extracts_counts_by_kind() {
        assert_eq!(
            route_probe_metrics(
                RouteProbeKind::SearchCount,
                &json!({"total": 9, "results": []})
            ),
            (0, json!(9), Value::Null)
        );
        assert_eq!(
            route_probe_metrics(RouteProbeKind::ArrayRows, &json!({"groups": [{"id": 1}]})),
            (1, Value::Null, Value::Null)
        );
        assert_eq!(
            route_probe_metrics(
                RouteProbeKind::MomentPaged,
                &json!({"items": [{"id": 1}], "has_next": true})
            ),
            (1, Value::Null, Value::Bool(true))
        );
    }

    #[test]
    fn route_probe_metrics_does_not_count_bare_object_as_array_rows() {
        assert_eq!(
            route_probe_metrics(RouteProbeKind::ArrayRows, &json!({"error": "wrong shape"})),
            (0, Value::Null, Value::Null)
        );
    }

    #[test]
    fn route_probe_shape_error_rejects_malformed_success_response() {
        assert_eq!(
            route_probe_shape_error(RouteProbeKind::SearchCount, &json!({"total": 0})),
            None
        );
        assert_eq!(
            route_probe_shape_error(RouteProbeKind::ArrayRows, &json!({"groups": []})),
            None
        );
        assert_eq!(
            route_probe_shape_error(RouteProbeKind::MomentPaged, &json!({"items": []})),
            None
        );
        assert_eq!(
            route_probe_shape_error(RouteProbeKind::SearchCount, &json!({"results": []})),
            Some("search_count response missing numeric total/count")
        );
        assert_eq!(
            route_probe_shape_error(RouteProbeKind::ArrayRows, &json!({"error": "wrong shape"})),
            Some("array_rows response missing array rows")
        );
        assert_eq!(
            route_probe_shape_error(
                RouteProbeKind::MomentPaged,
                &json!({"error": "wrong shape"})
            ),
            Some("moment response missing array rows")
        );
    }
}
