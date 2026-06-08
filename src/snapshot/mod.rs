mod archive;
mod create;
mod diff;
mod inspect;
mod query;
mod restore;
use crate::prelude::*;
pub(crate) use archive::*;
pub(crate) use create::*;
pub(crate) use diff::*;
pub(crate) use inspect::*;
pub(crate) use query::*;
pub(crate) use restore::*;

#[derive(Clone, Debug)]
pub(crate) struct SnapshotCreateOptions {
    pub(crate) full_contacts: bool,
    pub(crate) full_contact_ids: Vec<u64>,
    pub(crate) full_limit: Option<usize>,
    pub(crate) full_concurrency: usize,
    pub(crate) moments: bool,
    pub(crate) moments_start: Option<String>,
    pub(crate) moments_end: Option<String>,
    pub(crate) moments_contact_ids: Vec<u64>,
    pub(crate) moments_limit: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SnapshotDiffOptions {
    pub(crate) details: bool,
    pub(crate) detail_limit: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SnapshotVerifyArchiveOptions {
    pub(crate) require_index: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotCatalogOptions {
    pub(crate) root: PathBuf,
    pub(crate) recursive: bool,
    pub(crate) max_depth: Option<usize>,
    pub(crate) limit: Option<usize>,
    pub(crate) include_snapshots: bool,
    pub(crate) include_archives: bool,
    pub(crate) verify: bool,
    pub(crate) doctor: bool,
    pub(crate) require_index: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotPruneOptions {
    pub(crate) root: PathBuf,
    pub(crate) recursive: bool,
    pub(crate) max_depth: Option<usize>,
    pub(crate) limit: Option<usize>,
    pub(crate) include_snapshots: bool,
    pub(crate) include_archives: bool,
    pub(crate) keep_latest: Option<usize>,
    pub(crate) older_than_days: Option<usize>,
    pub(crate) failed: bool,
    pub(crate) require_index: bool,
    pub(crate) dry_run: bool,
    pub(crate) yes: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotHistoryOptions {
    pub(crate) root: PathBuf,
    pub(crate) recursive: bool,
    pub(crate) max_depth: Option<usize>,
    pub(crate) limit: Option<usize>,
    pub(crate) section: SnapshotQuerySection,
    pub(crate) ids: Vec<u64>,
    pub(crate) verify: bool,
    pub(crate) index: SnapshotIndexMode,
    pub(crate) details: bool,
    pub(crate) detail_limit: usize,
    pub(crate) records: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotFindOptions {
    pub(crate) root: PathBuf,
    pub(crate) recursive: bool,
    pub(crate) max_depth: Option<usize>,
    pub(crate) snapshot_limit: Option<usize>,
    pub(crate) sections: Vec<SnapshotQuerySection>,
    pub(crate) ids: Vec<u64>,
    pub(crate) contains: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) verify: bool,
    pub(crate) index: SnapshotIndexMode,
    pub(crate) records: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotTimelineOptions {
    pub(crate) root: PathBuf,
    pub(crate) recursive: bool,
    pub(crate) max_depth: Option<usize>,
    pub(crate) limit: Option<usize>,
    pub(crate) changes_only: bool,
    pub(crate) diffs: bool,
    pub(crate) diff: SnapshotDiffOptions,
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotDriftOptions {
    pub(crate) dir: PathBuf,
    pub(crate) page_size: usize,
    pub(crate) compare_groups: bool,
    pub(crate) full_contact_ids: Vec<u64>,
    pub(crate) full_concurrency: usize,
    pub(crate) verify: bool,
    pub(crate) diff: SnapshotDiffOptions,
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotReportOptions {
    pub(crate) dir: PathBuf,
    pub(crate) root: PathBuf,
    pub(crate) recursive: bool,
    pub(crate) max_depth: Option<usize>,
    pub(crate) limit: Option<usize>,
    pub(crate) neighbors: usize,
    pub(crate) diffs: bool,
    pub(crate) include_drift: bool,
    pub(crate) drift: SnapshotDriftOptions,
    pub(crate) stats: SnapshotStatsOptions,
    pub(crate) doctor: SnapshotDoctorOptions,
    pub(crate) diff: SnapshotDiffOptions,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SnapshotReportStatus {
    pub(crate) verify_ok: bool,
    pub(crate) stats_ok: bool,
    pub(crate) doctor_ok: bool,
    pub(crate) neighbors_ok: bool,
    pub(crate) drift_included: bool,
    pub(crate) drift_ok: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotStatsOptions {
    pub(crate) top: usize,
    pub(crate) verify: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotDoctorOptions {
    pub(crate) top: usize,
    pub(crate) verify: bool,
    pub(crate) require_index: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotPackOptions {
    pub(crate) verify: bool,
    pub(crate) include_indexes: bool,
    pub(crate) compression_level: i32,
    pub(crate) force: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotPackageEntry {
    pub(crate) archive_path: PathBuf,
    pub(crate) source_path: PathBuf,
    pub(crate) kind: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotArchiveFile {
    pub(crate) bytes: u64,
    pub(crate) sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapshotCatalogCandidateKind {
    Snapshot,
    Archive,
}

impl SnapshotCatalogCandidateKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Snapshot => "snapshot",
            Self::Archive => "archive",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotCatalogCandidate {
    pub(crate) kind: SnapshotCatalogCandidateKind,
    pub(crate) path: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotPruneAction {
    pub(crate) kind: SnapshotCatalogCandidateKind,
    pub(crate) path: PathBuf,
    pub(crate) delete: bool,
    pub(crate) reasons: Vec<String>,
    pub(crate) rank_time_unix_ms: Option<u64>,
    pub(crate) rank_time_source: &'static str,
    pub(crate) bytes: Option<u64>,
    pub(crate) health: Option<Value>,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotHistoryState {
    pub(crate) seen: bool,
    pub(crate) present: bool,
    pub(crate) hash: Option<String>,
    pub(crate) record: Option<Value>,
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotHistorySnapshot {
    pub(crate) dir: PathBuf,
    pub(crate) created_at_unix_ms: Option<u64>,
    pub(crate) rank_time_unix_ms: Option<u64>,
    pub(crate) rank_time_source: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotIndexOptions {
    pub(crate) section: SnapshotQuerySection,
    pub(crate) force: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SnapshotQuerySection {
    pub(crate) label: &'static str,
    pub(crate) file_label: &'static str,
    pub(crate) file_name: &'static str,
    pub(crate) kind: SnapshotQuerySectionKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapshotQuerySectionKind {
    Jsonl,
    JsonArray,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapshotIndexMode {
    Auto,
    Off,
    Require,
}

impl SnapshotIndexMode {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "auto" => Ok(Self::Auto),
            "off" => Ok(Self::Off),
            "require" => Ok(Self::Require),
            other => err(format!(
                "--index must be auto, off, or require; got {other}"
            )),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Off => "off",
            Self::Require => "require",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotQueryOptions {
    pub(crate) section: SnapshotQuerySection,
    pub(crate) ids: Vec<u64>,
    pub(crate) contains: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) verify: bool,
    pub(crate) index: SnapshotIndexMode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SnapshotIndex {
    pub(crate) schema: String,
    pub(crate) meshx_version: String,
    pub(crate) created_at_unix_ms: u64,
    pub(crate) section: String,
    pub(crate) file: SnapshotIndexFile,
    pub(crate) record_count: u64,
    pub(crate) indexed_count: u64,
    pub(crate) skipped_without_id: u64,
    pub(crate) entries: BTreeMap<String, SnapshotIndexEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct SnapshotIndexFile {
    pub(crate) path: String,
    pub(crate) bytes: u64,
    pub(crate) sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SnapshotIndexEntry {
    pub(crate) offset: u64,
    pub(crate) bytes: u64,
    pub(crate) line: u64,
}

#[derive(Default)]
pub(crate) struct SnapshotStatsAccumulator {
    pub(crate) rows: u64,
    pub(crate) rows_with_id: u64,
    pub(crate) missing_id: u64,
    pub(crate) ids: BTreeSet<String>,
    pub(crate) duplicate_ids: BTreeMap<String, u64>,
    pub(crate) key_counts: BTreeMap<String, u64>,
    pub(crate) field_counts: BTreeMap<&'static str, u64>,
    pub(crate) email_domains: BTreeMap<String, u64>,
}

pub(crate) struct SnapshotMomentsResult {
    pub(crate) files: Map<String, Value>,
    pub(crate) counts: Map<String, Value>,
    pub(crate) routes: Vec<Value>,
}

pub(crate) struct SnapshotMomentWrite {
    pub(crate) label: &'static str,
    pub(crate) file: Value,
    pub(crate) count: usize,
    pub(crate) meta: Value,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SnapshotMomentKind {
    DateWindow,
    Paged,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SnapshotMomentRoute {
    pub(crate) label: &'static str,
    pub(crate) file_name: &'static str,
    pub(crate) route: &'static str,
    pub(crate) kind: SnapshotMomentKind,
}

impl SnapshotCreateOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let full_contact_ids = optional_ids_from_matches(matches, "full-contact-ids")?;
        let full_limit = optional_usize_from_matches(matches, "full-limit")?;
        let full_concurrency = contact_fetch_concurrency(matches, "full-concurrency")?;
        let full_contacts = matches.get_flag("full-contacts")
            || !full_contact_ids.is_empty()
            || full_limit.is_some();
        let moments_start = matches.get_one::<String>("moments-start").cloned();
        let moments_end = matches.get_one::<String>("moments-end").cloned();
        let moments_contact_ids = optional_ids_from_matches(matches, "moments-contact-ids")?;
        let moments = matches.get_flag("moments")
            || moments_start.is_some()
            || moments_end.is_some()
            || !moments_contact_ids.is_empty();
        if moments && (moments_start.is_none() || moments_end.is_none()) {
            return err("snapshot:create --moments requires --moments-start and --moments-end");
        }
        let moments_limit = optional_usize_from_matches(matches, "moments-limit")?
            .unwrap_or(MOMENT_PAGE_SIZE_DEFAULT);
        Ok(Self {
            full_contacts,
            full_contact_ids,
            full_limit,
            full_concurrency,
            moments,
            moments_start,
            moments_end,
            moments_contact_ids,
            moments_limit,
        })
    }
}

impl SnapshotDiffOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let detail_limit = optional_usize_from_matches(matches, "detail-limit")?;
        Ok(Self {
            details: matches.get_flag("details") || detail_limit.is_some(),
            detail_limit: detail_limit.unwrap_or(SNAPSHOT_DIFF_DETAIL_LIMIT_DEFAULT),
        })
    }
}

impl SnapshotVerifyArchiveOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Self {
        Self {
            require_index: matches.get_flag("require-index"),
        }
    }
}

impl SnapshotCatalogOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let snapshots = matches.get_flag("snapshots");
        let archives = matches.get_flag("archives");
        let require_index = matches.get_flag("require-index");
        let verify = matches.get_flag("verify");
        let doctor = matches.get_flag("doctor");
        if require_index && !verify && !doctor {
            return err("--require-index only applies with --verify or --doctor");
        }
        Ok(Self {
            root: PathBuf::from(
                matches
                    .get_one::<String>("root")
                    .expect("defaulted by clap"),
            ),
            recursive: matches.get_flag("recursive"),
            max_depth: optional_positive_usize_from_matches(matches, "max-depth")?,
            limit: optional_positive_usize_from_matches(matches, "limit")?,
            include_snapshots: snapshots || !archives,
            include_archives: archives || !snapshots,
            verify,
            doctor,
            require_index,
        })
    }
}

impl SnapshotPruneOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let snapshots = matches.get_flag("snapshots");
        let archives = matches.get_flag("archives");
        let keep_latest = optional_nonnegative_usize_from_matches(matches, "keep-latest")?;
        let older_than_days = optional_positive_usize_from_matches(matches, "older-than-days")?;
        let failed = matches.get_flag("failed");
        let require_index = matches.get_flag("require-index");
        if keep_latest.is_none() && older_than_days.is_none() && !failed {
            return err(
                "snapshot:prune needs at least one criterion: --keep-latest, --older-than-days, or --failed",
            );
        }
        if require_index && !failed {
            return err("--require-index only applies with --failed");
        }
        Ok(Self {
            root: PathBuf::from(
                matches
                    .get_one::<String>("root")
                    .expect("defaulted by clap"),
            ),
            recursive: matches.get_flag("recursive"),
            max_depth: optional_positive_usize_from_matches(matches, "max-depth")?,
            limit: optional_positive_usize_from_matches(matches, "limit")?,
            include_snapshots: snapshots || !archives,
            include_archives: archives || !snapshots,
            keep_latest,
            older_than_days,
            failed,
            require_index,
            dry_run: matches.get_flag("dry-run"),
            yes: matches.get_flag("yes"),
        })
    }
}

impl SnapshotHistoryOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let ids = optional_ids_from_matches(matches, "ids")?;
        if ids.is_empty() {
            return err("snapshot:history needs at least one --ids value");
        }
        let detail_limit = optional_usize_from_matches(matches, "detail-limit")?;
        Ok(Self {
            root: PathBuf::from(
                matches
                    .get_one::<String>("root")
                    .expect("defaulted by clap"),
            ),
            recursive: matches.get_flag("recursive"),
            max_depth: optional_positive_usize_from_matches(matches, "max-depth")?,
            limit: optional_positive_usize_from_matches(matches, "limit")?,
            section: snapshot_query_section(
                matches
                    .get_one::<String>("section")
                    .map(String::as_str)
                    .unwrap_or("contacts"),
            )?,
            ids,
            verify: !matches.get_flag("skip-verify"),
            index: SnapshotIndexMode::parse(
                matches
                    .get_one::<String>("index")
                    .map(String::as_str)
                    .unwrap_or("auto"),
            )?,
            details: matches.get_flag("details") || detail_limit.is_some(),
            detail_limit: detail_limit.unwrap_or(SNAPSHOT_DIFF_DETAIL_LIMIT_DEFAULT),
            records: matches.get_flag("records"),
        })
    }
}

impl SnapshotFindOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let ids = optional_ids_from_matches(matches, "ids")?;
        let contains = matches.get_one::<String>("contains").cloned();
        if ids.is_empty() && contains.is_none() {
            return err("snapshot:find needs --ids or --contains");
        }
        Ok(Self {
            root: PathBuf::from(
                matches
                    .get_one::<String>("root")
                    .expect("defaulted by clap"),
            ),
            recursive: matches.get_flag("recursive"),
            max_depth: optional_positive_usize_from_matches(matches, "max-depth")?,
            snapshot_limit: optional_positive_usize_from_matches(matches, "snapshot-limit")?,
            sections: snapshot_sections_from_matches(matches, "section")?,
            ids,
            contains,
            limit: optional_positive_usize_from_matches(matches, "limit")?,
            verify: !matches.get_flag("skip-verify"),
            index: SnapshotIndexMode::parse(
                matches
                    .get_one::<String>("index")
                    .map(String::as_str)
                    .unwrap_or("auto"),
            )?,
            records: matches.get_flag("records"),
        })
    }
}

impl SnapshotTimelineOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let diff = SnapshotDiffOptions::from_matches(matches)?;
        Ok(Self {
            root: PathBuf::from(
                matches
                    .get_one::<String>("root")
                    .expect("defaulted by clap"),
            ),
            recursive: matches.get_flag("recursive"),
            max_depth: optional_positive_usize_from_matches(matches, "max-depth")?,
            limit: optional_positive_usize_from_matches(matches, "limit")?,
            changes_only: matches.get_flag("changes-only"),
            diffs: matches.get_flag("diffs") || diff.details,
            diff,
        })
    }
}

impl SnapshotDriftOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let diff = SnapshotDiffOptions::from_matches(matches)?;
        Ok(Self {
            dir: PathBuf::from(matches.get_one::<String>("dir").expect("required by clap")),
            page_size: optional_usize_from_matches(matches, "page-size")?
                .unwrap_or(SEARCH_LIMIT_MAX),
            compare_groups: !matches.get_flag("skip-groups"),
            full_contact_ids: dedupe_ids(optional_ids_from_matches(matches, "full-contact-ids")?),
            full_concurrency: contact_fetch_concurrency(matches, "full-concurrency")?,
            verify: !matches.get_flag("skip-verify"),
            diff,
        })
    }
}

impl SnapshotReportOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let dir = PathBuf::from(matches.get_one::<String>("dir").expect("required by clap"));
        let root = matches
            .get_one::<String>("root")
            .map(PathBuf::from)
            .or_else(|| dir.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        let diff = SnapshotDiffOptions::from_matches(matches)?;
        let full_contact_ids = dedupe_ids(optional_ids_from_matches(matches, "full-contact-ids")?);
        let include_drift = matches.get_flag("drift") || !full_contact_ids.is_empty();
        let drift = SnapshotDriftOptions {
            dir: dir.clone(),
            page_size: optional_usize_from_matches(matches, "page-size")?
                .unwrap_or(SEARCH_LIMIT_MAX),
            compare_groups: !matches.get_flag("skip-groups"),
            full_contact_ids,
            full_concurrency: contact_fetch_concurrency(matches, "full-concurrency")?,
            verify: !matches.get_flag("skip-verify"),
            diff,
        };
        Ok(Self {
            dir,
            root,
            recursive: matches.get_flag("recursive"),
            max_depth: optional_positive_usize_from_matches(matches, "max-depth")?,
            limit: optional_positive_usize_from_matches(matches, "limit")?,
            neighbors: optional_nonnegative_usize_from_matches(matches, "neighbors")?.unwrap_or(1),
            diffs: matches.get_flag("diffs") || diff.details,
            include_drift,
            drift,
            stats: SnapshotStatsOptions::from_matches(matches)?,
            doctor: SnapshotDoctorOptions::from_matches(matches)?,
            diff,
        })
    }
}

impl SnapshotStatsOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        Ok(Self {
            top: optional_positive_usize_from_matches(matches, "top")?
                .unwrap_or(SNAPSHOT_STATS_TOP_DEFAULT),
            verify: !matches.get_flag("skip-verify"),
        })
    }
}

impl SnapshotDoctorOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        Ok(Self {
            top: optional_positive_usize_from_matches(matches, "top")?
                .unwrap_or(SNAPSHOT_STATS_TOP_DEFAULT),
            verify: !matches.get_flag("skip-verify"),
            require_index: matches.get_flag("require-index"),
        })
    }
}

impl SnapshotPackOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        Ok(Self {
            verify: !matches.get_flag("skip-verify"),
            include_indexes: !matches.get_flag("no-index"),
            compression_level: snapshot_pack_compression_level(matches)?,
            force: matches.get_flag("force"),
        })
    }
}

impl SnapshotIndexOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let section = snapshot_query_section(
            matches
                .get_one::<String>("section")
                .map(String::as_str)
                .unwrap_or("contacts"),
        )?;
        if section.kind != SnapshotQuerySectionKind::Jsonl {
            return err(format!(
                "snapshot:index only supports JSONL sections; {} is stored as {}",
                section.label, section.file_name
            ));
        }
        Ok(Self {
            section,
            force: matches.get_flag("force"),
        })
    }
}

impl SnapshotQueryOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let section = snapshot_query_section(
            matches
                .get_one::<String>("section")
                .map(String::as_str)
                .unwrap_or("contacts"),
        )?;
        Ok(Self {
            section,
            ids: optional_ids_from_matches(matches, "ids")?,
            contains: matches.get_one::<String>("contains").cloned(),
            limit: optional_positive_usize_from_matches(matches, "limit")?,
            verify: !matches.get_flag("skip-verify"),
            index: SnapshotIndexMode::parse(
                matches
                    .get_one::<String>("index")
                    .map(String::as_str)
                    .unwrap_or("auto"),
            )?,
        })
    }
}

