use clap::builder::PossibleValuesParser;
use clap::{Arg, ArgAction, Command, ValueHint, value_parser};
use clap_complete::Shell;

use crate::command_spec::{
    CommandSpec, OptionSpec, ValueKind, command_specs, contact_mutation_options, search_options,
};
use crate::consts::{API_BASE, MCP_BASE, VERSION};

pub(crate) fn build_cli() -> Command {
    let mut root = Command::new("mesh")
        .bin_name("mesh")
        .version(VERSION)
        .about("Inspect, export, update, and snapshot me.sh data from the terminal.")
        .arg_required_else_help(true)
        .subcommand_required(true)
        .propagate_version(true)
        .arg(
            Arg::new("format")
                .long("format")
                .short('f')
                .global(true)
                .help("Output format.")
                .value_parser(["json", "compact-json", "jsonl", "csv", "tsv", "table"])
                .default_value("json"),
        )
        .arg(
            Arg::new("output")
                .long("output")
                .short('o')
                .global(true)
                .help("Write command output to a file instead of stdout.")
                .value_hint(ValueHint::FilePath),
        )
        .arg(
            Arg::new("yes")
                .long("yes")
                .global(true)
                .help("Confirm write or destructive commands.")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .global(true)
                .help("Print the me.sh route and payload without sending the request.")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("timeout")
                .long("timeout")
                .global(true)
                .help("HTTP timeout in seconds.")
                .default_value("30")
                .value_parser(value_parser!(u64).range(1..=600)),
        )
        .arg(
            Arg::new("retries")
                .long("retries")
                .global(true)
                .help("Retries for transient HTTP failures.")
                .default_value("2")
                .value_parser(value_parser!(u32).range(0..=10)),
        )
        .arg(
            Arg::new("config")
                .long("config")
                .global(true)
                .help("Config file path. Defaults to ~/.config/mesh.json.")
                .env("MESH_CONFIG")
                .value_hint(ValueHint::FilePath),
        )
        .arg(
            Arg::new("api-base")
                .long("api-base")
                .global(true)
                .help("Override API base URL.")
                .env("MESH_API_BASE")
                .hide_env_values(true)
                .default_value(API_BASE),
        )
        .arg(
            Arg::new("mcp-base")
                .long("mcp-base")
                .global(true)
                .help("Override MCP tool base URL.")
                .env("MESH_MCP_BASE")
                .hide_env_values(true)
                .default_value(MCP_BASE),
        );

    root = root
        .subcommand(
            Command::new("login").about("Login with me.sh OAuth.").arg(
                Arg::new("open")
                    .long("open")
                    .help("Open the OAuth URL with the system browser.")
                    .action(ArgAction::SetTrue),
            ),
        )
        .subcommand(Command::new("logout").about("Remove stored me.sh credentials."))
        .subcommand(Command::new("status").about("Show local authentication status."))
        .subcommand(Command::new("whoami").about("Fetch the current me.sh user."))
        .subcommand(Command::new("doctor").about("Check config, auth, and network reachability."))
        .subcommand(Command::new("routes").about("Print the command-to-route map."))
        .subcommand(
            Command::new("routes:doctor")
                .about("Probe safe read-only me.sh tool routes and summarize response health.")
                .arg(
                    Arg::new("profile")
                        .long("profile")
                        .default_value("core")
                        .value_parser(PossibleValuesParser::new(["core", "moments", "all"]))
                        .help("Probe set to run. core checks search/groups; moments adds recent/upcoming activity; all also checks date-window activity."),
                )
                .arg(
                    Arg::new("routes")
                        .long("routes")
                        .help("Specific probe labels or routes, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("start")
                        .long("start")
                        .value_name("YYYY-MM-DD")
                        .help("Start date for notes, events, and emails probes."),
                )
                .arg(
                    Arg::new("end")
                        .long("end")
                        .value_name("YYYY-MM-DD")
                        .help("End date for notes, events, and emails probes."),
                )
                .arg(
                    Arg::new("contact-ids")
                        .long("contact-ids")
                        .help("Optional contact IDs for moment probes, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .value_name("COUNT")
                        .help("Page size for recent/upcoming moment probes. Default 1, maximum 1000."),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return one row per route probe instead of nested JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("schema")
                .about("Print command metadata.")
                .arg(Arg::new("command").help("Optional command name.")),
        )
        .subcommand(
            Command::new("plan:audit")
                .about("Audit mesh dry-run output before live writes.")
                .arg(
                    Arg::new("input")
                        .long("input")
                        .short('i')
                        .required(true)
                        .help("Dry-run output file to audit.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("input-format")
                        .long("input-format")
                        .default_value("auto")
                        .value_parser(["auto", "json", "jsonl", "csv", "tsv"])
                        .help("Input format. auto uses file extension or JSON-looking content."),
                )
                .arg(
                    Arg::new("max-writes")
                        .long("max-writes")
                        .help("Warn when the plan contains more write actions than this."),
                )
                .arg(
                    Arg::new("max-contact-ids")
                        .long("max-contact-ids")
                        .help("Warn when the plan touches more unique contact IDs than this."),
                )
                .arg(
                    Arg::new("max-group-ids")
                        .long("max-group-ids")
                        .help("Warn when the plan touches more unique group IDs than this."),
                )
                .arg(
                    Arg::new("id-sample-limit")
                        .long("id-sample-limit")
                        .default_value("20")
                        .help("Maximum contact/group IDs to include in samples. Use 0 for none."),
                )
                .arg(
                    Arg::new("duplicate-limit")
                        .long("duplicate-limit")
                        .default_value("20")
                        .help("Maximum duplicate payload examples to include. Use 0 for none."),
                )
                .arg(
                    Arg::new("strict")
                        .long("strict")
                        .help("Exit nonzero when warnings are found.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("raw")
                .about("Call a raw me.sh MCP /tools/v2 route.")
                .arg(
                    Arg::new("route")
                        .long("route")
                        .required(true)
                        .help("Route under /tools/v2, for example /search."),
                )
                .arg(
                    Arg::new("body")
                        .long("body")
                        .default_value("{}")
                        .help("JSON request body."),
                ),
        )
        .subcommand(
            Command::new("completions")
                .about("Generate shell completions.")
                .arg(
                    Arg::new("shell")
                        .required(true)
                        .value_parser(value_parser!(Shell))
                        .help("Shell to generate completions for."),
                ),
        )
        .subcommand(Command::new("config:path").about("Print the active config path."))
        .subcommand(Command::new("config:show").about("Print redacted config."))
        .subcommand(
            Command::new("snapshot:create")
                .about("Create a local me.sh data snapshot with a manifest.")
                .arg(
                    Arg::new("dir")
                        .long("dir")
                        .help("Snapshot output directory. Defaults to ./mesh-snapshot-<unix-ms>.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("force")
                        .long("force")
                        .help("Allow writing into an existing empty directory.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("full-contacts")
                        .long("full-contacts")
                        .help("Also fetch selected contacts with /get-contact into full-contacts.jsonl.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("full-contact-ids")
                        .long("full-contact-ids")
                        .help("Only fetch full records for these contact IDs, comma-separated or repeated. Implies --full-contacts.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("full-limit")
                        .long("full-limit")
                        .value_name("COUNT")
                        .help("Limit full-contact fetches after ID selection. Maximum 1000."),
                )
                .arg(
                    Arg::new("full-concurrency")
                        .long("full-concurrency")
                        .value_name("COUNT")
                        .help("Concurrent /get-contact requests for full snapshots. Default 4, maximum 16."),
                )
                .arg(
                    Arg::new("moments")
                        .long("moments")
                        .help("Also snapshot notes, events, emails, and reminders into JSONL files.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("moments-start")
                        .long("moments-start")
                        .value_name("YYYY-MM-DD")
                        .help("Start date for notes/events/emails when --moments is used."),
                )
                .arg(
                    Arg::new("moments-end")
                        .long("moments-end")
                        .value_name("YYYY-MM-DD")
                        .help("End date for notes/events/emails when --moments is used."),
                )
                .arg(
                    Arg::new("moments-contact-ids")
                        .long("moments-contact-ids")
                        .help("Limit moment snapshot routes to these contact IDs, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("moments-limit")
                        .long("moments-limit")
                        .value_name("COUNT")
                        .help("Page size for recent/upcoming moment routes. Default 100, maximum 1000."),
                ),
        )
        .subcommand(
            Command::new("snapshot:verify")
                .about("Verify files listed in a snapshot manifest.")
                .arg(
                    Arg::new("dir")
                        .long("dir")
                        .required(true)
                        .help("Snapshot directory.")
                        .value_hint(ValueHint::DirPath),
                ),
        )
        .subcommand(
            Command::new("snapshot:verify-archive")
                .about("Verify a packed snapshot archive without extracting it.")
                .arg(
                    Arg::new("archive")
                        .long("archive")
                        .required(true)
                        .help("Snapshot .tar.zst archive to verify.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("require-index")
                        .long("require-index")
                        .help("Treat missing JSONL index sidecars as failures.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("snapshot:catalog")
                .about("Find and summarize local snapshot directories and packed archives.")
                .arg(
                    Arg::new("root")
                        .long("root")
                        .default_value(".")
                        .help("Directory or archive path to inspect. Defaults to the current directory.")
                        .value_hint(ValueHint::AnyPath),
                )
                .arg(
                    Arg::new("recursive")
                        .long("recursive")
                        .help("Recurse below --root. Without this, only --root and its direct children are inspected.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("max-depth")
                        .long("max-depth")
                        .value_name("COUNT")
                        .help("Maximum directory depth to scan when --recursive is set."),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .value_name("COUNT")
                        .help("Stop after this many discovered snapshots/archives."),
                )
                .arg(
                    Arg::new("snapshots")
                        .long("snapshots")
                        .help("Only include snapshot directories unless --archives is also set.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("archives")
                        .long("archives")
                        .help("Only include .tar.zst archives unless --snapshots is also set.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("verify")
                        .long("verify")
                        .help("Verify discovered snapshot manifests and archive contents.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("doctor")
                        .long("doctor")
                        .help("Run snapshot:doctor for directories; archives use archive verification.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("require-index")
                        .long("require-index")
                        .help("Treat missing JSONL indexes as failures when --verify or --doctor is used.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("snapshot:prune")
                .about("Delete old or failed local snapshot directories and packed archives.")
                .arg(
                    Arg::new("root")
                        .long("root")
                        .default_value(".")
                        .help("Directory or archive path to inspect. Defaults to the current directory.")
                        .value_hint(ValueHint::AnyPath),
                )
                .arg(
                    Arg::new("recursive")
                        .long("recursive")
                        .help("Recurse below --root. Without this, only --root and its direct children are inspected.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("max-depth")
                        .long("max-depth")
                        .value_name("COUNT")
                        .help("Maximum directory depth to scan when --recursive is set."),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .value_name("COUNT")
                        .help("Stop after this many discovered snapshots/archives."),
                )
                .arg(
                    Arg::new("snapshots")
                        .long("snapshots")
                        .help("Only include snapshot directories unless --archives is also set.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("archives")
                        .long("archives")
                        .help("Only include .tar.zst archives unless --snapshots is also set.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("keep-latest")
                        .long("keep-latest")
                        .value_name("COUNT")
                        .help("Keep the newest COUNT items per kind and prune older discoveries. COUNT may be 0."),
                )
                .arg(
                    Arg::new("older-than-days")
                        .long("older-than-days")
                        .value_name("DAYS")
                        .help("Prune items whose snapshot timestamp or filesystem mtime is older than this many days."),
                )
                .arg(
                    Arg::new("failed")
                        .long("failed")
                        .help("Prune items that fail snapshot or archive verification.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("require-index")
                        .long("require-index")
                        .help("When --failed is used, treat missing JSONL indexes as failures.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("snapshot:history")
                .about("Trace record IDs across local snapshot directories.")
                .arg(
                    Arg::new("root")
                        .long("root")
                        .default_value(".")
                        .help("Snapshot directory or root containing snapshot directories.")
                        .value_hint(ValueHint::AnyPath),
                )
                .arg(
                    Arg::new("recursive")
                        .long("recursive")
                        .help("Recurse below --root. Without this, only --root and its direct children are inspected.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("max-depth")
                        .long("max-depth")
                        .value_name("COUNT")
                        .help("Maximum directory depth to scan when --recursive is set."),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .value_name("COUNT")
                        .help("Stop after this many discovered snapshot directories."),
                )
                .arg(
                    Arg::new("section")
                        .long("section")
                        .default_value("contacts")
                        .value_parser(PossibleValuesParser::new([
                            "contacts",
                            "full-contacts",
                            "full_contacts",
                            "groups",
                            "notes",
                            "events",
                            "emails",
                            "events-upcoming",
                            "events_upcoming",
                            "emails-recent",
                            "emails_recent",
                            "reminders-recent",
                            "reminders_recent",
                            "reminders-upcoming",
                            "reminders_upcoming",
                        ]))
                        .help("Snapshot section to trace."),
                )
                .arg(
                    Arg::new("ids")
                        .long("ids")
                        .help("Record IDs to trace, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("index")
                        .long("index")
                        .default_value("auto")
                        .value_parser(["auto", "off", "require"])
                        .help("Use a snapshot:index sidecar for JSONL ID lookups when available."),
                )
                .arg(
                    Arg::new("skip-verify")
                        .long("skip-verify")
                        .help("Read snapshots without verifying manifest hashes first.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("details")
                        .long("details")
                        .help("Include bounded JSON-pointer changes for changed records.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("detail-limit")
                        .long("detail-limit")
                        .value_name("COUNT")
                        .help("Maximum field changes per changed record. Default 20, maximum 1000."),
                )
                .arg(
                    Arg::new("records")
                        .long("records")
                        .help("Include the full current record for present observations.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("dry-run")
                        .long("dry-run")
                        .help("Print the local files and work that would be used without reading records.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("snapshot:find")
                .about("Find records across local snapshot directories.")
                .arg(
                    Arg::new("root")
                        .long("root")
                        .default_value(".")
                        .help("Snapshot directory or root containing snapshot directories.")
                        .value_hint(ValueHint::AnyPath),
                )
                .arg(
                    Arg::new("recursive")
                        .long("recursive")
                        .help("Recurse below --root. Without this, only --root and its direct children are inspected.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("max-depth")
                        .long("max-depth")
                        .value_name("COUNT")
                        .help("Maximum directory depth to scan when --recursive is set."),
                )
                .arg(
                    Arg::new("snapshot-limit")
                        .long("snapshot-limit")
                        .value_name("COUNT")
                        .help("Stop after this many discovered snapshot directories."),
                )
                .arg(
                    Arg::new("section")
                        .long("section")
                        .help("Snapshot section to search. Defaults to every known section. Comma-separated or repeated.")
                        .action(ArgAction::Append)
                        .value_parser(PossibleValuesParser::new([
                            "contacts",
                            "full-contacts",
                            "full_contacts",
                            "groups",
                            "notes",
                            "events",
                            "emails",
                            "events-upcoming",
                            "events_upcoming",
                            "emails-recent",
                            "emails_recent",
                            "reminders-recent",
                            "reminders_recent",
                            "reminders-upcoming",
                            "reminders_upcoming",
                        ])),
                )
                .arg(
                    Arg::new("ids")
                        .long("ids")
                        .help("Only include records with these top-level IDs, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("contains")
                        .long("contains")
                        .help("Only include records whose JSON contains this case-insensitive text."),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .value_name("COUNT")
                        .help("Maximum matching records to return across all searched snapshots."),
                )
                .arg(
                    Arg::new("index")
                        .long("index")
                        .default_value("auto")
                        .value_parser(["auto", "off", "require"])
                        .help("Use snapshot:index sidecars for JSONL ID lookups when available."),
                )
                .arg(
                    Arg::new("skip-verify")
                        .long("skip-verify")
                        .help("Read snapshots without verifying manifest hashes first.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("records")
                        .long("records")
                        .help("Include the full matching record.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("snapshot:timeline")
                .about("Diff adjacent local snapshots in time order.")
                .arg(
                    Arg::new("root")
                        .long("root")
                        .default_value(".")
                        .help("Snapshot directory or root containing snapshot directories.")
                        .value_hint(ValueHint::AnyPath),
                )
                .arg(
                    Arg::new("recursive")
                        .long("recursive")
                        .help("Recurse below --root. Without this, only --root and its direct children are inspected.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("max-depth")
                        .long("max-depth")
                        .value_name("COUNT")
                        .help("Maximum directory depth to scan when --recursive is set."),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .value_name("COUNT")
                        .help("Stop after this many discovered snapshot directories."),
                )
                .arg(
                    Arg::new("changes-only")
                        .long("changes-only")
                        .help("Only emit adjacent pairs with detected changes or errors.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("diffs")
                        .long("diffs")
                        .help("Include the full snapshot:diff result for each emitted pair.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("details")
                        .long("details")
                        .help("Include bounded field-level change paths in embedded diffs. Implies --diffs.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("detail-limit")
                        .long("detail-limit")
                        .value_name("COUNT")
                        .help("Maximum changed records per section to describe when --details is used. Default 20, maximum 1000."),
                ),
        )
        .subcommand(
            Command::new("snapshot:drift")
                .about("Compare a local snapshot with current live me.sh data.")
                .arg(
                    Arg::new("dir")
                        .long("dir")
                        .required(true)
                        .help("Snapshot directory to compare against live me.sh.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("page-size")
                        .long("page-size")
                        .value_name("COUNT")
                        .help("Rows per live search page. Default 1000, maximum 1000."),
                )
                .arg(
                    Arg::new("skip-groups")
                        .long("skip-groups")
                        .help("Only compare contacts; do not call /get-groups.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("full-contact-ids")
                        .long("full-contact-ids")
                        .help("Also compare selected full /get-contact records, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("full-concurrency")
                        .long("full-concurrency")
                        .value_name("COUNT")
                        .help("Concurrent /get-contact requests for full-record drift. Default 4, maximum 16."),
                )
                .arg(
                    Arg::new("skip-verify")
                        .long("skip-verify")
                        .help("Read the snapshot without verifying manifest hashes first.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("details")
                        .long("details")
                        .help("Include bounded JSON-pointer changes for changed records.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("detail-limit")
                        .long("detail-limit")
                        .value_name("COUNT")
                        .help("Maximum changed records per section to describe. Default 20, maximum 1000."),
                ),
        )
        .subcommand(
            Command::new("snapshot:report")
                .about("Generate a combined local snapshot health and context report.")
                .arg(
                    Arg::new("dir")
                        .long("dir")
                        .required(true)
                        .help("Snapshot directory to report on.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("root")
                        .long("root")
                        .help("Root containing sibling snapshots for neighbor context. Defaults to the snapshot parent.")
                        .value_hint(ValueHint::AnyPath),
                )
                .arg(
                    Arg::new("recursive")
                        .long("recursive")
                        .help("Recurse below --root when looking for neighboring snapshots.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("max-depth")
                        .long("max-depth")
                        .value_name("COUNT")
                        .help("Maximum directory depth to scan when --recursive is set."),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .value_name("COUNT")
                        .help("Stop neighbor discovery after this many snapshot directories."),
                )
                .arg(
                    Arg::new("neighbors")
                        .long("neighbors")
                        .value_name("COUNT")
                        .help("Previous and next snapshots to summarize around --dir. Default 1; use 0 to disable."),
                )
                .arg(
                    Arg::new("diffs")
                        .long("diffs")
                        .help("Include full snapshot:diff output for neighbor comparisons.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("drift")
                        .long("drift")
                        .help("Also compare this snapshot with current live me.sh data.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("page-size")
                        .long("page-size")
                        .value_name("COUNT")
                        .help("Rows per live search page when --drift is used. Default 1000, maximum 1000."),
                )
                .arg(
                    Arg::new("skip-groups")
                        .long("skip-groups")
                        .help("When --drift is used, only compare contacts; do not call /get-groups.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("full-contact-ids")
                        .long("full-contact-ids")
                        .help("When --drift is used, compare selected full /get-contact records, comma-separated or repeated. Implies --drift.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("full-concurrency")
                        .long("full-concurrency")
                        .value_name("COUNT")
                        .help("Concurrent /get-contact requests for full-record drift. Default 4, maximum 16."),
                )
                .arg(
                    Arg::new("top")
                        .long("top")
                        .value_name("COUNT")
                        .help("Top key/domain counts for embedded stats and doctor output. Default 10."),
                )
                .arg(
                    Arg::new("require-index")
                        .long("require-index")
                        .help("Require fresh JSONL indexes in the embedded doctor report.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("skip-verify")
                        .long("skip-verify")
                        .help("Read snapshot data without verifying manifest hashes first.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("details")
                        .long("details")
                        .help("Include bounded JSON-pointer changes in neighbor diffs or drift. Implies --diffs for neighbors.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("detail-limit")
                        .long("detail-limit")
                        .value_name("COUNT")
                        .help("Maximum changed records per section to describe. Default 20, maximum 1000."),
                ),
        )
        .subcommand(
            Command::new("snapshot:diff")
                .about("Compare two local me.sh snapshots.")
                .arg(
                    Arg::new("old")
                        .long("old")
                        .required(true)
                        .help("Older snapshot directory.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("new")
                        .long("new")
                        .required(true)
                        .help("Newer snapshot directory.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("details")
                        .long("details")
                        .help("Include bounded field-level change paths for changed snapshot records.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("detail-limit")
                        .long("detail-limit")
                        .value_name("COUNT")
                        .help("Maximum changed records per section to describe when --details is used. Default 20, maximum 1000."),
                ),
        )
        .subcommand(
            Command::new("snapshot:stats")
                .about("Summarize local snapshot row counts, IDs, and field coverage.")
                .arg(
                    Arg::new("dir")
                        .long("dir")
                        .required(true)
                        .help("Snapshot directory.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("top")
                        .long("top")
                        .value_name("COUNT")
                        .help("Number of top keys, duplicates, and domains to report. Default 10."),
                )
                .arg(
                    Arg::new("skip-verify")
                        .long("skip-verify")
                        .help("Read snapshot records without verifying manifest hashes first.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("snapshot:doctor")
                .about("Check local snapshot manifest, counts, IDs, and index health.")
                .arg(
                    Arg::new("dir")
                        .long("dir")
                        .required(true)
                        .help("Snapshot directory.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("top")
                        .long("top")
                        .value_name("COUNT")
                        .help("Number of top duplicate IDs and domains to include in embedded stats. Default 10."),
                )
                .arg(
                    Arg::new("require-index")
                        .long("require-index")
                        .help("Treat missing, invalid, or stale JSONL indexes as failures.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("skip-verify")
                        .long("skip-verify")
                        .help("Read snapshot records without verifying manifest hashes first.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("snapshot:pack")
                .about("Pack a local snapshot directory into a verified .tar.zst archive.")
                .arg(
                    Arg::new("dir")
                        .long("dir")
                        .required(true)
                        .help("Snapshot directory.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("archive")
                        .long("archive")
                        .required(true)
                        .help("Archive path to write, usually ending in .tar.zst.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("compression-level")
                        .long("compression-level")
                        .value_name("LEVEL")
                        .help("Zstd compression level 0-22. Default 0 uses zstd's default level."),
                )
                .arg(
                    Arg::new("no-index")
                        .long("no-index")
                        .help("Do not include valid .meshx-index sidecars.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("skip-verify")
                        .long("skip-verify")
                        .help("Pack snapshot files without verifying manifest hashes first.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("force")
                        .long("force")
                        .help("Replace an existing archive path.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("snapshot:unpack")
                .about("Unpack a mesh snapshot .tar.zst archive into a directory.")
                .arg(
                    Arg::new("archive")
                        .long("archive")
                        .required(true)
                        .help("Archive path to read.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("dir")
                        .long("dir")
                        .required(true)
                        .help("Destination snapshot directory.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("force")
                        .long("force")
                        .help("Allow an existing empty destination directory.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("skip-verify")
                        .long("skip-verify")
                        .help("Unpack without verifying manifest hashes afterward.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("snapshot:index")
                .about("Build or refresh a byte-offset index for a snapshot JSONL section.")
                .arg(
                    Arg::new("dir")
                        .long("dir")
                        .required(true)
                        .help("Snapshot directory.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("section")
                        .long("section")
                        .default_value("contacts")
                        .value_parser(PossibleValuesParser::new([
                            "contacts",
                            "full-contacts",
                            "full_contacts",
                            "notes",
                            "events",
                            "emails",
                            "events-upcoming",
                            "events_upcoming",
                            "emails-recent",
                            "emails_recent",
                            "reminders-recent",
                            "reminders_recent",
                            "reminders-upcoming",
                            "reminders_upcoming",
                        ]))
                        .help("Snapshot JSONL section to index."),
                )
                .arg(
                    Arg::new("force")
                        .long("force")
                        .help("Rebuild even when an existing index matches the snapshot file.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("snapshot:query")
                .about("Read records from a verified local snapshot.")
                .arg(
                    Arg::new("dir")
                        .long("dir")
                        .required(true)
                        .help("Snapshot directory.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("section")
                        .long("section")
                        .default_value("contacts")
                        .value_parser(PossibleValuesParser::new([
                            "contacts",
                            "full-contacts",
                            "full_contacts",
                            "groups",
                            "notes",
                            "events",
                            "emails",
                            "events-upcoming",
                            "events_upcoming",
                            "emails-recent",
                            "emails_recent",
                            "reminders-recent",
                            "reminders_recent",
                            "reminders-upcoming",
                            "reminders_upcoming",
                        ]))
                        .help("Snapshot record section to read."),
                )
                .arg(
                    Arg::new("ids")
                        .long("ids")
                        .help("Only include records with these top-level IDs, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("contains")
                        .long("contains")
                        .help("Only include records whose JSON contains this case-insensitive text."),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .value_name("COUNT")
                        .help("Maximum matching records to return."),
                )
                .arg(
                    Arg::new("index")
                        .long("index")
                        .default_value("auto")
                        .value_parser(["auto", "off", "require"])
                        .help("Use a snapshot:index sidecar for --ids lookups when available."),
                )
                .arg(
                    Arg::new("skip-verify")
                        .long("skip-verify")
                        .help("Read snapshot records without verifying manifest hashes first.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("snapshot:restore")
                .about("Restore or import contacts from full-contacts.jsonl.")
                .arg(
                    Arg::new("dir")
                        .long("dir")
                        .required(true)
                        .help("Snapshot directory.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("mode")
                        .long("mode")
                        .default_value("update")
                        .value_parser(PossibleValuesParser::new(["update", "create"]))
                        .help("update restores onto original contact IDs; create imports as new contacts."),
                )
                .arg(
                    Arg::new("contact-ids")
                        .long("contact-ids")
                        .help("Only restore these snapshot contact IDs, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("include-notes")
                        .long("include-notes")
                        .help("Also recreate notes from full-contact snapshot data. This can create duplicates.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("contacts:count").about("Count contacts with me.sh search filters."),
        )
        .subcommand(
            Command::new("contacts:export")
                .about("Export search results. Use --format csv, tsv, jsonl, table, or json.")
                .arg(
                    Arg::new("all")
                        .long("all")
                        .help("Fetch every matching contact by paging with exclude-contact-ids.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("page-size")
                        .long("page-size")
                        .value_name("COUNT")
                        .help("Rows per search page for --all. Default 1000, maximum 1000."),
                )
                .arg(
                    Arg::new("resume")
                        .long("resume")
                        .help("Resume a previous --all --format jsonl --output export by appending missing contacts.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("contacts:resolve")
                .about("Resolve me.sh search filters to contact ID candidates.")
                .arg(
                    Arg::new("candidate-limit")
                        .long("candidate-limit")
                        .value_name("COUNT")
                        .help("Maximum candidate rows to fetch. Default 20, maximum 1000."),
                )
                .arg(
                    Arg::new("one")
                        .long("one")
                        .help("Require exactly one matching contact.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .help("Allow resolving without search filters.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("contacts:bulk-get")
                .about("Fetch many contacts by ID.")
                .arg(
                    Arg::new("contact-ids")
                        .long("contact-ids")
                        .help("Contact IDs, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("input")
                        .long("input")
                        .short('i')
                        .help("File containing contact IDs as JSON array, CSV, or one ID per line.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("concurrency")
                        .long("concurrency")
                        .value_name("COUNT")
                        .help("Concurrent /get-contact requests. Default 4, maximum 16."),
                ),
        )
        .subcommand(
            Command::new("contacts:activity")
                .about("Fetch full contacts plus related notes, events, emails, and reminders.")
                .arg(
                    Arg::new("contact-ids")
                        .long("contact-ids")
                        .required(true)
                        .help("Contact IDs, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("sections")
                        .long("sections")
                        .help("Moment sections: all, notes, events, emails, events-upcoming, emails-recent, reminders-recent, reminders-upcoming.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("start")
                        .long("start")
                        .value_name("YYYY-MM-DD")
                        .help("Start date required for notes, events, and emails."),
                )
                .arg(
                    Arg::new("end")
                        .long("end")
                        .value_name("YYYY-MM-DD")
                        .help("End date required for notes, events, and emails."),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .value_name("COUNT")
                        .help("Page size for recent/upcoming sections. Default 100, maximum 1000."),
                )
                .arg(
                    Arg::new("concurrency")
                        .long("concurrency")
                        .value_name("COUNT")
                        .help("Concurrent /get-contact requests. Default 4, maximum 16."),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return one row per contact/section/activity item instead of nested JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("contacts:profile")
                .about("Build a read-only contact profile with full contact, groups, and recent activity.")
                .arg(
                    Arg::new("contact-ids")
                        .long("contact-ids")
                        .required(true)
                        .help("Contact IDs, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("activity-sections")
                        .long("activity-sections")
                        .help("Activity sections. Defaults to recent/upcoming sections. Use all, notes, events, emails, events-upcoming, emails-recent, reminders-recent, reminders-upcoming.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("start")
                        .long("start")
                        .help("Start date required when activity sections include notes, events, or emails."),
                )
                .arg(
                    Arg::new("end")
                        .long("end")
                        .help("End date required when activity sections include notes, events, or emails."),
                )
                .arg(
                    Arg::new("activity-limit")
                        .long("activity-limit")
                        .help("Page size for recent/upcoming activity sections. Default 100, maximum 1000."),
                )
                .arg(
                    Arg::new("group-scan-limit")
                        .long("group-scan-limit")
                        .help("Maximum members to scan per group when discovering memberships. Default 1000; higher values page repeatedly."),
                )
                .arg(
                    Arg::new("concurrency")
                        .long("concurrency")
                        .value_name("COUNT")
                        .help("Concurrent /get-contact and group member reads. Default 4, maximum 16."),
                )
                .arg(
                    Arg::new("skip-groups")
                        .long("skip-groups")
                        .help("Do not scan group membership.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("skip-activity")
                        .long("skip-activity")
                        .help("Do not fetch moment activity.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return one summary row per contact instead of nested JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("contacts:groups")
                .about("Inventory live group/list memberships for selected contacts.")
                .arg(
                    Arg::new("contact-ids")
                        .long("contact-ids")
                        .help("Contact IDs, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("input")
                        .long("input")
                        .short('i')
                        .help("File containing contact IDs as JSON array, CSV, or one ID per line.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("group-query")
                        .long("group-query")
                        .help("Case-insensitive substring to restrict scanned group names."),
                )
                .arg(
                    Arg::new("group-ids")
                        .long("group-ids")
                        .help("Restrict scanned groups to IDs or starred, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("include-fields")
                        .long("include-fields")
                        .help("Extra member contact fields to include. Accepts aliases email, phone, linkedin, work, education, messages, events, notes, and integrations.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("group-scan-limit")
                        .long("group-scan-limit")
                        .help("Maximum members to scan per group when discovering memberships. Default 1000; higher values page repeatedly."),
                )
                .arg(
                    Arg::new("concurrency")
                        .long("concurrency")
                        .value_name("COUNT")
                        .help("Concurrent group member reads. Default 4, maximum 16."),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return one row per contact/group membership instead of nested JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("moments:feed")
                .about("Fetch a live read-only activity feed across notes, events, emails, and reminders.")
                .arg(
                    Arg::new("sections")
                        .long("sections")
                        .help("Moment sections: all, notes, events, emails, events-upcoming, emails-recent, reminders-recent, reminders-upcoming.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("start")
                        .long("start")
                        .help("Start date required for notes, events, and emails."),
                )
                .arg(
                    Arg::new("end")
                        .long("end")
                        .help("End date required for notes, events, and emails."),
                )
                .arg(
                    Arg::new("contact-ids")
                        .long("contact-ids")
                        .help("Optional contact IDs to filter the feed. Comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .help("Page size for recent/upcoming sections. Default 100, maximum 1000."),
                )
                .arg(
                    Arg::new("item-limit")
                        .long("item-limit")
                        .help("Maximum flattened feed rows to emit after sorting. Maximum 1000."),
                )
                .arg(
                    Arg::new("sort")
                        .long("sort")
                        .default_value("desc")
                        .value_parser(PossibleValuesParser::new(["desc", "asc", "none"]))
                        .help("Sort flattened feed rows by extracted date."),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return one row per activity item instead of nested JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("moments:stats")
                .about("Summarize live read-only activity by section, contact, and date bucket.")
                .arg(
                    Arg::new("sections")
                        .long("sections")
                        .help("Moment sections: all, notes, events, emails, events-upcoming, emails-recent, reminders-recent, reminders-upcoming.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("start")
                        .long("start")
                        .help("Start date required for notes, events, and emails."),
                )
                .arg(
                    Arg::new("end")
                        .long("end")
                        .help("End date required for notes, events, and emails."),
                )
                .arg(
                    Arg::new("contact-ids")
                        .long("contact-ids")
                        .help("Optional contact IDs to filter the stats. Comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .help("Page size for recent/upcoming sections. Default 100, maximum 1000."),
                )
                .arg(
                    Arg::new("date-bucket")
                        .long("date-bucket")
                        .default_value("day")
                        .value_parser(PossibleValuesParser::new(["day", "month", "year", "raw"]))
                        .help("Date bucket for activity dates."),
                )
                .arg(
                    Arg::new("top-contacts")
                        .long("top-contacts")
                        .value_name("COUNT")
                        .help("Top contacts to return. Default 10, maximum 1000."),
                )
                .arg(
                    Arg::new("top-dates")
                        .long("top-dates")
                        .value_name("COUNT")
                        .help("Top date buckets to return. Default 10, maximum 1000."),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return section, contact, and date-bucket rows instead of nested JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("moments:timeline")
                .about("Build a chronological read-only activity timeline from live me.sh or a local snapshot.")
                .arg(
                    Arg::new("snapshot-dir")
                        .long("snapshot-dir")
                        .help("Read moment rows from a verified local snapshot instead of live me.sh.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("sections")
                        .long("sections")
                        .help("Moment sections: all, notes, events, emails, events-upcoming, emails-recent, reminders-recent, reminders-upcoming.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("start")
                        .long("start")
                        .help("Start date. Required for live notes, events, and emails; filters snapshot rows locally when paired with --end."),
                )
                .arg(
                    Arg::new("end")
                        .long("end")
                        .help("End date. Required for live notes, events, and emails; filters snapshot rows locally when paired with --start."),
                )
                .arg(
                    Arg::new("contact-ids")
                        .long("contact-ids")
                        .help("Optional contact IDs to filter the timeline. Comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .help("Page size for live recent/upcoming sections. Default 100, maximum 1000."),
                )
                .arg(
                    Arg::new("item-limit")
                        .long("item-limit")
                        .help("Maximum activity rows to consider after sorting/filtering. Maximum 1000."),
                )
                .arg(
                    Arg::new("date-bucket")
                        .long("date-bucket")
                        .default_value("day")
                        .value_parser(PossibleValuesParser::new(["day", "month", "year", "raw"]))
                        .help("Date bucket for timeline groups."),
                )
                .arg(
                    Arg::new("bucket-limit")
                        .long("bucket-limit")
                        .value_name("COUNT")
                        .help("Maximum date buckets to return. Default 30, maximum 1000."),
                )
                .arg(
                    Arg::new("items-per-bucket")
                        .long("items-per-bucket")
                        .value_name("COUNT")
                        .help("Maximum nested activity samples per bucket. Default 5, maximum 1000; use 0 for counts only."),
                )
                .arg(
                    Arg::new("sort")
                        .long("sort")
                        .default_value("desc")
                        .value_parser(PossibleValuesParser::new(["desc", "asc", "none"]))
                        .help("Sort flattened activity rows by extracted date before bucketing."),
                )
                .arg(
                    Arg::new("skip-verify")
                        .long("skip-verify")
                        .help("Do not verify snapshot manifest hashes before reading --snapshot-dir.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return one row per returned activity item instead of nested JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("notes:bulk-create")
                .about("Create the same note or reminder for many contacts from IDs, a file, or live search.")
                .arg(
                    Arg::new("content")
                        .long("content")
                        .required(true)
                        .help("Note content to create for each target contact."),
                )
                .arg(
                    Arg::new("reminder-date")
                        .long("reminder-date")
                        .help("Optional ISO 8601 reminder date for every created note."),
                )
                .arg(
                    Arg::new("contact-ids")
                        .long("contact-ids")
                        .help("Target contact IDs, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("input")
                        .long("input")
                        .short('i')
                        .help("File containing target contact IDs as JSON array, CSV, or one ID per line.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("from-search")
                        .long("from-search")
                        .help("Add target contacts from live contact search filters.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("all-search")
                        .long("all-search")
                        .help("Allow --from-search with no filters, meaning every live contact.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("page-size")
                        .long("page-size")
                        .value_name("COUNT")
                        .help("Rows per live search page. Default 1000, maximum 1000."),
                )
                .arg(
                    Arg::new("target-limit")
                        .long("target-limit")
                        .value_name("COUNT")
                        .help("Maximum total target contacts after ID and search selection."),
                )
                .arg(
                    Arg::new("concurrency")
                        .long("concurrency")
                        .value_name("COUNT")
                        .help("Concurrent /note writes. Default 4, maximum 16."),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return one row per note target/write instead of nested JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(contact_bulk_state_command(
            "contacts:bulk-archive",
            "Archive many contacts from IDs, a file, or live search.",
        ))
        .subcommand(contact_bulk_state_command(
            "contacts:bulk-restore",
            "Restore many archived contacts from IDs, a file, or live search.",
        ))
        .subcommand(contact_bulk_update_command())
        .subcommand(
            Command::new("contacts:apply")
                .about("Apply bulk contact actions from JSON, JSONL, CSV, or TSV.")
                .arg(
                    Arg::new("input")
                        .long("input")
                        .short('i')
                        .required(true)
                        .help("Action file to read.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("input-format")
                        .long("input-format")
                        .default_value("auto")
                        .value_parser(PossibleValuesParser::new([
                            "auto", "json", "jsonl", "csv", "tsv",
                        ]))
                        .help("Input format. auto uses file extension or JSON-looking content."),
                )
                .arg(
                    Arg::new("default-action")
                        .long("default-action")
                        .default_value("create")
                        .value_parser(PossibleValuesParser::new([
                            "create", "update", "archive", "restore", "note",
                        ]))
                        .help("Action for rows without an action column."),
                )
                .arg(
                    Arg::new("ignore-unknown")
                        .long("ignore-unknown")
                        .help("Ignore unknown input columns instead of failing before writes.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("concurrency")
                        .long("concurrency")
                        .value_name("COUNT")
                        .help("Concurrent write requests. Default 4, maximum 16."),
                ),
        )
        .subcommand(
            Command::new("contacts:dedupe")
                .about("Find likely duplicate contacts from live search, snapshots, or local files.")
                .arg(
                    Arg::new("input")
                        .long("input")
                        .short('i')
                        .help("Local JSON, JSONL, CSV, or TSV contacts file to analyze.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("input-format")
                        .long("input-format")
                        .default_value("auto")
                        .value_parser(PossibleValuesParser::new([
                            "auto", "json", "jsonl", "csv", "tsv",
                        ]))
                        .help("Input format for --input. auto uses file extension or JSON-looking content."),
                )
                .arg(
                    Arg::new("snapshot-dir")
                        .long("snapshot-dir")
                    .help("Snapshot directory to analyze instead of live me.sh.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("by")
                        .long("by")
                        .help("Duplicate signals, comma-separated or repeated. Defaults to email,phone,linkedin,name.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("min-confidence")
                        .long("min-confidence")
                        .default_value("60")
                        .value_name("PERCENT")
                        .help("Minimum candidate confidence, 0 to 100."),
                )
                .arg(
                    Arg::new("candidate-limit")
                        .long("candidate-limit")
                        .value_name("COUNT")
                        .help("Maximum candidate groups to output."),
                ),
        )
        .subcommand(
            Command::new("contacts:quality")
                .about("Audit contact data quality from live search, snapshots, or local files.")
                .arg(
                    Arg::new("input")
                        .long("input")
                        .short('i')
                        .help("Local JSON, JSONL, CSV, or TSV contacts file to audit.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("input-format")
                        .long("input-format")
                        .default_value("auto")
                        .value_parser(PossibleValuesParser::new([
                            "auto", "json", "jsonl", "csv", "tsv",
                        ]))
                        .help("Input format for --input. auto uses file extension or JSON-looking content."),
                )
                .arg(
                    Arg::new("snapshot-dir")
                        .long("snapshot-dir")
                    .help("Snapshot directory to audit instead of live me.sh.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("issue-limit")
                        .long("issue-limit")
                        .value_name("COUNT")
                        .help("Maximum issue contact rows to output. Default 50, maximum 1000."),
                )
                .arg(
                    Arg::new("top")
                        .long("top")
                        .value_name("COUNT")
                        .help("Top issue, warning, domain, and duplicate counts to output. Default 10."),
                ),
        )
        .subcommand(
            Command::new("contacts:facets")
                .about("Aggregate contact facets from live search, snapshots, or local files.")
                .arg(
                    Arg::new("input")
                        .long("input")
                        .short('i')
                        .help("Local JSON, JSONL, CSV, or TSV contacts file to analyze.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("input-format")
                        .long("input-format")
                        .default_value("auto")
                        .value_parser(PossibleValuesParser::new([
                            "auto", "json", "jsonl", "csv", "tsv",
                        ]))
                        .help("Input format for --input. auto uses file extension or JSON-looking content."),
                )
                .arg(
                    Arg::new("snapshot-dir")
                        .long("snapshot-dir")
                    .help("Snapshot directory to analyze instead of live me.sh.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("by")
                        .long("by")
                        .help("Facets, comma-separated or repeated. Defaults to email-domain,company,title,location.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("top")
                        .long("top")
                        .value_name("COUNT")
                        .help("Top buckets per facet. Default 10."),
                )
                .arg(
                    Arg::new("min-count")
                        .long("min-count")
                        .value_name("COUNT")
                        .help("Minimum bucket count to return. Default 1."),
                )
                .arg(
                    Arg::new("sample-limit")
                        .long("sample-limit")
                        .value_name("COUNT")
                        .help("Sample contacts per bucket. Default 5, maximum 1000."),
                )
                .arg(
                    Arg::new("include-empty")
                        .long("include-empty")
                        .help("Include an empty bucket for contacts missing a facet.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return one row per facet bucket instead of nested JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("contacts:pivot")
                .about("Cross-tab contact facets from live search, snapshots, or local files.")
                .arg(
                    Arg::new("input")
                        .long("input")
                        .short('i')
                        .help("Local JSON, JSONL, CSV, or TSV contacts file to analyze.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("input-format")
                        .long("input-format")
                        .default_value("auto")
                        .value_parser(PossibleValuesParser::new([
                            "auto", "json", "jsonl", "csv", "tsv",
                        ]))
                        .help("Input format for --input. auto uses file extension or JSON-looking content."),
                )
                .arg(
                    Arg::new("snapshot-dir")
                        .long("snapshot-dir")
                    .help("Snapshot directory to analyze instead of live me.sh.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("rows")
                        .long("rows")
                        .value_name("FACET")
                        .help("Row facet. Default integration."),
                )
                .arg(
                    Arg::new("cols")
                        .long("cols")
                        .value_name("FACET")
                        .help("Column facet. Default channel."),
                )
                .arg(
                    Arg::new("top-rows")
                        .long("top-rows")
                        .value_name("COUNT")
                        .help("Top row values to return. Default 10, maximum 1000."),
                )
                .arg(
                    Arg::new("top-cols")
                        .long("top-cols")
                        .value_name("COUNT")
                        .help("Top column values to return. Default 10, maximum 1000."),
                )
                .arg(
                    Arg::new("min-count")
                        .long("min-count")
                        .value_name("COUNT")
                        .help("Minimum cell count to return. Default 1."),
                )
                .arg(
                    Arg::new("sample-limit")
                        .long("sample-limit")
                        .value_name("COUNT")
                        .help("Sample contacts per cell. Default 3, maximum 1000."),
                )
                .arg(
                    Arg::new("include-empty")
                        .long("include-empty")
                        .help("Include empty row or column buckets for contacts missing a facet.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return one row per pivot cell instead of nested JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("contacts:overview")
                .about("Build a read-only contact health, duplicate, and facet overview.")
                .arg(
                    Arg::new("input")
                        .long("input")
                        .short('i')
                        .help("Local JSON, JSONL, CSV, or TSV contacts file to analyze.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("input-format")
                        .long("input-format")
                        .default_value("auto")
                        .value_parser(PossibleValuesParser::new([
                            "auto", "json", "jsonl", "csv", "tsv",
                        ]))
                        .help("Input format for --input. auto uses file extension or JSON-looking content."),
                )
                .arg(
                    Arg::new("snapshot-dir")
                        .long("snapshot-dir")
                    .help("Snapshot directory to analyze instead of live me.sh.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("facets")
                        .long("facets")
                        .help("Facet buckets, comma-separated or repeated. Defaults to email-domain,company,title,location,integration,channel.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("dedupe-by")
                        .long("dedupe-by")
                        .help("Duplicate signals, comma-separated or repeated. Defaults to email,phone,linkedin,name.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("min-confidence")
                        .long("min-confidence")
                        .default_value("60")
                        .value_name("PERCENT")
                        .help("Minimum duplicate candidate confidence, 0 to 100."),
                )
                .arg(
                    Arg::new("candidate-limit")
                        .long("candidate-limit")
                        .value_name("COUNT")
                        .help("Maximum duplicate candidate groups to include. Default 10, maximum 1000."),
                )
                .arg(
                    Arg::new("issue-limit")
                        .long("issue-limit")
                        .value_name("COUNT")
                        .help("Maximum issue contact rows to include. Default 20, maximum 1000."),
                )
                .arg(
                    Arg::new("top")
                        .long("top")
                        .value_name("COUNT")
                        .help("Top issue, warning, domain, duplicate, and facet counts to include. Default 10."),
                )
                .arg(
                    Arg::new("min-count")
                        .long("min-count")
                        .value_name("COUNT")
                        .help("Minimum facet bucket count to include. Default 1."),
                )
                .arg(
                    Arg::new("sample-limit")
                        .long("sample-limit")
                        .value_name("COUNT")
                        .help("Sample contacts per facet bucket. Default 3, maximum 1000."),
                )
                .arg(
                    Arg::new("include-empty")
                        .long("include-empty")
                        .help("Include empty facet buckets for contacts missing a facet.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return overview summary, issue, duplicate, and facet rows instead of nested JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("contacts:map")
                .about("Build a read-only contact relationship map from shared facets.")
                .arg(
                    Arg::new("input")
                        .long("input")
                        .short('i')
                        .help("Local contact rows as JSON, JSONL, CSV, or TSV.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("input-format")
                        .long("input-format")
                        .default_value("auto")
                        .value_parser(PossibleValuesParser::new([
                            "auto", "json", "jsonl", "csv", "tsv",
                        ]))
                        .help("Input format for --input. auto uses file extension or JSON-looking content."),
                )
                .arg(
                    Arg::new("snapshot-dir")
                        .long("snapshot-dir")
                        .help("Read contacts from a verified snapshot instead of live search.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("by")
                        .long("by")
                        .help("Facets to connect contacts by. Defaults to email-domain,company,integration,channel.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("min-shared")
                        .long("min-shared")
                        .value_name("COUNT")
                        .help("Minimum contacts sharing a facet bucket. Default 2."),
                )
                .arg(
                    Arg::new("top-buckets")
                        .long("top-buckets")
                        .value_name("COUNT")
                        .help("Maximum facet buckets to return. Default 50."),
                )
                .arg(
                    Arg::new("sample-limit")
                        .long("sample-limit")
                        .value_name("COUNT")
                        .help("Contact samples per bucket. Default 5."),
                )
                .arg(
                    Arg::new("edge-limit")
                        .long("edge-limit")
                        .value_name("COUNT")
                        .help("Maximum contact-to-bucket edges to emit. Default 5000. Use 0 for no edges."),
                )
                .arg(
                    Arg::new("page-size")
                        .long("page-size")
                        .value_name("COUNT")
                        .help("Rows per live search page. Default 1000, maximum 1000."),
                )
                .arg(
                    Arg::new("analyze-limit")
                        .long("analyze-limit")
                        .value_name("COUNT")
                        .help("Maximum live contacts to analyze after search filtering."),
                )
                .arg(
                    Arg::new("include-empty")
                        .long("include-empty")
                        .help("Include missing facet buckets such as (empty).")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return normalized map rows instead of nested JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("contacts:reconnect")
                .about("Rank contacts that need relationship attention using profile and activity signals.")
                .arg(
                    Arg::new("input")
                        .long("input")
                        .short('i')
                        .help("Local contact rows as JSON, JSONL, CSV, or TSV.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("input-format")
                        .long("input-format")
                        .default_value("auto")
                        .value_parser(PossibleValuesParser::new([
                            "auto", "json", "jsonl", "csv", "tsv",
                        ]))
                        .help("Input format for --input. auto uses file extension or JSON-looking content."),
                )
                .arg(
                    Arg::new("snapshot-dir")
                        .long("snapshot-dir")
                        .help("Read contacts from a verified snapshot instead of live search.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("activity-sections")
                        .long("activity-sections")
                        .help("Activity sections. Defaults to recent/upcoming sections. Use all, notes, events, emails, events-upcoming, emails-recent, reminders-recent, reminders-upcoming.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("start")
                        .long("start")
                        .help("Start date required when activity sections include notes, events, or emails."),
                )
                .arg(
                    Arg::new("end")
                        .long("end")
                        .help("End date required when activity sections include notes, events, or emails."),
                )
                .arg(
                    Arg::new("activity-limit")
                        .long("activity-limit")
                        .value_name("COUNT")
                        .help("Page size for recent/upcoming activity sections. Default 100, maximum 1000."),
                )
                .arg(
                    Arg::new("page-size")
                        .long("page-size")
                        .value_name("COUNT")
                        .help("Rows per live search page. Default 1000, maximum 1000."),
                )
                .arg(
                    Arg::new("analyze-limit")
                        .long("analyze-limit")
                        .value_name("COUNT")
                        .help("Maximum live contacts to rank after search filtering."),
                )
                .arg(
                    Arg::new("top")
                        .long("top")
                        .value_name("COUNT")
                        .help("Maximum ranked contacts to return. Default 50, maximum 1000."),
                )
                .arg(
                    Arg::new("low-activity-threshold")
                        .long("low-activity-threshold")
                        .value_name("COUNT")
                        .help("Flag contacts with activity count at or below this threshold. Default 0."),
                )
                .arg(
                    Arg::new("skip-activity")
                        .long("skip-activity")
                        .help("Rank only visible contact profile signals without fetching activity routes.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return one ranked contact row instead of nested JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("contacts:segments")
                .about("Run saved contact search segments and compare their overlap.")
                .arg(
                    Arg::new("input")
                        .long("input")
                        .short('i')
                        .required(true)
                        .help("JSON, JSONL, CSV, or TSV segment definitions to run.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("input-format")
                        .long("input-format")
                        .default_value("auto")
                        .value_parser(PossibleValuesParser::new([
                            "auto", "json", "jsonl", "csv", "tsv",
                        ]))
                        .help("Input format. auto uses file extension or JSON-looking content."),
                )
                .arg(
                    Arg::new("include-fields")
                        .long("include-fields")
                        .help("Fields to include in every segment search. Accepts aliases email, phone, linkedin, work, education, messages, events, notes, and integrations.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("page-size")
                        .long("page-size")
                        .value_name("COUNT")
                        .help("Search page size. Default 1000, maximum 1000."),
                )
                .arg(
                    Arg::new("sample-limit")
                        .long("sample-limit")
                        .value_name("COUNT")
                        .help("Sample contacts per segment. Default 5, maximum 1000."),
                )
                .arg(
                    Arg::new("min-overlap")
                        .long("min-overlap")
                        .value_name("COUNT")
                        .help("Minimum pair overlap to return. Default 1."),
                )
                .arg(
                    Arg::new("min-jaccard")
                        .long("min-jaccard")
                        .value_name("RATIO")
                        .help("Minimum pair Jaccard ratio, 0 to 1. Default 0."),
                )
                .arg(
                    Arg::new("top-overlaps")
                        .long("top-overlaps")
                        .value_name("COUNT")
                        .help("Maximum overlap pairs to return. Default 20."),
                )
                .arg(
                    Arg::new("all-overlaps")
                        .long("all-overlaps")
                        .help("Return every pair that passes overlap filters.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return segment and overlap rows instead of nested JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("contacts:sets")
                .about("Run saved contact search segments and combine them with set algebra.")
                .arg(
                    Arg::new("input")
                        .long("input")
                        .short('i')
                        .required(true)
                        .help("JSON, JSONL, CSV, or TSV segment definitions to run.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("input-format")
                        .long("input-format")
                        .default_value("auto")
                        .value_parser(PossibleValuesParser::new([
                            "auto", "json", "jsonl", "csv", "tsv",
                        ]))
                        .help("Input format. auto uses file extension or JSON-looking content."),
                )
                .arg(
                    Arg::new("include-fields")
                        .long("include-fields")
                        .help("Fields to include in every segment search. Accepts aliases email, phone, linkedin, work, education, messages, events, notes, and integrations.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("page-size")
                        .long("page-size")
                        .value_name("COUNT")
                        .help("Search page size. Default 1000, maximum 1000."),
                )
                .arg(
                    Arg::new("sample-limit")
                        .long("sample-limit")
                        .value_name("COUNT")
                        .help("Sample contacts per segment and result set. Default 5, maximum 1000."),
                )
                .arg(
                    Arg::new("segments")
                        .long("segments")
                        .help("Segment names to use, comma-separated or repeated. Defaults to every segment in input order.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("mode")
                        .long("mode")
                        .default_value("union")
                        .value_parser(PossibleValuesParser::new([
                            "union",
                            "intersection",
                            "first-only",
                            "symmetric-diff",
                        ]))
                        .help("Set operation to run across selected segments."),
                )
                .arg(
                    Arg::new("id-limit")
                        .long("id-limit")
                        .value_name("COUNT")
                        .help("Maximum result contact IDs to return. Default 50, maximum 1000. Use 0 for counts only."),
                )
                .arg(
                    Arg::new("all-ids")
                        .long("all-ids")
                        .help("Return every result contact ID.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return segment, result, and result contact rows instead of nested JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("groups:find")
                .about("List groups whose names contain --query.")
                .arg(
                    Arg::new("query")
                        .long("query")
                        .short('q')
                        .required(true)
                        .help("Case-insensitive substring to match against group names."),
                ),
        )
        .subcommand(
            Command::new("groups:resolve")
                .about("Resolve group names or selectors to group ID candidates.")
                .arg(
                    Arg::new("query")
                        .long("query")
                        .short('q')
                        .help("Case-insensitive substring to match against group names."),
                )
                .arg(
                    Arg::new("group-ids")
                        .long("group-ids")
                        .help("Group IDs or starred, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("candidate-limit")
                        .long("candidate-limit")
                        .value_name("COUNT")
                        .help("Maximum candidate groups to return. Default 20, maximum 1000."),
                )
                .arg(
                    Arg::new("one")
                        .long("one")
                        .help("Require exactly one matching group.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .help("Allow resolving without query or group IDs.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("member-counts")
                        .long("member-counts")
                        .help("Count live members for returned candidate groups.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("concurrency")
                        .long("concurrency")
                        .value_name("COUNT")
                        .help("Concurrent live member-count searches. Default 4, maximum 16."),
                ),
        )
        .subcommand(
            Command::new("groups:profile")
                .about("Show live group metadata, audit signals, member counts, and member samples.")
                .arg(
                    Arg::new("query")
                        .long("query")
                        .short('q')
                        .help("Case-insensitive substring to select group names."),
                )
                .arg(
                    Arg::new("group-ids")
                        .long("group-ids")
                        .help("Group IDs or starred, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .help("Profile every live group.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("one")
                        .long("one")
                        .help("Require exactly one matching group.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("member-limit")
                        .long("member-limit")
                        .value_name("COUNT")
                        .help("Maximum members to sample per group. Default 5. Use 0 for count only."),
                )
                .arg(
                    Arg::new("all-members")
                        .long("all-members")
                        .help("Fetch every member for each selected group.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("include-fields")
                        .long("include-fields")
                        .help("Extra member contact fields to include. Accepts aliases email, phone, linkedin, work, education, messages, events, notes, and integrations.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("page-size")
                        .long("page-size")
                        .value_name("COUNT")
                        .help("Rows per member search page. Default 1000, maximum 1000."),
                )
                .arg(
                    Arg::new("concurrency")
                        .long("concurrency")
                        .value_name("COUNT")
                        .help("Concurrent group member reads. Default 4, maximum 16."),
                ),
        )
        .subcommand(
            Command::new("groups:overlap")
                .about("Find overlapping live group/list member sets across selected groups.")
                .arg(
                    Arg::new("query")
                        .long("query")
                        .short('q')
                        .help("Case-insensitive substring to select group names."),
                )
                .arg(
                    Arg::new("group-ids")
                        .long("group-ids")
                        .help("Group IDs or starred, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .help("Analyze every live group.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("min-overlap")
                        .long("min-overlap")
                        .value_name("COUNT")
                        .help("Minimum shared member count for returned pairs. Default 1. Use 0 to include disjoint pairs."),
                )
                .arg(
                    Arg::new("min-jaccard")
                        .long("min-jaccard")
                        .value_name("RATIO")
                        .help("Minimum Jaccard ratio from 0 to 1 for returned pairs. Default 0."),
                )
                .arg(
                    Arg::new("top")
                        .long("top")
                        .value_name("COUNT")
                        .help("Maximum pair rows to return after sorting by overlap and Jaccard."),
                )
                .arg(
                    Arg::new("page-size")
                        .long("page-size")
                        .value_name("COUNT")
                        .help("Rows per member search page. Default 1000, maximum 1000."),
                )
                .arg(
                    Arg::new("concurrency")
                        .long("concurrency")
                        .value_name("COUNT")
                        .help("Concurrent group member searches. Default 4, maximum 16."),
                ),
        )
        .subcommand(
            Command::new("groups:compare")
                .about("Compare two live group/list member sets without writes.")
                .arg(
                    Arg::new("left-group-id")
                        .long("left-group-id")
                        .help("Left group ID or starred."),
                )
                .arg(
                    Arg::new("left-query")
                        .long("left-query")
                        .help("Case-insensitive substring that must resolve to exactly one left group."),
                )
                .arg(
                    Arg::new("right-group-id")
                        .long("right-group-id")
                        .help("Right group ID or starred."),
                )
                .arg(
                    Arg::new("right-query")
                        .long("right-query")
                        .help("Case-insensitive substring that must resolve to exactly one right group."),
                )
                .arg(
                    Arg::new("include-fields")
                        .long("include-fields")
                        .help("Extra contact fields to include in compared member rows. Accepts aliases email, phone, linkedin, work, education, messages, events, notes, and integrations.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("id-limit")
                        .long("id-limit")
                        .value_name("COUNT")
                        .help("Maximum contact refs to include per overlap/difference set. Default 50. Use 0 for counts only."),
                )
                .arg(
                    Arg::new("all-ids")
                        .long("all-ids")
                        .help("Include every compared contact ref in JSON and flat outputs.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("page-size")
                        .long("page-size")
                        .value_name("COUNT")
                        .help("Rows per member search page. Default 1000, maximum 1000."),
                )
                .arg(
                    Arg::new("concurrency")
                        .long("concurrency")
                        .value_name("COUNT")
                        .help("Concurrent group member searches. Default 4, maximum 16."),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return one row per overlap/left-only/right-only member instead of nested JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("groups:audit")
                .about("Audit groups/lists from live me.sh or a local snapshot.")
                .arg(
                    Arg::new("snapshot-dir")
                        .long("snapshot-dir")
                    .help("Snapshot directory to audit instead of live me.sh.")
                        .value_hint(ValueHint::DirPath),
                )
                .arg(
                    Arg::new("query")
                        .long("query")
                        .short('q')
                        .help("Case-insensitive substring to match against group names."),
                )
                .arg(
                    Arg::new("group-ids")
                        .long("group-ids")
                        .help("Group IDs or starred, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("member-counts")
                        .long("member-counts")
                        .help("Count live members for each selected group using read-only search totals.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("issues-only")
                        .long("issues-only")
                        .help("Only output groups with audit issues.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("concurrency")
                        .long("concurrency")
                        .value_name("COUNT")
                        .help("Concurrent live member-count searches. Default 4, maximum 16."),
                )
                .arg(
                    Arg::new("top")
                        .long("top")
                        .value_name("COUNT")
                        .help("Top duplicate-name and member-count rows to summarize. Default 10."),
                ),
        )
        .subcommand(
            Command::new("groups:members")
                .about("List members of selected me.sh groups using read-only search.")
                .arg(
                    Arg::new("group-ids")
                        .long("group-ids")
                        .help("Group IDs or starred, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("query")
                        .long("query")
                        .short('q')
                        .help("Case-insensitive substring to select groups by name."),
                )
                .arg(
                    Arg::new("all-groups")
                        .long("all-groups")
                        .help("List members for every live group.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("include-fields")
                        .long("include-fields")
                        .help("Extra contact fields to include. Accepts aliases email, phone, linkedin, work, education, messages, events, notes, and integrations.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("limit-per-group")
                        .long("limit-per-group")
                        .value_name("COUNT")
                        .help("Maximum members to return per group. Use 0 to count only."),
                )
                .arg(
                    Arg::new("page-size")
                        .long("page-size")
                        .value_name("COUNT")
                        .help("Rows per search page. Default 1000, maximum 1000."),
                )
                .arg(
                    Arg::new("concurrency")
                        .long("concurrency")
                        .value_name("COUNT")
                        .help("Concurrent group member searches. Default 4, maximum 16."),
                )
                .arg(
                    Arg::new("flat")
                        .long("flat")
                        .help("Return one row per group/member instead of nested group JSON.")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("groups:sync")
                .about("Reconcile one live group/list to a desired contact ID set.")
                .arg(
                    Arg::new("group-id")
                        .long("group-id")
                        .required(true)
                        .help("Numeric group ID to reconcile."),
                )
                .arg(
                    Arg::new("contact-ids")
                        .long("contact-ids")
                        .help("Desired contact IDs, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("input")
                        .long("input")
                        .short('i')
                        .help("File containing desired contact IDs as JSON array, CSV, or one ID per line.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("empty")
                        .long("empty")
                        .help("Use an intentionally empty desired set. Required to clear a group.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("from-search")
                        .long("from-search")
                        .help("Build the desired member set from live contact search filters.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("all-search")
                        .long("all-search")
                        .help("Allow --from-search with no filters, meaning every live contact.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("mode")
                        .long("mode")
                        .default_value("replace")
                        .value_parser(PossibleValuesParser::new([
                            "replace",
                            "add-only",
                            "remove-only",
                        ]))
                        .help("replace computes add and remove deltas; add-only and remove-only constrain the delta."),
                )
                .arg(
                    Arg::new("page-size")
                        .long("page-size")
                        .value_name("COUNT")
                        .help("Rows per current-member search page. Default 1000, maximum 1000."),
                )
                .arg(
                    Arg::new("chunk-size")
                        .long("chunk-size")
                        .value_name("COUNT")
                        .help("Contact IDs per /update-group write chunk. Default 500, maximum 1000."),
                )
                .arg(
                    Arg::new("concurrency")
                        .long("concurrency")
                        .value_name("COUNT")
                        .help("Concurrent /update-group write chunks. Default 4, maximum 16."),
                ),
        )
        .subcommand(group_bulk_membership_command(
            "groups:bulk-add",
            "Add many contacts to many groups/lists from IDs, a file, or live search.",
        ))
        .subcommand(group_bulk_membership_command(
            "groups:bulk-remove",
            "Remove many contacts from many groups/lists from IDs, a file, or live search.",
        ))
        .subcommand(
            Command::new("groups:apply")
                .about("Apply bulk group create/update/member actions from JSON, JSONL, CSV, or TSV.")
                .arg(
                    Arg::new("input")
                        .long("input")
                        .short('i')
                        .required(true)
                        .help("Group action file to read.")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("input-format")
                        .long("input-format")
                        .default_value("auto")
                        .value_parser(PossibleValuesParser::new([
                            "auto", "json", "jsonl", "csv", "tsv",
                        ]))
                        .help("Input format. auto uses file extension or JSON-looking content."),
                )
                .arg(
                    Arg::new("default-action")
                        .long("default-action")
                        .default_value("update")
                        .value_parser(PossibleValuesParser::new([
                            "create", "update", "add", "remove",
                        ]))
                        .help("Action for rows without an action column."),
                )
                .arg(
                    Arg::new("ignore-unknown")
                        .long("ignore-unknown")
                        .help("Ignore unknown input columns instead of failing before writes.")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("concurrency")
                        .long("concurrency")
                        .value_name("COUNT")
                        .help("Concurrent group write requests. Default 4, maximum 16."),
                ),
        )
        .subcommand(
            Command::new("contacts:merge-plan")
                .about("Fetch contacts and show a read-only merge preview.")
                .arg(
                    Arg::new("contact-ids")
                        .long("contact-ids")
                        .required(true)
                        .help("2 to 10 contact IDs, comma-separated or repeated.")
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("concurrency")
                        .long("concurrency")
                        .value_name("COUNT")
                        .help("Concurrent /get-contact requests. Default 4, maximum 16."),
                ),
        )
        .subcommand(Command::new("fish:init").about("Print fish setup lines for mesh."));

    for spec in command_specs() {
        root = root.subcommand(command_from_spec(&spec));
    }

    let search_options = search_options();
    root = patch_subcommand_options(root, "contacts:count", &search_options);
    root = patch_subcommand_options(root, "contacts:export", &search_options);
    let resolve_search_options = search_options
        .iter()
        .filter(|option| option.flag != "limit")
        .cloned()
        .collect::<Vec<_>>();
    root = patch_subcommand_options(root, "contacts:resolve", &resolve_search_options);
    root = patch_subcommand_options(root, "contacts:dedupe", &search_options);
    root = patch_subcommand_options(root, "contacts:quality", &search_options);
    root = patch_subcommand_options(root, "contacts:facets", &search_options);
    root = patch_subcommand_options(root, "contacts:pivot", &search_options);
    root = patch_subcommand_options(root, "contacts:overview", &search_options);
    let map_search_options = search_options
        .iter()
        .filter(|option| option.flag != "limit")
        .cloned()
        .collect::<Vec<_>>();
    root = patch_subcommand_options(root, "contacts:map", &map_search_options);
    root = patch_subcommand_options(root, "contacts:reconnect", &map_search_options);
    let note_bulk_search_options = search_options
        .iter()
        .filter(|option| !matches!(option.flag, "limit" | "include-fields"))
        .cloned()
        .collect::<Vec<_>>();
    root = patch_subcommand_options(root, "notes:bulk-create", &note_bulk_search_options);
    root = patch_subcommand_options(root, "contacts:bulk-archive", &note_bulk_search_options);
    root = patch_subcommand_options(root, "contacts:bulk-restore", &note_bulk_search_options);
    root = patch_subcommand_options(root, "contacts:bulk-update", &note_bulk_search_options);
    let group_sync_search_options = search_options
        .iter()
        .filter(|option| !matches!(option.flag, "limit" | "include-fields"))
        .cloned()
        .collect::<Vec<_>>();
    root = patch_subcommand_options(root, "groups:sync", &group_sync_search_options);
    root = patch_subcommand_options(root, "groups:bulk-add", &group_sync_search_options);
    root = patch_subcommand_options(root, "groups:bulk-remove", &group_sync_search_options);
    root
}

fn group_bulk_membership_command(name: &'static str, about: &'static str) -> Command {
    Command::new(name)
        .about(about)
        .arg(
            Arg::new("target-group-ids")
                .long("target-group-ids")
                .help("Target group IDs, comma-separated or repeated. The special starred list is read-only here.")
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("query")
                .long("query")
                .short('q')
                .help("Case-insensitive substring to select target groups by name."),
        )
        .arg(
            Arg::new("all-groups")
                .long("all-groups")
                .help("Select every writable live group/list.")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("one")
                .long("one")
                .help("Require exactly one writable target group.")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("group-limit")
                .long("group-limit")
                .value_name("COUNT")
                .help("Maximum writable target groups after selection."),
        )
        .arg(
            Arg::new("contact-ids")
                .long("contact-ids")
                .help("Target contact IDs, comma-separated or repeated.")
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("input")
                .long("input")
                .short('i')
                .help("File containing target contact IDs as JSON array, CSV, or one ID per line.")
                .value_hint(ValueHint::FilePath),
        )
        .arg(
            Arg::new("from-search")
                .long("from-search")
                .help("Add target contacts from live contact search filters.")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("all-search")
                .long("all-search")
                .help("Allow --from-search with no filters, meaning every live contact.")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("page-size")
                .long("page-size")
                .value_name("COUNT")
                .help("Rows per live contact search page. Default 1000, maximum 1000."),
        )
        .arg(
            Arg::new("target-limit")
                .long("target-limit")
                .value_name("COUNT")
                .help("Maximum total target contacts after ID and search selection."),
        )
        .arg(
            Arg::new("chunk-size")
                .long("chunk-size")
                .value_name("COUNT")
                .help("Contact IDs per /update-group write chunk. Default 500, maximum 1000."),
        )
        .arg(
            Arg::new("concurrency")
                .long("concurrency")
                .value_name("COUNT")
                .help("Concurrent /update-group write chunks. Default 4, maximum 16."),
        )
        .arg(
            Arg::new("flat")
                .long("flat")
                .help("Return one row per group/contact chunk instead of nested JSON.")
                .action(ArgAction::SetTrue),
        )
}

fn contact_bulk_state_command(name: &'static str, about: &'static str) -> Command {
    Command::new(name)
        .about(about)
        .arg(
            Arg::new("contact-ids")
                .long("contact-ids")
                .help("Target contact IDs, comma-separated or repeated.")
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("input")
                .long("input")
                .short('i')
                .help("File containing target contact IDs as JSON array, CSV, or one ID per line.")
                .value_hint(ValueHint::FilePath),
        )
        .arg(
            Arg::new("from-search")
                .long("from-search")
                .help("Add target contacts from live contact search filters.")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("all-search")
                .long("all-search")
                .help("Allow --from-search with no filters, meaning every live contact.")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("page-size")
                .long("page-size")
                .value_name("COUNT")
                .help("Rows per live search page. Default 1000, maximum 1000."),
        )
        .arg(
            Arg::new("target-limit")
                .long("target-limit")
                .value_name("COUNT")
                .help("Maximum total target contacts after ID and search selection."),
        )
        .arg(
            Arg::new("chunk-size")
                .long("chunk-size")
                .value_name("COUNT")
                .help("Contact IDs per archive/restore request. Default 500, maximum 1000."),
        )
        .arg(
            Arg::new("concurrency")
                .long("concurrency")
                .value_name("COUNT")
                .help("Concurrent archive/restore requests. Default 4, maximum 16."),
        )
        .arg(
            Arg::new("flat")
                .long("flat")
                .help("Return one row per archive/restore chunk instead of nested JSON.")
                .action(ArgAction::SetTrue),
        )
}

fn contact_bulk_update_command() -> Command {
    let mut command =
        Command::new("contacts:bulk-update")
            .about("Apply the same contact field updates to many contacts from IDs, a file, or live search.")
            .arg(
                Arg::new("contact-ids")
                    .long("contact-ids")
                    .help("Target contact IDs, comma-separated or repeated.")
                    .action(ArgAction::Append),
            )
            .arg(
                Arg::new("input")
                    .long("input")
                    .short('i')
                    .help("File containing target contact IDs as JSON array, CSV, or one ID per line.")
                    .value_hint(ValueHint::FilePath),
            )
            .arg(
                Arg::new("from-search")
                    .long("from-search")
                    .help("Add target contacts from live contact search filters.")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("all-search")
                    .long("all-search")
                    .help("Allow --from-search with no filters, meaning every live contact.")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("page-size")
                    .long("page-size")
                    .value_name("COUNT")
                    .help("Rows per live search page. Default 1000, maximum 1000."),
            )
            .arg(
                Arg::new("target-limit")
                    .long("target-limit")
                    .value_name("COUNT")
                    .help("Maximum total target contacts after ID and search selection."),
            )
            .arg(
                Arg::new("concurrency")
                    .long("concurrency")
                    .value_name("COUNT")
                    .help("Concurrent /update-contact writes. Default 4, maximum 16."),
            )
            .arg(
                Arg::new("flat")
                    .long("flat")
                    .help("Return one row per target update instead of nested JSON.")
                    .action(ArgAction::SetTrue),
            );
    for option in contact_mutation_options() {
        command = command.arg(arg_from_option(&option));
    }
    command
}

fn patch_subcommand_options(mut root: Command, name: &str, options: &[OptionSpec]) -> Command {
    let sub = root
        .find_subcommand_mut(name)
        .expect("known generated subcommand");
    for option in options {
        *sub = sub.clone().arg(arg_from_option(option));
    }
    root
}

fn command_from_spec(spec: &CommandSpec) -> Command {
    let mut command = Command::new(spec.name).about(spec.description);
    for option in &spec.options {
        command = command.arg(arg_from_option(option));
    }
    command
}

fn arg_from_option(option: &OptionSpec) -> Arg {
    let mut arg = Arg::new(option.flag)
        .long(option.flag)
        .help(option.description);

    if option.required && option.default.is_none() {
        arg = arg.required(true);
    }

    match option.kind {
        ValueKind::Boolean => arg
            .num_args(0..=1)
            .default_missing_value("true")
            .value_name("BOOL")
            .action(ArgAction::Set),
        ValueKind::ArrayString | ValueKind::ArrayNumber | ValueKind::ArrayMixed => {
            arg.action(ArgAction::Append).num_args(1)
        }
        ValueKind::Json => arg.value_name("JSON").action(ArgAction::Set),
        ValueKind::Number => arg.value_name("NUMBER").action(ArgAction::Set),
        ValueKind::String => {
            if option.allowed.is_empty() {
                arg.action(ArgAction::Set)
            } else {
                arg.action(ArgAction::Set)
                    .value_parser(PossibleValuesParser::new(option.allowed))
            }
        }
    }
}
