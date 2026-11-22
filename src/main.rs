//! parastats CLI entrypoint.
//!
//! Subcommands:
//!   parastats serve [--addr 0.0.0.0:8080] [--db path]
//!       Run migrations, optionally ingest `PARASTATS_INGEST_DIR` (env), serve
//!       the API + SPA on --addr.
//!   parastats ingest <folder> [--db path] [--rebuild]
//!       Scan a folder of .igc files → upsert into the DB, print a summary.
//!
//! Env: `PARASTATS_ADDR` (bind address, default 0.0.0.0:8080),
//! `PARASTATS_DB` (SQLite path, default ./parastats.db),
//! `PARASTATS_INGEST_DIR` (folder ingested once on `serve` startup).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use parastats::api::{router, AppState};

#[derive(Parser)]
#[command(
    name = "parastats",
    version,
    about = "Self-hosted paragliding flight stats web app from IGC logs"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run migrations, optionally ingest PARASTATS_INGEST_DIR, serve API + SPA.
    Serve {
        /// Bind address, e.g. 0.0.0.0:8080. Env: PARASTATS_ADDR.
        #[arg(long, env = "PARASTATS_ADDR", default_value = "0.0.0.0:8080")]
        addr: String,
        /// SQLite database path. Env: PARASTATS_DB.
        #[arg(long, env = "PARASTATS_DB", default_value = "./parastats.db")]
        db: PathBuf,
    },
    /// Scan a folder of .igc files and upsert them into the database.
    Ingest {
        /// Folder of .igc files (recursive).
        folder: PathBuf,
        /// SQLite database path. Env: PARASTATS_DB.
        #[arg(long, env = "PARASTATS_DB", default_value = "./parastats.db")]
        db: PathBuf,
        /// Re-ingest even files whose content hash already exists in the DB.
        #[arg(long)]
        rebuild: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Serve { addr, db } => tokio::runtime::Runtime::new()?.block_on(serve(addr, db)),
        Command::Ingest {
            folder,
            db,
            rebuild,
        } => tokio::runtime::Runtime::new()?.block_on(ingest(folder, db, rebuild)),
    }
}

async fn serve(addr: String, db_path: PathBuf) -> Result<()> {
    let addr: SocketAddr = addr
        .parse()
        .with_context(|| format!("parsing addr {addr}"))?;
    let pool = parastats::db::open_db(db_path.to_str().context("db path must be utf-8")?)
        .await
        .with_context(|| format!("opening db {}", db_path.display()))?;

    // Optional ingest-on-start (docker compose mounts ./flights read-only).
    if let Ok(dir) = std::env::var("PARASTATS_INGEST_DIR") {
        if !dir.is_empty() && std::path::Path::new(&dir).is_dir() {
            eprintln!("ingesting {dir}…");
            match parastats::ingest::ingest_folder(&pool, std::path::Path::new(&dir), false).await {
                Ok(s) => eprintln!(
                    "ingest: {} scanned, {} stored, {} skipped, {} failed",
                    s.scanned, s.stored, s.skipped, s.failed
                ),
                Err(e) => eprintln!("warn: ingest-on-start failed: {e:#}"),
            }
        }
    }

    let state = Arc::new(AppState { pool });
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    println!("parastats listening on http://{addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

async fn ingest(folder: PathBuf, db_path: PathBuf, rebuild: bool) -> Result<()> {
    let pool = parastats::db::open_db(db_path.to_str().context("db path must be utf-8")?)
        .await
        .with_context(|| format!("opening db {}", db_path.display()))?;
    let summary = parastats::ingest::ingest_folder(&pool, &folder, rebuild).await?;
    println!(
        "ingested {} from {}:\n  stored:   {}\n  replaced: {}\n  skipped:  {}\n  failed:   {}",
        summary.scanned,
        folder.display(),
        summary.stored,
        summary.replaced,
        summary.skipped,
        summary.failed
    );
    for (path, err) in &summary.failures {
        println!("  fail: {}: {err}", path.display());
    }
    Ok(())
}