pub(crate) fn snapshot_full_contact_ids(
    options: &SnapshotCreateOptions,
    contact_ids: &[u64],
) -> Result<Vec<u64>> {
    let mut ids = if options.full_contact_ids.is_empty() {
        contact_ids.to_vec()
    } else {
        options.full_contact_ids.clone()
    };
    ids = dedupe_ids(ids);
    if let Some(limit) = options.full_limit {
        ids.truncate(limit);
    }
    if ids.is_empty() {
        return err("full contact snapshot requested but no contact IDs were available");
    }
    Ok(ids)
}

pub(crate) fn prepare_snapshot_dir(dir: &Path, force: bool) -> Result<()> {
    if dir.exists() {
        if !force {
            return err(format!(
                "{} already exists. Use --force only for an existing empty directory.",
                dir.display()
            ));
        }
        let mut entries = fs::read_dir(dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {}", dir.display()))?;
        if entries.next().transpose().into_diagnostic()?.is_some() {
            return err(format!("{} exists and is not empty", dir.display()));
        }
    } else {
        fs::create_dir_all(dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("creating {}", dir.display()))?;
    }
    Ok(())
}

pub(crate) fn snapshot_file_entry(name: &str, bytes: usize, digest: impl AsRef<[u8]>) -> Value {
    json!({
        "path": name,
        "bytes": bytes,
        "sha256": hex::encode(digest.as_ref()),
    })
}

pub(crate) fn verify_snapshot(dir: &Path) -> Result<Value> {
    let manifest = read_snapshot_manifest(dir)?;
    let Some(files) = manifest.get("files").and_then(Value::as_object) else {
        return err("snapshot manifest does not contain files object");
    };

    let mut checked = Vec::new();
    let mut failures = Vec::new();
    for (label, entry) in files {
        let path = entry
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| miette!("manifest file entry {label} is missing path"))?;
        let expected_hash = entry
            .get("sha256")
            .and_then(Value::as_str)
            .ok_or_else(|| miette!("manifest file entry {label} is missing sha256"))?;
        let expected_bytes = entry
            .get("bytes")
            .and_then(Value::as_u64)
            .ok_or_else(|| miette!("manifest file entry {label} is missing bytes"))?;
        let file_path = safe_snapshot_file_path(dir, path)?;
        let bytes = fs::read(&file_path)
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {}", file_path.display()))?;
        let actual_hash = sha256_hex(&bytes);
        let actual_bytes = bytes.len() as u64;
        let ok = actual_hash == expected_hash && actual_bytes == expected_bytes;
        if !ok {
            failures.push(json!({
                "label": label,
                "path": path,
                "expected_sha256": expected_hash,
                "actual_sha256": actual_hash,
                "expected_bytes": expected_bytes,
                "actual_bytes": actual_bytes,
            }));
        }
        checked.push(json!({
            "label": label,
            "path": path,
            "ok": ok,
        }));
    }
    Ok(json!({
        "dir": dir.display().to_string(),
        "ok": failures.is_empty(),
        "checked": checked,
        "failures": failures,
    }))
}

