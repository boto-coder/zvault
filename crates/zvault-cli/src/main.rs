use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

// ─── CLI definition ──────────────────────────────────────────────────────────

/// ZVault — local-first, end-to-end encrypted password manager.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Import format selector for the `import` subcommand.
#[derive(clap::ValueEnum, Debug, Clone)]
pub enum ImportFormat {
    /// Bitwarden JSON export.
    Bitwarden,
    /// 1Password 1PUX export.
    OnePassword,
    /// LastPass CSV export.
    Lastpass,
    /// KeePass KDBX / XML export.
    Keepass,
    /// Generic CSV with configurable column mapping.
    Csv,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Unlock a vault and start a session.
    Unlock {
        /// Path to the vault file.
        vault: PathBuf,
        /// Master password. Falls back to interactive prompt if not set.
        #[arg(long, env = "ZVAULT_PASSWORD", hide_env_values = true)]
        password: Option<String>,
    },

    /// Lock the active session for a vault.
    Lock {
        /// Path to the vault file.
        vault: PathBuf,
    },

    /// List all items in the vault.
    List {
        /// Path to the vault file.
        vault: PathBuf,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Get a single vault item by ID.
    Get {
        /// Path to the vault file.
        vault: PathBuf,
        /// Item UUID.
        id: String,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Add a new vault item (interactive).
    Add {
        /// Path to the vault file.
        vault: PathBuf,
    },

    /// Edit an existing vault item (interactive).
    Edit {
        /// Path to the vault file.
        vault: PathBuf,
        /// Item UUID.
        id: String,
    },

    /// Delete a vault item.
    Delete {
        /// Path to the vault file.
        vault: PathBuf,
        /// Item UUID.
        id: String,
    },

    /// List all authorised devices.
    Devices {
        /// Path to the vault file.
        vault: PathBuf,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Force an immediate Nostr sync.
    Sync {
        /// Path to the vault file.
        vault: PathBuf,
    },

    /// Import credentials from another password manager.
    Import {
        /// Path to the vault file.
        vault: PathBuf,
        /// Path to the import file.
        file: PathBuf,
        /// Source format.
        #[arg(long, value_enum)]
        format: ImportFormat,
    },

    /// Export the vault.
    Export {
        /// Path to the vault file.
        vault: PathBuf,
        /// Output path for the export file.
        output: PathBuf,
        /// Write a plaintext export instead of encrypted `.zvault-export`.
        /// WARNING: plaintext exports are written to disk unencrypted.
        #[arg(long)]
        plaintext: bool,
    },

    /// View the audit log.
    Audit {
        /// Path to the vault file.
        vault: PathBuf,
        /// Verify the HMAC chain integrity.
        #[arg(long)]
        verify: bool,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
}

// ─── Command stubs ───────────────────────────────────────────────────────────

async fn cmd_unlock(vault: PathBuf, password: Option<String>) -> Result<()> {
    let _ = (vault, password);
    eprintln!("not yet implemented");
    Ok(())
}

async fn cmd_lock(vault: PathBuf) -> Result<()> {
    let _ = vault;
    eprintln!("not yet implemented");
    Ok(())
}

async fn cmd_list(vault: PathBuf, json: bool) -> Result<()> {
    let _ = (vault, json);
    eprintln!("not yet implemented");
    Ok(())
}

async fn cmd_get(vault: PathBuf, id: String, json: bool) -> Result<()> {
    let _ = (vault, id, json);
    eprintln!("not yet implemented");
    Ok(())
}

async fn cmd_add(vault: PathBuf) -> Result<()> {
    let _ = vault;
    eprintln!("not yet implemented");
    Ok(())
}

async fn cmd_edit(vault: PathBuf, id: String) -> Result<()> {
    let _ = (vault, id);
    eprintln!("not yet implemented");
    Ok(())
}

async fn cmd_delete(vault: PathBuf, id: String) -> Result<()> {
    let _ = (vault, id);
    eprintln!("not yet implemented");
    Ok(())
}

async fn cmd_devices(vault: PathBuf, json: bool) -> Result<()> {
    let _ = (vault, json);
    eprintln!("not yet implemented");
    Ok(())
}

async fn cmd_sync(vault: PathBuf) -> Result<()> {
    let _ = vault;
    eprintln!("not yet implemented");
    Ok(())
}

async fn cmd_import(vault: PathBuf, file: PathBuf, format: ImportFormat) -> Result<()> {
    let _ = (vault, file, format);
    eprintln!("not yet implemented");
    Ok(())
}

async fn cmd_export(vault: PathBuf, output: PathBuf, plaintext: bool) -> Result<()> {
    let _ = (vault, output, plaintext);
    eprintln!("not yet implemented");
    Ok(())
}

async fn cmd_audit(vault: PathBuf, verify: bool, json: bool) -> Result<()> {
    let _ = (vault, verify, json);
    eprintln!("not yet implemented");
    Ok(())
}

// ─── Entry point ─────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Unlock { vault, password } => cmd_unlock(vault, password).await,
        Commands::Lock { vault } => cmd_lock(vault).await,
        Commands::List { vault, json } => cmd_list(vault, json).await,
        Commands::Get { vault, id, json } => cmd_get(vault, id, json).await,
        Commands::Add { vault } => cmd_add(vault).await,
        Commands::Edit { vault, id } => cmd_edit(vault, id).await,
        Commands::Delete { vault, id } => cmd_delete(vault, id).await,
        Commands::Devices { vault, json } => cmd_devices(vault, json).await,
        Commands::Sync { vault } => cmd_sync(vault).await,
        Commands::Import {
            vault,
            file,
            format,
        } => cmd_import(vault, file, format).await,
        Commands::Export {
            vault,
            output,
            plaintext,
        } => cmd_export(vault, output, plaintext).await,
        Commands::Audit {
            vault,
            verify,
            json,
        } => cmd_audit(vault, verify, json).await,
    }
}
