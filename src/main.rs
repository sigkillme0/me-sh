//! mesh
//!
//! Module layout:
//! - `cli`, `command_spec` — clap command tree and generated route metadata
//! - `error`, `consts` — shared error type and configuration constants
//! - `output`, `payload` — output formats and CLI/JSON payload coercion
//! - `http`, `config`, `auth`, `runtime` — transport, credentials, the runtime
//! - `fetch`, `routes` — shared live-read primitives and route diagnostics
//! - `contacts`, `groups`, `moments`, `snapshot`, `plan_audit` — workflows
//! - `dispatch` — maps a parsed subcommand to its workflow

mod auth;
mod cli;
mod command_spec;
mod config;
mod consts;
mod contacts;
mod dispatch;
mod error;
mod fetch;
mod groups;
mod http;
mod moments;
mod output;
mod payload;
mod plan_audit;
mod prelude;
mod progress;
mod routes;
mod runtime;
mod snapshot;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    dispatch::install_diagnostics();
    // Usage errors never reach this point: clap prints its own message and
    // exits 2 from `get_matches`.
    let matches = cli::build_cli().get_matches();
    // Decide the error rendering once, before running the command.
    let error_format = error::error_format_from_matches(&matches);
    match run(matches).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(report) => {
            match error_format {
                // Byte-identical to what `Termination` printed when main
                // returned `miette::Result` directly.
                error::ErrorFormat::Human => eprintln!("Error: {report:?}"),
                error::ErrorFormat::Json => eprintln!("{}", error::error_envelope(&report)),
            }
            std::process::ExitCode::from(error::classify_report(&report).exit_code())
        }
    }
}

async fn run(matches: clap::ArgMatches) -> miette::Result<()> {
    let runtime = runtime::Runtime::from_matches(&matches)?;
    dispatch::run(matches, runtime).await
}