pub(crate) fn summary_bool(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub(crate) fn value_u64(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or_default()
}

pub(crate) fn read_snapshot_manifest(dir: &Path) -> Result<Value> {
    let path = dir.join("manifest.json");
    let text = fs::read_to_string(&path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text)
        .into_diagnostic()
        .wrap_err_with(|| format!("parsing {}", path.display()))
}

impl SnapshotStatsAccumulator {
    pub(crate) fn add_row(&mut self, row: &Value) {
        self.rows += 1;
        match record_id(row) {
            Some(id) => {
                self.rows_with_id += 1;
                if !self.ids.insert(id.clone()) {
                    *self.duplicate_ids.entry(id).or_insert(1) += 1;
                }
            }
            None => self.missing_id += 1,
        }

        if let Value::Object(object) = row {
            for key in object.keys() {
                *self.key_counts.entry(key.clone()).or_default() += 1;
            }
        }

        for (field, aliases) in SNAPSHOT_STATS_FIELDS {
            if row_has_any_data(row, aliases) {
                *self.field_counts.entry(*field).or_default() += 1;
            }
        }

        for domain in email_domains_from_row(row) {
            *self.email_domains.entry(domain).or_default() += 1;
        }
    }

    pub(crate) fn finish(
        self,
        section: SnapshotQuerySection,
        storage: &str,
        entry: &Value,
        top: usize,
    ) -> Result<Value> {
        Ok(json!({
            "file": snapshot_file_fingerprint(entry)?,
            "section": section.label,
            "storage": storage,
            "rows": self.rows,
            "ids": {
                "rows_with_id": self.rows_with_id,
                "missing_id": self.missing_id,
                "unique_ids": self.ids.len(),
                "duplicate_id_count": self.duplicate_ids.len(),
                "duplicate_ids": top_count_entries(&self.duplicate_ids, top),
            },
            "field_coverage": field_coverage_value(&self.field_counts, self.rows),
            "top_keys": top_count_entries(&self.key_counts, top),
            "top_email_domains": top_count_entries(&self.email_domains, top),
        }))
    }
}

pub(crate) fn field_coverage_value(counts: &BTreeMap<&'static str, u64>, rows: u64) -> Value {
    let mut coverage = Map::new();
    for (field, _) in SNAPSHOT_STATS_FIELDS {
        let count = counts.get(field).copied().unwrap_or_default();
        let percent = if rows == 0 {
            0.0
        } else {
            (count as f64) * 100.0 / (rows as f64)
        };
        coverage.insert(
            (*field).to_string(),
            json!({
                "count": count,
                "percent": percent,
            }),
        );
    }
    Value::Object(coverage)
}

pub(crate) fn top_count_entries(counts: &BTreeMap<String, u64>, limit: usize) -> Value {
    let mut entries = counts.iter().collect::<Vec<_>>();
    entries.sort_by(|(left_key, left_count), (right_key, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| left_key.cmp(right_key))
    });
    Value::Array(
        entries
            .into_iter()
            .take(limit)
            .map(|(value, count)| {
                json!({
                    "value": value,
                    "count": count,
                })
            })
            .collect(),
    )
}

