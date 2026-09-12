mod config;
mod connect;
mod error;
mod init;
mod inspect;
mod project;
mod status;
mod verify;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::error::CliError;

/// Verification state layer. Not a test runner.
#[derive(Debug, Parser)]
#[command(
    name = "meno",
    version = env!("CARGO_PKG_VERSION"),
    about = "Verification state layer. Not a test runner."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Initialize Meno in this Git repository
    Init,
    /// Discover and configure integrations
    Connect {
        /// Adapter to connect: command, junit, playwright, or agent.
        /// If omitted: print discovery catalog + usage (non-interactive; do not prompt)
        #[arg(long)]
        adapter: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long, num_args = 1..)]
        argv: Vec<String>,
        #[arg(long)]
        can_invoke: bool,
        #[arg(long, value_name = "LEVEL")]
        side_effect_level: Option<String>,
        /// Overwrite an existing adapter of the same name
        #[arg(long)]
        replace: bool,
        /// Serve MCP on stdin/stdout (agent adapter)
        #[arg(long)]
        stdio: bool,
        /// Write project `.mcp.json` and install the Meno Skill (agent adapter)
        #[arg(long)]
        write: bool,
        /// Agent harness: generic or claude (default generic)
        #[arg(long, value_name = "HARNESS")]
        harness: Option<String>,
    },
    /// Collect safe evidence and evaluate claims
    Verify {
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
        /// Ingest JUnit XML, Playwright JSON, or a generic evidence envelope (repeatable)
        #[arg(long = "from", value_name = "PATH")]
        from: Vec<PathBuf>,
        /// Invoke filesystem/network command adapters (never consequential)
        #[arg(long)]
        confirm_invoke: bool,
    },
    /// Fast verification summary without invoking commands
    Status {
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// Inspect a claim or list verification state
    Inspect {
        claim_id: Option<String>,
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
        /// Record a human confirmation for this claim
        #[arg(long, conflicts_with = "export")]
        confirm: bool,
        /// Confirmation statement (required with --confirm)
        #[arg(long)]
        statement: Option<String>,
        /// Optional actor recorded on the confirmation
        #[arg(long)]
        actor: Option<String>,
        /// Optional artifact attached to the confirmation
        #[arg(long)]
        artifact: Option<PathBuf>,
        /// Write a portable evidence bundle directory
        #[arg(long, value_name = "PATH", conflicts_with = "import")]
        export: Option<PathBuf>,
        /// Additive import of a portable evidence bundle
        #[arg(long, value_name = "PATH", conflicts_with = "export")]
        import: Option<PathBuf>,
    },
}

fn main() {
    if let Err(err) = try_main() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<(), CliError> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init => init::run(),
        Commands::Connect {
            adapter,
            name,
            path,
            argv,
            can_invoke,
            side_effect_level,
            replace,
            stdio,
            write,
            harness,
        } => connect::run(connect::ConnectArgs {
            adapter,
            name,
            path,
            argv,
            can_invoke,
            side_effect_level,
            replace,
            stdio,
            write,
            harness,
        }),
        Commands::Verify {
            json,
            from,
            confirm_invoke,
        } => verify::run(json, from, confirm_invoke),
        Commands::Status { json } => status::run(json),
        Commands::Inspect {
            claim_id,
            json,
            confirm,
            statement,
            actor,
            artifact,
            export,
            import,
        } => inspect::run(inspect::InspectArgs {
            claim_id,
            json,
            confirm,
            statement,
            actor,
            artifact,
            export,
            import,
        }),
    }
}
