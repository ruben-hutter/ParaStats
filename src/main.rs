//! parastats CLI entrypoint.
//!
//! Subcommands:
//!   parastats serve [--addr 0.0.0.0:8080] [--db path]
//!       Run migrations, optionally ingest `PARASTATS_INGEST_DIR` (env), serve
//!       the API + SPA on --addr.
//!   parastats ingest <folder> [--db path] [--rebuild]
//!       Scan a folder of .igc files → upsert into the DB, print a summary.
//!   parastats resolve-sites [--db path] [--refresh]
//!       Backfill `site IS NULL` flights from the external takeoff-site
//!       source (OSM Overpass, cached in SQLite; see src/sites.rs).
//!
//! Env: `PARASTATS_ADDR` (bind address, default 0.0.0.0:8080),
//! `PARASTATS_DB` (SQLite path, default ./parastats.db),
//! `PARASTATS_INGEST_DIR` (folder ingested once on `serve` startup),
//! `PARASTATS_OVERPASS_URL` (Overpass interpreter URL; default: mirror list).

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
    /// Backfill missing flight sites from the external takeoff-site DB.
    ResolveSites {
        /// SQLite database path. Env: PARASTATS_DB.
        #[arg(long, env = "PARASTATS_DB", default_value = "./parastats.db")]
        db: PathBuf,
        /// Force a refresh of the cached site list (default: refetch when
        /// older than 7 days).
        #[arg(long)]
        refresh: bool,
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
        Command::ResolveSites { db, refresh } => {
            tokio::runtime::Runtime::new()?.block_on(resolve_sites(db, refresh))
        }
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
        "ingested {} from {}:\n  stored:   {}\n  replaced: {}\n  skipped:  {}\n  failed:   {}\n  sites resolved externally: {}",
        summary.scanned,
        folder.display(),
        summary.stored,
        summary.replaced,
        summary.skipped,
        summary.failed,
        summary.sites_resolved
    );
    for (path, err) in &summary.failures {
        println!("  fail: {}: {err}", path.display());
    }
    Ok(())
}

async fn resolve_sites(db_path: PathBuf, refresh: bool) -> Result<()> {
    let pool = parastats::db::open_db(db_path.to_str().context("db path must be utf-8")?)
        .await
        .with_context(|| format!("opening db {}", db_path.display()))?;
    let s = parastats::sites::backfill_missing(&pool, refresh).await?;
    println!(
        "resolve-sites: {} sites cached; {} flights without site:\n  resolved:   {}\n  unmatched: {} (kept NULL)\n  no coords: {}",
        s.sites, s.considered, s.resolved, s.unmatched, s.no_coords
    );
    Ok(())
}