pub(crate) fn row_has_any_data(row: &Value, aliases: &[&str]) -> bool {
    let Value::Object(object) = row else {
        return false;
    };
    aliases
        .iter()
        .any(|alias| object.get(*alias).is_some_and(value_has_meaningful_data))
}

pub(crate) fn value_has_meaningful_data(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(_) => true,
        Value::String(value) => !value.trim().is_empty(),
        Value::Array(items) => items.iter().any(value_has_meaningful_data),
        Value::Object(object) => object.values().any(value_has_meaningful_data),
    }
}

pub(crate) fn email_domains_from_row(row: &Value) -> BTreeSet<String> {
    let mut strings = Vec::new();
    collect_strings(row, &mut strings);
    strings
        .into_iter()
        .filter_map(|value| email_domain_from_string(&value))
        .collect()
}

pub(crate) fn collect_strings(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::String(value) => output.push(value.clone()),
        Value::Array(items) => {
            for item in items {
                collect_strings(item, output);
            }
        }
        Value::Object(object) => {
            for value in object.values() {
                collect_strings(value, output);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

pub(crate) fn email_domain_from_string(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_matches(|ch: char| {
        ch == '<' || ch == '>' || ch == '"' || ch == '\'' || ch == ',' || ch == ';'
    });
    let (_, domain) = trimmed.rsplit_once('@')?;
    let domain = domain
        .trim_start_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '.' && ch != '-')
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '.' || *ch == '-')
        .collect::<String>()
        .to_ascii_lowercase();
    if valid_email_domain(&domain) {
        Some(domain)
    } else {
        None
    }
}

fn valid_email_domain(domain: &str) -> bool {
    domain.contains('.')
        && domain.chars().any(|ch| ch.is_ascii_alphabetic())
        && domain.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        })
}

pub(crate) fn read_snapshot_section_values(
    dir: &Path,
    section: SnapshotQuerySection,
) -> Result<Vec<Value>> {
    let path = snapshot_manifest_file_path(dir, section.file_label)?;
    match section.kind {
        SnapshotQuerySectionKind::Jsonl => read_snapshot_jsonl_values_at_path(&path),
        SnapshotQuerySectionKind::JsonArray => read_snapshot_array_values_at_path(&path),
    }
}

pub(crate) fn read_snapshot_array_values_at_path(path: &Path) -> Result<Vec<Value>> {
    let text = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .into_diagnostic()
        .wrap_err_with(|| format!("parsing {}", path.display()))?;
    value
        .as_array()
        .cloned()
        .ok_or_else(|| miette!("{} must be a JSON array", path.display()))
}

pub(crate) fn snapshot_sections_from_matches(
    matches: &ArgMatches,
    flag: &str,
) -> Result<Vec<SnapshotQuerySection>> {
    let raw = split_list_values(&collect_values(matches, flag));
    if raw.is_empty() {
        return Ok(snapshot_all_query_sections());
    }
    let mut seen = BTreeSet::new();
    let mut sections = Vec::new();
    for value in raw {
        let section = snapshot_query_section(&value)?;
        if seen.insert(section.label) {
            sections.push(section);
        }
    }
    Ok(sections)
}

pub(crate) fn read_snapshot_jsonl_values_at_path(path: &Path) -> Result<Vec<Value>> {
    let text = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", path.display()))?;
    let mut values = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        values.push(
            serde_json::from_str(line)
                .into_diagnostic()
                .wrap_err_with(|| format!("parsing {} line {}", path.display(), index + 1))?,
        );
    }
    Ok(values)
}

pub(crate) fn read_snapshot_jsonl_records_at_path(path: &Path) -> Result<BTreeMap<String, String>> {
    let text = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", path.display()))?;
    let mut records = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .into_diagnostic()
            .wrap_err_with(|| format!("parsing {} line {}", path.display(), index + 1))?;
        if let Some(id) = record_id(&value) {
            insert_snapshot_record(
                &mut records,
                id,
                record_hash(&value)?,
                path,
                format!("line {}", index + 1),
            )?;
        }
    }
    Ok(records)
}

pub(crate) fn read_snapshot_jsonl_record_values_at_path(
    path: &Path,
) -> Result<BTreeMap<String, Value>> {
    let text = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", path.display()))?;
    let mut records = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .into_diagnostic()
            .wrap_err_with(|| format!("parsing {} line {}", path.display(), index + 1))?;
        if let Some(id) = record_id(&value) {
            insert_snapshot_record(&mut records, id, value, path, format!("line {}", index + 1))?;
        }
    }
    Ok(records)
}

pub(crate) fn read_snapshot_array_records_at_path(path: &Path) -> Result<BTreeMap<String, String>> {
    let text = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .into_diagnostic()
        .wrap_err_with(|| format!("parsing {}", path.display()))?;
    let mut records = BTreeMap::new();
    let rows = value
        .as_array()
        .ok_or_else(|| miette!("{} must be a JSON array", path.display()))?;
    for (index, value) in rows.iter().enumerate() {
        if let Some(id) = record_id(value) {
            insert_snapshot_record(
                &mut records,
                id,
                record_hash(value)?,
                path,
                format!("row {}", index + 1),
            )?;
        }
    }
    Ok(records)
}

pub(crate) fn read_snapshot_array_record_values_at_path(
    path: &Path,
) -> Result<BTreeMap<String, Value>> {
    let text = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .into_diagnostic()
        .wrap_err_with(|| format!("parsing {}", path.display()))?;
    let mut records = BTreeMap::new();
    let rows = value
        .as_array()
        .ok_or_else(|| miette!("{} must be a JSON array", path.display()))?;
    for (index, value) in rows.iter().enumerate() {
        if let Some(id) = record_id(value) {
            insert_snapshot_record(
                &mut records,
                id,
                value.clone(),
                path,
                format!("row {}", index + 1),
            )?;
        }
    }
    Ok(records)
}

pub(crate) fn insert_snapshot_record<T>(
    records: &mut BTreeMap<String, T>,
    id: String,
    value: T,
    path: &Path,
    location: String,
) -> Result<()> {
    if records.contains_key(&id) {
        return err(format!(
            "{} contains duplicate record ID {} at {}",
            path.display(),
            id,
            location
        ));
    }
    records.insert(id, value);
    Ok(())
}

pub(crate) fn snapshot_manifest_file_entry(dir: &Path, label: &str) -> Result<Option<Value>> {
    let manifest = read_snapshot_manifest(dir)?;
    Ok(manifest
        .get("files")
        .and_then(Value::as_object)
        .and_then(|files| files.get(label))
        .cloned())
}

pub(crate) fn snapshot_manifest_file_path(dir: &Path, label: &str) -> Result<PathBuf> {
    let entry = snapshot_manifest_file_entry(dir, label)?
        .ok_or_else(|| miette!("snapshot does not contain file {label}"))?;
    let path = entry
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| miette!("snapshot file entry {label} is missing path"))?;
    safe_snapshot_file_path(dir, path)
}

pub(crate) fn snapshot_file_fingerprint(entry: &Value) -> Result<Value> {
    let path = entry
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| miette!("snapshot file entry is missing path"))?;
    let bytes = entry
        .get("bytes")
        .and_then(Value::as_u64)
        .ok_or_else(|| miette!("snapshot file entry {path} is missing bytes"))?;
    let sha256 = entry
        .get("sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| miette!("snapshot file entry {path} is missing sha256"))?;
    Ok(json!({
        "path": path,
        "bytes": bytes,
        "sha256": sha256,
    }))
}

pub(crate) fn snapshot_manifest_has_file(dir: &Path, label: &str) -> Result<bool> {
    Ok(snapshot_manifest_file_entry(dir, label)?.is_some())
}

pub(crate) fn collect_value_changes(
    old: &Value,
    new: &Value,
    path: &str,
    changes: &mut Vec<Value>,
    limit: usize,
) {
    if old == new || changes.len() >= limit {
        return;
    }
    match (old, new) {
        (Value::Object(old_object), Value::Object(new_object)) => {
            let keys = old_object
                .keys()
                .chain(new_object.keys())
                .cloned()
                .collect::<BTreeSet<_>>();
            for key in keys {
                if changes.len() >= limit {
                    break;
                }
                let next_path = json_pointer_child(path, &key);
                match (old_object.get(&key), new_object.get(&key)) {
                    (Some(old_value), Some(new_value)) => {
                        collect_value_changes(old_value, new_value, &next_path, changes, limit);
                    }
                    (Some(old_value), None) => changes.push(json!({
                        "path": next_path,
                        "kind": "removed",
                        "old": diff_preview_value(old_value),
                        "new": Value::Null,
                    })),
                    (None, Some(new_value)) => changes.push(json!({
                        "path": next_path,
                        "kind": "added",
                        "old": Value::Null,
                        "new": diff_preview_value(new_value),
                    })),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(old_array), Value::Array(new_array)) => {
            if old_array.len() != new_array.len() && changes.len() < limit {
                changes.push(json!({
                    "path": json_pointer_child(path, "length"),
                    "kind": "changed",
                    "old": old_array.len(),
                    "new": new_array.len(),
                }));
            }
            for index in 0..old_array.len().min(new_array.len()) {
                if changes.len() >= limit {
                    break;
                }
                collect_value_changes(
                    &old_array[index],
                    &new_array[index],
                    &json_pointer_child(path, &index.to_string()),
                    changes,
                    limit,
                );
            }
        }
        _ => {
            changes.push(json!({
                "path": if path.is_empty() { "/" } else { path },
                "kind": "changed",
                "old": diff_preview_value(old),
                "new": diff_preview_value(new),
            }));
        }
    }
}

pub(crate) fn json_pointer_child(parent: &str, segment: &str) -> String {
    let escaped = segment.replace('~', "~0").replace('/', "~1");
    if parent.is_empty() {
        format!("/{escaped}")
    } else {
        format!("{parent}/{escaped}")
    }
}

pub(crate) fn record_id(value: &Value) -> Option<String> {
    let id = value.get("id")?;
    match id {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

pub(crate) fn contact_id_from_value(value: &Value) -> Option<u64> {
    record_id(value)?.parse().ok()
}

pub(crate) fn record_hash(value: &Value) -> Result<String> {
    let bytes = serde_json::to_vec(value)
        .into_diagnostic()
        .wrap_err("serializing snapshot record for hashing")?;
    Ok(sha256_hex(&bytes))
}

pub(crate) fn safe_snapshot_file_path(dir: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return err(format!(
            "snapshot file path must stay inside snapshot dir: {relative}"
        ));
    }
    Ok(dir.join(path))
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

    fn temp_snapshot_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "meshx-{label}-{}-{}",
            std::process::id(),
            NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn json_pointer_child_escapes_pointer_segments() {
        assert_eq!(json_pointer_child("", "a/b~c"), "/a~1b~0c");
        assert_eq!(json_pointer_child("/root", "a/b~c"), "/root/a~1b~0c");
    }

    #[test]
    fn record_id_accepts_string_and_number_ids() {
        assert_eq!(record_id(&json!({"id": "c-1"})), Some("c-1".to_string()));
        assert_eq!(record_id(&json!({"id": 42})), Some("42".to_string()));
        assert_eq!(record_id(&json!({"id": true})), None);
        assert_eq!(record_id(&json!({"name": "Ada"})), None);
    }

    #[test]
    fn safe_snapshot_file_path_keeps_paths_inside_snapshot_dir() -> Result<()> {
        let dir = Path::new("/tmp/snapshot");

        assert_eq!(
            safe_snapshot_file_path(dir, "contacts.jsonl")?,
            dir.join("contacts.jsonl")
        );
        assert!(safe_snapshot_file_path(dir, "../contacts.jsonl").is_err());
        assert!(safe_snapshot_file_path(dir, "/tmp/contacts.jsonl").is_err());
        Ok(())
    }

    #[test]
    fn read_snapshot_jsonl_records_rejects_duplicate_ids() -> Result<()> {
        let dir = temp_snapshot_dir("duplicate-jsonl-records");
        fs::create_dir_all(&dir).into_diagnostic()?;
        fs::write(
            dir.join("contacts.jsonl"),
            "{\"id\":1,\"name\":\"first\"}\n{\"id\":1,\"name\":\"second\"}\n",
        )
        .into_diagnostic()?;

        let error = read_snapshot_jsonl_records_at_path(&dir.join("contacts.jsonl"))
            .expect_err("duplicate IDs should not be collapsed");

        fs::remove_dir_all(&dir).ok();
        assert!(error.to_string().contains("duplicate record ID 1"));
        Ok(())
    }

    #[test]
    fn read_snapshot_array_record_values_rejects_duplicate_ids() -> Result<()> {
        let dir = temp_snapshot_dir("duplicate-array-records");
        fs::create_dir_all(&dir).into_diagnostic()?;
        fs::write(
            dir.join("groups.json"),
            r#"[{"id":1,"name":"first"},{"id":1,"name":"second"}]"#,
        )
        .into_diagnostic()?;

        let error = read_snapshot_array_record_values_at_path(&dir.join("groups.json"))
            .expect_err("duplicate IDs should not be collapsed");

        fs::remove_dir_all(&dir).ok();
        assert!(error.to_string().contains("duplicate record ID 1"));
        Ok(())
    }

    #[test]
    fn read_snapshot_array_values_rejects_non_array_files() -> Result<()> {
        let dir = temp_snapshot_dir("non-array-snapshot-file");
        fs::create_dir_all(&dir).into_diagnostic()?;
        fs::write(dir.join("groups.json"), r#"{"id":1,"name":"not an array"}"#)
            .into_diagnostic()?;

        let error = read_snapshot_array_values_at_path(&dir.join("groups.json"))
            .expect_err("snapshot array files should require a JSON array");

        fs::remove_dir_all(&dir).ok();
        assert!(error.to_string().contains("must be a JSON array"));
        Ok(())
    }

    #[test]
    fn email_domain_from_string_normalizes_wrapped_addresses() {
        assert_eq!(
            email_domain_from_string("Ada <Ada@Example.INVALID>,"),
            Some("example.invalid".to_string())
        );
        assert_eq!(email_domain_from_string("not-an-email"), None);
    }

    #[test]
    fn email_domain_from_string_stops_before_trailing_text() {
        assert_eq!(
            email_domain_from_string("ada@example.invalid phone"),
            Some("example.invalid".to_string())
        );
    }

    #[test]
    fn email_domain_from_string_rejects_malformed_domains() {
        assert_eq!(email_domain_from_string("ada@.com"), None);
        assert_eq!(email_domain_from_string("ada@example."), None);
        assert_eq!(email_domain_from_string("ada@example..invalid"), None);
        assert_eq!(email_domain_from_string("ada@-example.invalid"), None);
        assert_eq!(email_domain_from_string("ada@example-.invalid"), None);
    }

    #[test]
    fn email_domains_from_row_collects_nested_unique_domains() {
        let row = json!({
            "email": "ada@example.invalid",
            "history": [
                {"value": "Ada <ada@EXAMPLE.invalid>"},
                {"value": "grace@mesh.invalid"}
            ]
        });

        assert_eq!(
            email_domains_from_row(&row),
            BTreeSet::from(["example.invalid".to_string(), "mesh.invalid".to_string()])
        );
    }
}
