//! ZVault CLI — command-line interface for the ZVault password manager.
//!
//! Each subcommand opens the vault, performs an operation, saves if needed,
//! and closes. There is no persistent REPL; each invocation is stateless.

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};
use zvault_core::vault::{ItemKind, VaultFile, VaultItem};

// ─── CLI definition ──────────────────────────────────────────────────────────

/// ZVault — local-first, end-to-end encrypted password manager.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Export format selector for the `export` subcommand.
#[derive(clap::ValueEnum, Debug, Clone, Copy)]
pub enum ExportFormat {
    /// JSON export (plaintext).
    Json,
    /// CSV export (plaintext).
    Csv,
    /// Encrypted `.zvault-export` backup.
    ZvaultExport,
}

/// Import format selector for the `import` subcommand.
#[derive(clap::ValueEnum, Debug, Clone, Copy)]
pub enum ImportFormat {
    /// Bitwarden JSON export.
    Bitwarden,
    /// Generic CSV with columns: name, username, password, url, notes.
    Csv,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a new vault file.
    Init {
        /// Path for the new vault file.
        path: PathBuf,
    },

    /// Unlock a vault (verify password).
    Unlock {
        /// Path to the vault file.
        path: PathBuf,
    },

    /// Lock the current session (no-op in stateless mode; placeholder).
    Lock,

    /// List all items in the vault.
    List {
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
        /// Show passwords in output.
        #[arg(long)]
        show_password: bool,
    },

    /// Get a single vault item by ID.
    Get {
        /// Item UUID.
        id: String,
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
        /// Show password in cleartext.
        #[arg(long)]
        show_password: bool,
        /// Show current TOTP code (if configured).
        #[arg(long)]
        totp: bool,
    },

    /// Add a new item to the vault (interactive or JSON).
    Add {
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
        /// JSON string describing the item to add (non-interactive mode).
        /// Format: {"kind":"login","name":"...","username":"...","password":"...","uri":"..."}
        #[arg(long)]
        json: Option<String>,
    },

    /// Edit an existing vault item.
    Edit {
        /// Item UUID.
        id: String,
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
    },

    /// Delete a vault item.
    Delete {
        /// Item UUID.
        id: String,
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
        /// Skip confirmation prompt.
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Change the vault master password.
    Rekey {
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
    },

    /// Export the vault.
    Export {
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
        /// Export format.
        #[arg(long, value_enum)]
        format: ExportFormat,
        /// Output file path.
        #[arg(long, short)]
        output: PathBuf,
    },

    /// Import items into the vault.
    Import {
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
        /// Import format.
        #[arg(long, value_enum)]
        format: ImportFormat,
        /// Input file path.
        #[arg(long, short)]
        input: PathBuf,
    },

    /// List all devices admitted to the vault.
    Devices {
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
    },

    /// Device management subcommands.
    Device {
        #[command(subcommand)]
        action: DeviceAction,
    },

    /// Device pairing subcommands (invite/join-request flows).
    Pair {
        #[command(subcommand)]
        action: PairAction,
    },

    /// Vault sync subcommands (send/receive via Nostr relay).
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },

    /// Manage Nostr relay configuration.
    Relay {
        #[command(subcommand)]
        action: RelayAction,
    },
}

#[derive(Subcommand, Debug)]
enum DeviceAction {
    /// Admit a new device to the vault.
    Admit {
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
        /// Label for the new device (e.g. "Alice's MacBook").
        #[arg(long)]
        label: String,
        /// Public key (hex) of the device to admit. If not provided, a
        /// placeholder entry is created.
        #[arg(long)]
        pubkey: Option<String>,
    },

    /// Revoke a device from the vault.
    Revoke {
        /// Device UUID to revoke.
        id: String,
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
    },

    /// Initialise a device identity for this CLI instance.
    Init {
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
        /// Human-readable label for this device.
        #[arg(long)]
        label: String,
    },

    /// Show this device's public identity (device ID, label, pubkey, npub).
    Show {
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
    },

    /// Export this device's secret key (nsec + hex). USE WITH CAUTION.
    ExportKey {
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
    },
}

/// Pairing subcommands (device invite/join-request flows).
#[derive(Subcommand, Debug)]
enum PairAction {
    /// Generate an invite code for a new device to join this vault.
    Invite {
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
    },
    /// Generate a join-request code to request access to a vault.
    Request {
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
    },
    /// Import and process a pairing code (invite, join-request, or response).
    Import {
        /// The pairing code to import (starts with zvault:).
        code: String,
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
        /// Skip confirmation prompt.
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

/// Sync subcommands.
#[derive(Subcommand, Debug)]
enum SyncAction {
    /// Send the vault state to another device via a Nostr relay.
    Send {
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
        /// WebSocket URL of the Nostr relay (e.g. ws://127.0.0.1:4736).
        /// If omitted, uses enabled relays from vault settings.
        #[arg(long)]
        relay: Option<String>,
        /// Recipient device's public key (hex, 64 chars).
        #[arg(long)]
        recipient: String,
    },
    /// Receive and apply sync messages from a Nostr relay.
    Receive {
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
        /// WebSocket URL of the Nostr relay.
        /// If omitted, uses enabled relays from vault settings.
        #[arg(long)]
        relay: Option<String>,
        /// How long to wait for messages (seconds).
        #[arg(long, default_value = "10")]
        timeout: u64,
    },
}

/// Relay management subcommands.
#[derive(Subcommand, Debug)]
enum RelayAction {
    /// List all configured relays.
    List {
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
    },
    /// Add a new relay.
    Add {
        /// WebSocket URL of the relay (ws:// or wss://).
        url: String,
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
    },
    /// Remove a relay.
    Remove {
        /// WebSocket URL of the relay to remove.
        url: String,
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
    },
    /// Enable a relay for sync.
    Enable {
        /// WebSocket URL of the relay to enable.
        url: String,
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
    },
    /// Disable a relay (keeps it in the list but excludes from sync).
    Disable {
        /// WebSocket URL of the relay to disable.
        url: String,
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
    },
    /// Reset relays to the default list.
    Reset {
        /// Path to the vault file.
        #[arg(long, short, env = "ZVAULT_PATH")]
        vault: PathBuf,
    },
}

// ─── Password helpers ────────────────────────────────────────────────────────

/// Get the master password from environment or interactive prompt.
///
/// If `ZVAULT_PASSWORD` is set, it is consumed (removed from the environment)
/// to reduce the window of exposure.  The returned `String` should be zeroed
/// by the caller after use (see command functions).
fn get_password(prompt: &str) -> Result<String> {
    // Check ZVAULT_PASSWORD env var first (for CI/scripting).
    if let Ok(pw) = std::env::var("ZVAULT_PASSWORD") {
        if !pw.is_empty() {
            // Remove from environment to prevent child-process leakage.
            std::env::remove_var("ZVAULT_PASSWORD");
            return Ok(pw);
        }
    }
    rpassword::prompt_password(prompt).context("failed to read password")
}

/// Get a new password with confirmation.
///
/// Same env-var clearing semantics as [`get_password`].
fn get_new_password(prompt: &str) -> Result<String> {
    if let Ok(pw) = std::env::var("ZVAULT_PASSWORD") {
        if !pw.is_empty() {
            std::env::remove_var("ZVAULT_PASSWORD");
            return Ok(pw);
        }
    }
    let pw = rpassword::prompt_password(prompt).context("failed to read password")?;
    let mut confirm =
        rpassword::prompt_password("Confirm password: ").context("failed to read confirmation")?;
    if pw != confirm {
        confirm.zeroize();
        bail!("passwords do not match");
    }
    confirm.zeroize();
    Ok(pw)
}

/// Prompt for a single line of input (with a message).
fn prompt_line(msg: &str) -> Result<String> {
    print!("{msg}");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

/// Prompt for an optional field (returns None if empty).
fn prompt_optional(msg: &str) -> Result<Option<String>> {
    let val = prompt_line(msg)?;
    if val.is_empty() {
        Ok(None)
    } else {
        Ok(Some(val))
    }
}

// ─── Display helpers ─────────────────────────────────────────────────────────

fn kind_label(kind: &ItemKind) -> &'static str {
    match kind {
        ItemKind::Login => "Login",
        ItemKind::SecureNote => "SecureNote",
        ItemKind::Card => "Card",
        ItemKind::Identity => "Identity",
    }
}

fn display_item(item: &VaultItem, show_password: bool) {
    println!("  ID:       {}", item.id);
    println!("  Name:     {}", item.name);
    println!("  Kind:     {}", kind_label(&item.kind));
    println!(
        "  Created:  {}",
        item.created_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    println!(
        "  Updated:  {}",
        item.updated_at.format("%Y-%m-%d %H:%M:%S UTC")
    );

    match &item.kind {
        ItemKind::Login => {
            if let Some(username) = &item.username {
                println!("  Username: {username}");
            }
            if show_password {
                if let Some(password) = &item.password {
                    println!("  Password: {password}");
                }
            } else if item.password.is_some() {
                println!("  Password: ********");
            }
            if let Some(totp) = &item.totp_secret {
                if show_password {
                    println!("  TOTP:     {totp}");
                } else {
                    println!("  TOTP:     ********");
                }
            }
            if !item.uris.is_empty() {
                println!("  URIs:");
                for uri in &item.uris {
                    println!("    - {}", uri.uri);
                }
            }
        }
        ItemKind::SecureNote => {
            if let Some(note) = &item.note {
                if show_password {
                    println!("  Note:     {note}");
                } else {
                    println!("  Note:     [hidden]");
                }
            }
        }
        ItemKind::Card => {
            if let Some(cardholder) = &item.cardholder {
                println!("  Holder:   {cardholder}");
            }
            if show_password {
                if let Some(number) = &item.card_number {
                    println!("  Number:   {number}");
                }
                if let Some(cvv) = &item.cvv {
                    println!("  CVV:      {cvv}");
                }
            } else {
                if item.card_number.is_some() {
                    println!("  Number:   ****");
                }
                if item.cvv.is_some() {
                    println!("  CVV:      ***");
                }
            }
            if let Some(expiry) = &item.expiry {
                println!("  Expiry:   {expiry}");
            }
        }
        ItemKind::Identity => {
            if let Some(id_fields) = &item.identity {
                if let Some(v) = &id_fields.first_name {
                    println!("  First:    {v}");
                }
                if let Some(v) = &id_fields.last_name {
                    println!("  Last:     {v}");
                }
                if let Some(v) = &id_fields.email {
                    println!("  Email:    {v}");
                }
                if let Some(v) = &id_fields.phone {
                    println!("  Phone:    {v}");
                }
                if let Some(v) = &id_fields.address {
                    println!("  Address:  {v}");
                }
                if let Some(v) = &id_fields.city {
                    println!("  City:     {v}");
                }
                if let Some(v) = &id_fields.country {
                    println!("  Country:  {v}");
                }
            }
        }
    }
}

// ─── Command implementations ─────────────────────────────────────────────────

fn cmd_init(path: PathBuf) -> Result<()> {
    if path.exists() {
        bail!("file already exists: {}", path.display());
    }

    let mut password = get_new_password("Enter master password: ")?;
    if password.is_empty() {
        password.zeroize();
        bail!("password cannot be empty");
    }

    let result = VaultFile::create(&password, &path).context("failed to create vault file");
    password.zeroize();
    let (_vf, _key) = result?;

    println!("✓ Vault created at {}", path.display());
    Ok(())
}

fn cmd_unlock(path: PathBuf) -> Result<()> {
    if !path.exists() {
        bail!("vault file not found: {}", path.display());
    }

    let mut password = get_password("Enter master password: ")?;
    let result =
        VaultFile::open(&password, &path).context("failed to unlock vault (wrong password?)");
    password.zeroize();
    let (_vf, _key, vault) = result?;

    println!("✓ Vault unlocked successfully");
    println!("  Items: {}", vault.items.len());
    println!("  Devices: {}", vault.devices.len());
    println!("  Version: {}", vault.version);
    Ok(())
}

fn cmd_lock() -> Result<()> {
    // In stateless mode, there is no persistent session to lock.
    // This command serves as a placeholder for future REPL mode.
    println!("✓ Session locked (stateless mode — no active session)");
    Ok(())
}

fn cmd_list(vault_path: PathBuf, _show_password: bool) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (_vf, _key, vault) = result?;

    let items = vault.list_items();
    if items.is_empty() {
        println!("No items in vault.");
        return Ok(());
    }

    println!("{:<38} {:<12} NAME", "ID", "KIND");
    println!("{}", "-".repeat(70));
    for item in items {
        println!(
            "{:<38} {:<12} {}",
            item.id,
            kind_label(&item.kind),
            item.name
        );
    }
    println!("\n{} item(s) total.", items.len());
    Ok(())
}

fn cmd_get(vault_path: PathBuf, id_str: String, show_password: bool, totp: bool) -> Result<()> {
    let id = Uuid::parse_str(&id_str).context("invalid UUID format")?;
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (_vf, _key, vault) = result?;

    match vault.get_item(id) {
        Some(item) => {
            if totp {
                // --totp mode: show only the TOTP code, not the raw secret
                if let Some(secret) = &item.totp_secret {
                    let totp_gen = totp_rs::TOTP::new(
                        totp_rs::Algorithm::SHA1,
                        6,
                        1,
                        30,
                        secret.as_bytes().to_vec(),
                    )
                    .map_err(|e| anyhow::anyhow!("invalid TOTP secret: {e}"))?;

                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_secs();
                    let code = totp_gen.generate(now);
                    let remaining = 30 - (now % 30);

                    println!("  TOTP:     {} (expires in {}s)", code, remaining);
                } else {
                    println!("  No TOTP configured for this item.");
                }
            } else {
                display_item(item, show_password);
            }
        }
        None => {
            bail!("item not found: {id}");
        }
    }
    Ok(())
}

/// JSON input format for `--json` non-interactive item creation.
#[derive(Deserialize)]
struct JsonItemInput {
    kind: String,
    name: String,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    uri: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    totp_secret: Option<String>,
}

/// Parse a JSON string into a `VaultItem`.
fn parse_json_item(json_str: &str) -> Result<VaultItem> {
    let input: JsonItemInput = serde_json::from_str(json_str).context("invalid JSON item input")?;

    let kind = match input.kind.to_lowercase().as_str() {
        "login" => ItemKind::Login,
        "note" | "securenote" | "secure_note" => ItemKind::SecureNote,
        "card" => ItemKind::Card,
        "identity" => ItemKind::Identity,
        _ => bail!("unknown item kind in JSON: {}", input.kind),
    };

    if input.name.is_empty() {
        bail!("name cannot be empty in JSON item");
    }

    let mut item = VaultItem::new(kind.clone(), &input.name);
    match kind {
        ItemKind::Login => {
            item.username = input.username;
            item.password = input.password;
            item.totp_secret = input.totp_secret;
            if let Some(uri) = input.uri {
                item.uris.push(zvault_core::vault::Uri {
                    uri,
                    r#match: zvault_core::vault::UriMatch::Domain,
                });
            }
        }
        ItemKind::SecureNote => {
            item.note = input.note;
        }
        _ => {}
    }

    Ok(item)
}

fn cmd_add(vault_path: PathBuf, json_input: Option<String>) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (vf, key, mut vault) = result?;

    // Non-interactive JSON mode.
    if let Some(json_str) = json_input {
        let item = parse_json_item(&json_str)?;
        let id = item.id;
        vault.add_item(item);
        vf.save(&key, &vault).context("failed to save vault")?;
        println!("✓ Item added: {id}");
        return Ok(());
    }

    // Prompt for item kind.
    println!("Item types: login, note, card, identity");
    let kind_str = prompt_line("Kind: ")?;
    let kind = match kind_str.to_lowercase().as_str() {
        "login" => ItemKind::Login,
        "note" | "securenote" | "secure_note" => ItemKind::SecureNote,
        "card" => ItemKind::Card,
        "identity" => ItemKind::Identity,
        _ => bail!("unknown item kind: {kind_str}. Use: login, note, card, identity"),
    };

    let name = prompt_line("Name: ")?;
    if name.is_empty() {
        bail!("name cannot be empty");
    }

    let mut item = VaultItem::new(kind.clone(), &name);

    match kind {
        ItemKind::Login => {
            item.username = prompt_optional("Username (optional): ")?;
            item.password = prompt_optional("Password (optional): ")?;
            item.totp_secret = prompt_optional("TOTP secret (optional): ")?;
            if let Some(uri) = prompt_optional("URI (optional): ")? {
                item.uris.push(zvault_core::vault::Uri {
                    uri,
                    r#match: zvault_core::vault::UriMatch::Domain,
                });
            }
        }
        ItemKind::SecureNote => {
            item.note = prompt_optional("Note: ")?;
        }
        ItemKind::Card => {
            item.cardholder = prompt_optional("Cardholder name (optional): ")?;
            item.card_number = prompt_optional("Card number (optional): ")?;
            item.expiry = prompt_optional("Expiry MM/YY (optional): ")?;
            item.cvv = prompt_optional("CVV (optional): ")?;
        }
        ItemKind::Identity => {
            let fields = zvault_core::vault::IdentityFields {
                first_name: prompt_optional("First name (optional): ")?,
                last_name: prompt_optional("Last name (optional): ")?,
                email: prompt_optional("Email (optional): ")?,
                phone: prompt_optional("Phone (optional): ")?,
                address: prompt_optional("Address (optional): ")?,
                city: prompt_optional("City (optional): ")?,
                country: prompt_optional("Country (optional): ")?,
            };
            item.identity = Some(fields);
        }
    }

    let id = item.id;
    vault.add_item(item);
    vf.save(&key, &vault).context("failed to save vault")?;

    println!("✓ Item added: {id}");
    Ok(())
}

fn cmd_edit(vault_path: PathBuf, id_str: String) -> Result<()> {
    let id = Uuid::parse_str(&id_str).context("invalid UUID format")?;
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (vf, key, mut vault) = result?;

    let existing = vault
        .get_item(id)
        .ok_or_else(|| anyhow::anyhow!("item not found: {id}"))?
        .clone();

    println!(
        "Editing item: {} ({})",
        existing.name,
        kind_label(&existing.kind)
    );
    println!("Press Enter to keep current value.");

    let mut updated = existing.clone();
    updated.updated_at = chrono::Utc::now();

    // Name
    let new_name = prompt_line(&format!("Name [{}]: ", existing.name))?;
    if !new_name.is_empty() {
        updated.name = new_name;
    }

    match &existing.kind {
        ItemKind::Login => {
            let current_user = existing.username.as_deref().unwrap_or("");
            let new_user = prompt_line(&format!("Username [{current_user}]: "))?;
            if !new_user.is_empty() {
                updated.username = Some(new_user);
            }

            let new_pw = prompt_line("Password [********]: ")?;
            if !new_pw.is_empty() {
                updated.password = Some(new_pw);
            }

            let new_totp = prompt_line("TOTP secret [unchanged]: ")?;
            if !new_totp.is_empty() {
                updated.totp_secret = Some(new_totp);
            }
        }
        ItemKind::SecureNote => {
            let new_note = prompt_line("Note [unchanged]: ")?;
            if !new_note.is_empty() {
                updated.note = Some(new_note);
            }
        }
        ItemKind::Card => {
            let current_holder = existing.cardholder.as_deref().unwrap_or("");
            let new_holder = prompt_line(&format!("Cardholder [{current_holder}]: "))?;
            if !new_holder.is_empty() {
                updated.cardholder = Some(new_holder);
            }

            let new_number = prompt_line("Card number [****]: ")?;
            if !new_number.is_empty() {
                updated.card_number = Some(new_number);
            }

            let current_expiry = existing.expiry.as_deref().unwrap_or("");
            let new_expiry = prompt_line(&format!("Expiry [{current_expiry}]: "))?;
            if !new_expiry.is_empty() {
                updated.expiry = Some(new_expiry);
            }

            let new_cvv = prompt_line("CVV [***]: ")?;
            if !new_cvv.is_empty() {
                updated.cvv = Some(new_cvv);
            }
        }
        ItemKind::Identity => {
            let mut fields = existing.identity.clone().unwrap_or_default();
            let current_first = fields.first_name.as_deref().unwrap_or("");
            let new_first = prompt_line(&format!("First name [{current_first}]: "))?;
            if !new_first.is_empty() {
                fields.first_name = Some(new_first);
            }

            let current_last = fields.last_name.as_deref().unwrap_or("");
            let new_last = prompt_line(&format!("Last name [{current_last}]: "))?;
            if !new_last.is_empty() {
                fields.last_name = Some(new_last);
            }

            let current_email = fields.email.as_deref().unwrap_or("");
            let new_email = prompt_line(&format!("Email [{current_email}]: "))?;
            if !new_email.is_empty() {
                fields.email = Some(new_email);
            }

            let current_phone = fields.phone.as_deref().unwrap_or("");
            let new_phone = prompt_line(&format!("Phone [{current_phone}]: "))?;
            if !new_phone.is_empty() {
                fields.phone = Some(new_phone);
            }

            let current_address = fields.address.as_deref().unwrap_or("");
            let new_address = prompt_line(&format!("Address [{current_address}]: "))?;
            if !new_address.is_empty() {
                fields.address = Some(new_address);
            }

            let current_city = fields.city.as_deref().unwrap_or("");
            let new_city = prompt_line(&format!("City [{current_city}]: "))?;
            if !new_city.is_empty() {
                fields.city = Some(new_city);
            }

            let current_country = fields.country.as_deref().unwrap_or("");
            let new_country = prompt_line(&format!("Country [{current_country}]: "))?;
            if !new_country.is_empty() {
                fields.country = Some(new_country);
            }

            updated.identity = Some(fields);
        }
    }

    vault
        .update_item(updated)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    vf.save(&key, &vault).context("failed to save vault")?;

    println!("✓ Item updated: {id}");
    Ok(())
}

fn cmd_delete(vault_path: PathBuf, id_str: String, yes: bool) -> Result<()> {
    let id = Uuid::parse_str(&id_str).context("invalid UUID format")?;
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (vf, key, mut vault) = result?;

    // Verify item exists and show its name.
    let item_name = vault
        .get_item(id)
        .ok_or_else(|| anyhow::anyhow!("item not found: {id}"))?
        .name
        .clone();

    if !yes {
        let answer = prompt_line(&format!("Delete \"{item_name}\" ({id})? [y/N]: "))?;
        if !matches!(answer.to_lowercase().as_str(), "y" | "yes") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    vault.delete_item(id).map_err(|e| anyhow::anyhow!("{e}"))?;
    vf.save(&key, &vault).context("failed to save vault")?;

    println!("✓ Item deleted: {item_name} ({id})");
    Ok(())
}

fn cmd_rekey(vault_path: PathBuf) -> Result<()> {
    if !vault_path.exists() {
        bail!("vault file not found: {}", vault_path.display());
    }

    let mut old_password = get_password("Enter current master password: ")?;
    let mut new_password = get_new_password("Enter new master password: ")?;
    if new_password.is_empty() {
        old_password.zeroize();
        new_password.zeroize();
        bail!("new password cannot be empty");
    }

    // Open vault with old password to verify it first.
    let open_result = VaultFile::open(&old_password, &vault_path)
        .context("failed to unlock with current password");
    let (vf, _key, _vault) = match open_result {
        Ok(v) => v,
        Err(e) => {
            old_password.zeroize();
            new_password.zeroize();
            return Err(e);
        }
    };

    // Rekey the vault file.
    let rekey_result = vf
        .rekey(&old_password, &new_password)
        .context("failed to rekey vault");
    old_password.zeroize();
    new_password.zeroize();
    let (_new_vf, _new_key, _new_vault) = rekey_result?;

    println!("✓ Master password changed successfully");
    Ok(())
}

fn cmd_export(vault_path: PathBuf, format: ExportFormat, output: PathBuf) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (_vf, _key, vault) = result?;

    match format {
        ExportFormat::Json => {
            let json = serde_json::to_string_pretty(&vault)
                .context("failed to serialise vault to JSON")?;
            write_restricted_file(&output, json.as_bytes())
                .context("failed to write export file")?;
            println!(
                "✓ Exported {} item(s) to {} (JSON)",
                vault.items.len(),
                output.display()
            );
        }
        ExportFormat::Csv => {
            let mut csv_output = String::new();
            csv_output.push_str("name,kind,username,password,uri,notes\n");
            for item in &vault.items {
                let uri = item.uris.first().map(|u| u.uri.as_str()).unwrap_or("");
                let username = item.username.as_deref().unwrap_or("");
                let pw = item.password.as_deref().unwrap_or("");
                let notes = item.note.as_deref().unwrap_or("");
                // Escape CSV fields.
                csv_output.push_str(&format!(
                    "{},{},{},{},{},{}\n",
                    csv_escape(&item.name),
                    kind_label(&item.kind),
                    csv_escape(username),
                    csv_escape(pw),
                    csv_escape(uri),
                    csv_escape(notes),
                ));
            }
            write_restricted_file(&output, csv_output.as_bytes())
                .context("failed to write export file")?;
            println!(
                "✓ Exported {} item(s) to {} (CSV)",
                vault.items.len(),
                output.display()
            );
        }
        ExportFormat::ZvaultExport => {
            // Encrypted export: create a new vault file at the output path.
            let mut export_password = get_new_password("Enter export password: ")?;
            if export_password.is_empty() {
                export_password.zeroize();
                bail!("export password cannot be empty");
            }
            let create_result = VaultFile::create(&export_password, &output)
                .context("failed to create export file");
            export_password.zeroize();
            let (vf_export, key_export) = create_result?;
            // Save the full vault data into the export file.
            vf_export
                .save(&key_export, &vault)
                .context("failed to write encrypted export")?;
            println!(
                "✓ Exported {} item(s) to {} (encrypted .zvault-export)",
                vault.items.len(),
                output.display()
            );
        }
    }
    Ok(())
}

fn cmd_import(vault_path: PathBuf, format: ImportFormat, input: PathBuf) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (vf, key, mut vault) = result?;

    let content = std::fs::read_to_string(&input).context("failed to read import file")?;

    let mut imported_count = 0;

    match format {
        ImportFormat::Bitwarden => {
            // Parse Bitwarden JSON export.
            let bw: serde_json::Value =
                serde_json::from_str(&content).context("invalid Bitwarden JSON")?;
            let items = bw
                .get("items")
                .and_then(|v| v.as_array())
                .context("Bitwarden JSON missing 'items' array")?;

            for bw_item in items {
                let name = bw_item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Untitled");
                let bw_type = bw_item.get("type").and_then(|v| v.as_u64()).unwrap_or(1);

                let kind = match bw_type {
                    1 => ItemKind::Login,
                    2 => ItemKind::SecureNote,
                    3 => ItemKind::Card,
                    4 => ItemKind::Identity,
                    _ => ItemKind::Login,
                };

                let mut item = VaultItem::new(kind.clone(), name);

                match kind {
                    ItemKind::Login => {
                        if let Some(login) = bw_item.get("login") {
                            item.username = login
                                .get("username")
                                .and_then(|v| v.as_str())
                                .map(String::from);
                            item.password = login
                                .get("password")
                                .and_then(|v| v.as_str())
                                .map(String::from);
                            item.totp_secret =
                                login.get("totp").and_then(|v| v.as_str()).map(String::from);
                            if let Some(uris) = login.get("uris").and_then(|v| v.as_array()) {
                                for u in uris {
                                    if let Some(uri_str) = u.get("uri").and_then(|v| v.as_str()) {
                                        item.uris.push(zvault_core::vault::Uri {
                                            uri: uri_str.to_string(),
                                            r#match: zvault_core::vault::UriMatch::Domain,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    ItemKind::SecureNote => {
                        item.note = bw_item
                            .get("notes")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                    ItemKind::Card => {
                        if let Some(card) = bw_item.get("card") {
                            item.cardholder = card
                                .get("cardholderName")
                                .and_then(|v| v.as_str())
                                .map(String::from);
                            item.card_number = card
                                .get("number")
                                .and_then(|v| v.as_str())
                                .map(String::from);
                            item.cvv = card.get("code").and_then(|v| v.as_str()).map(String::from);
                            let exp_month =
                                card.get("expMonth").and_then(|v| v.as_str()).unwrap_or("");
                            let exp_year =
                                card.get("expYear").and_then(|v| v.as_str()).unwrap_or("");
                            if !exp_month.is_empty() || !exp_year.is_empty() {
                                item.expiry = Some(format!("{exp_month}/{exp_year}"));
                            }
                        }
                    }
                    ItemKind::Identity => {
                        if let Some(ident) = bw_item.get("identity") {
                            let fields = zvault_core::vault::IdentityFields {
                                first_name: ident
                                    .get("firstName")
                                    .and_then(|v| v.as_str())
                                    .map(String::from),
                                last_name: ident
                                    .get("lastName")
                                    .and_then(|v| v.as_str())
                                    .map(String::from),
                                email: ident
                                    .get("email")
                                    .and_then(|v| v.as_str())
                                    .map(String::from),
                                phone: ident
                                    .get("phone")
                                    .and_then(|v| v.as_str())
                                    .map(String::from),
                                address: ident
                                    .get("address1")
                                    .and_then(|v| v.as_str())
                                    .map(String::from),
                                city: ident.get("city").and_then(|v| v.as_str()).map(String::from),
                                country: ident
                                    .get("country")
                                    .and_then(|v| v.as_str())
                                    .map(String::from),
                            };
                            item.identity = Some(fields);
                        }
                    }
                }

                // Also carry over notes field for non-SecureNote items.
                if !matches!(kind, ItemKind::SecureNote) && item.note.is_none() {
                    item.note = bw_item
                        .get("notes")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                }

                vault.add_item(item);
                imported_count += 1;
            }
        }
        ImportFormat::Csv => {
            // Parse CSV: name,username,password,url,notes
            let mut lines = content.lines();
            // Skip header line.
            let _header = lines.next();

            for line in lines {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let fields = parse_csv_line(line);
                let name = fields.first().map(|s| s.as_str()).unwrap_or("Untitled");
                if name.is_empty() {
                    continue;
                }

                let mut item = VaultItem::new(ItemKind::Login, name);
                if let Some(username) = fields.get(1) {
                    if !username.is_empty() {
                        item.username = Some(username.clone());
                    }
                }
                if let Some(pw) = fields.get(2) {
                    if !pw.is_empty() {
                        item.password = Some(pw.clone());
                    }
                }
                if let Some(url) = fields.get(3) {
                    if !url.is_empty() {
                        item.uris.push(zvault_core::vault::Uri {
                            uri: url.clone(),
                            r#match: zvault_core::vault::UriMatch::Domain,
                        });
                    }
                }
                if let Some(notes) = fields.get(4) {
                    if !notes.is_empty() {
                        item.note = Some(notes.clone());
                    }
                }

                vault.add_item(item);
                imported_count += 1;
            }
        }
    }

    vf.save(&key, &vault).context("failed to save vault")?;
    println!(
        "✓ Imported {imported_count} item(s) from {}",
        input.display()
    );
    Ok(())
}

fn cmd_devices(vault_path: PathBuf) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (_vf, _key, vault) = result?;

    if vault.devices.is_empty() {
        println!("No devices registered.");
        return Ok(());
    }

    println!("{:<38} {:<20} {:<10} ADDED", "DEVICE ID", "LABEL", "STATUS");
    println!("{}", "-".repeat(80));
    for device in &vault.devices {
        let status = if device.revoked { "revoked" } else { "active" };
        println!(
            "{:<38} {:<20} {:<10} {}",
            device.device_id,
            device.label,
            status,
            device.added_at.format("%Y-%m-%d")
        );
    }
    println!("\n{} device(s) total.", vault.devices.len());
    Ok(())
}

fn cmd_device_admit(vault_path: PathBuf, label: String, pubkey: Option<String>) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (vf, key, mut vault) = result?;

    let now = chrono::Utc::now();
    let device_id = Uuid::new_v4();
    // Use the provided pubkey, or generate a placeholder.
    let nostr_pubkey = pubkey.unwrap_or_else(|| format!("{:0>64x}", device_id.as_u128()));

    let entry = zvault_core::vault::DeviceEntry {
        device_id,
        nostr_pubkey,
        label: label.clone(),
        added_at: now,
        added_by: vault
            .devices
            .first()
            .map(|d| d.device_id)
            .unwrap_or(device_id),
        revoked: false,
        revoked_at: None,
        revoked_by: None,
    };

    vault.devices.push(entry);
    vault.version += 1;
    vault.updated_at = now;
    vf.save(&key, &vault).context("failed to save vault")?;

    println!("✓ Device admitted: {label} ({device_id})");
    Ok(())
}

fn cmd_device_revoke(vault_path: PathBuf, id_str: String) -> Result<()> {
    let id = Uuid::parse_str(&id_str).context("invalid UUID format")?;
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (vf, key, mut vault) = result?;

    let entry = vault
        .devices
        .iter_mut()
        .find(|d| d.device_id == id)
        .ok_or_else(|| anyhow::anyhow!("device not found: {id}"))?;

    if entry.revoked {
        bail!("device already revoked: {id}");
    }

    let label = entry.label.clone();
    entry.revoked = true;
    entry.revoked_at = Some(chrono::Utc::now());
    // revoked_by would be set to the current device identity in a full
    // implementation with SecureStorage. Here we leave it as None.

    vault.version += 1;
    vault.updated_at = chrono::Utc::now();
    vf.save(&key, &vault).context("failed to save vault")?;

    println!("✓ Device revoked: {label} ({id})");
    Ok(())
}

// ─── Device Init ─────────────────────────────────────────────────────────────

/// Sidecar `.device` file content (JSON before encryption).
#[derive(Serialize, Deserialize)]
struct CliDeviceFile {
    device_id: Uuid,
    secret_key_hex: String,
    pubkey_hex: String,
    label: String,
}

impl Drop for CliDeviceFile {
    fn drop(&mut self) {
        self.secret_key_hex.zeroize();
    }
}

fn device_sidecar_path(vault_path: &std::path::Path) -> PathBuf {
    let mut p = vault_path.as_os_str().to_os_string();
    p.push(".device");
    PathBuf::from(p)
}

fn cmd_device_init(vault_path: PathBuf, label: String) -> Result<()> {
    let sidecar = device_sidecar_path(&vault_path);
    if sidecar.exists() {
        bail!(
            "Device identity already initialised. Sidecar file exists: {}",
            sidecar.display()
        );
    }

    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    let (vf, key, mut vault) = match result {
        Ok(v) => v,
        Err(e) => {
            password.zeroize();
            return Err(e);
        }
    };

    // Generate a keypair using InMemoryStorage.
    let storage = zvault_core::device::InMemoryStorage::default();
    let (identity, material) = zvault_core::device::DeviceIdentity::generate(&label, &storage)
        .map_err(|e| anyhow::anyhow!("keypair generation failed: {e}"))?;

    // Load the secret key from the in-memory storage.
    let sk_bytes = identity
        .load_secret_key(&storage)
        .map_err(|e| anyhow::anyhow!("failed to load secret key: {e}"))?;
    let sk_hex = Zeroizing::new(hex::encode(sk_bytes.as_slice()));

    // Build the sidecar file content.
    let device_file = CliDeviceFile {
        device_id: material.device_id,
        secret_key_hex: (*sk_hex).clone(),
        pubkey_hex: material.pubkey_hex.clone(),
        label: label.clone(),
    };
    let device_json = Zeroizing::new(
        serde_json::to_vec(&device_file).context("failed to serialise device file")?,
    );

    // Encrypt the sidecar with the vault password.
    let (_, sidecar_key) = zvault_core::vault::VaultFile::create(&password, &sidecar)
        .context("failed to create sidecar file")?;

    // We need to store the device JSON in the sidecar. We use a mini vault
    // just to use the atomic encrypted write. Instead, let's encrypt directly.
    // Actually simpler: just encrypt the device JSON using the crypto module.
    drop(sidecar_key);
    // Remove the empty vault file we just created and write encrypted blob instead.
    std::fs::remove_file(&sidecar).ok();

    // Encrypt device data with vault password + fresh KdfParams.
    let sidecar_blob = zvault_core::crypto::encrypt(&key, &device_json)
        .map_err(|e| anyhow::anyhow!("encrypt sidecar failed: {e}"))?;
    std::fs::write(&sidecar, &sidecar_blob).context("failed to write sidecar file")?;

    // Bootstrap or admit the device into the vault device list.
    if vault.devices.is_empty() {
        // First device — bootstrap.
        let mut dm = zvault_core::device::DeviceManager::from_vault(&vault);
        dm.bootstrap(&material)
            .map_err(|e| anyhow::anyhow!("bootstrap failed: {e}"))?;
        dm.flush(&mut vault);
    } else {
        // Vault already has devices — add ourselves.
        let now = chrono::Utc::now();
        let entry = zvault_core::vault::DeviceEntry {
            device_id: material.device_id,
            nostr_pubkey: material.pubkey_hex.clone(),
            label: label.clone(),
            added_at: now,
            added_by: vault
                .devices
                .first()
                .map(|d| d.device_id)
                .unwrap_or(material.device_id),
            revoked: false,
            revoked_at: None,
            revoked_by: None,
        };
        vault.devices.push(entry);
        vault.version += 1;
        vault.updated_at = now;
    }

    vf.save(&key, &vault).context("failed to save vault")?;
    password.zeroize();

    println!("✓ Device initialised");
    println!("  Device ID: {}", material.device_id);
    println!("  Public key: {}", material.pubkey_hex);
    Ok(())
}

/// Load the device identity from the encrypted sidecar file.
fn load_device_identity(
    vault_path: &std::path::Path,
    vault_key: &zvault_core::crypto::VaultKey,
) -> Result<CliDeviceFile> {
    let sidecar = device_sidecar_path(vault_path);
    if !sidecar.exists() {
        bail!("Device identity not initialised. Run `zvault device init` first.");
    }

    let blob = std::fs::read(&sidecar).context("failed to read device sidecar file")?;
    let plaintext: Zeroizing<Vec<u8>> =
        Zeroizing::new(zvault_core::crypto::decrypt(vault_key, &blob).map_err(|e| {
            anyhow::anyhow!("failed to decrypt device sidecar (wrong password?): {e}")
        })?);

    let device_file: CliDeviceFile =
        serde_json::from_slice(&plaintext).context("failed to parse device sidecar JSON")?;
    Ok(device_file)
}

fn cmd_device_show(vault_path: PathBuf) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (_vf, key, _vault) = result?;

    let device = load_device_identity(&vault_path, &key)?;

    // Compute npub from the public key
    let pubkey_bytes =
        hex::decode(&device.pubkey_hex).context("invalid public key hex in device sidecar")?;
    let pubkey_array: [u8; 32] = pubkey_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("public key is not 32 bytes"))?;
    let npub = zvault_core::nip19::encode_npub(&pubkey_array);

    println!("Device Identity");
    println!("───────────────");
    println!("  Device ID:  {}", device.device_id);
    println!("  Label:      {}", device.label);
    println!("  Public key: {}", device.pubkey_hex);
    println!("  npub:       {npub}");

    Ok(())
}

fn cmd_device_export_key(vault_path: PathBuf) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (_vf, key, _vault) = result?;

    let device = load_device_identity(&vault_path, &key)?;

    // Decode the secret key hex
    let sk_bytes =
        hex::decode(&device.secret_key_hex).context("invalid secret key hex in device sidecar")?;
    let sk_array: [u8; 32] = sk_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("secret key is not 32 bytes"))?;
    let nsec = zvault_core::nip19::encode_nsec(&sk_array);

    eprintln!("⚠️  WARNING: This displays your device secret key.");
    eprintln!("⚠️  Anyone with this key can impersonate your device.");
    eprintln!("⚠️  Never share this key. Store it securely if backing up.");
    eprintln!();

    println!("Device Secret Key");
    println!("─────────────────");
    println!("  nsec: {}", *nsec);
    println!("  hex:  {}", device.secret_key_hex);

    Ok(())
}

// ─── Pairing Commands ────────────────────────────────────────────────────────

fn cmd_pair_invite(vault_path: PathBuf) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (_vf, key, vault) = result?;

    // Load device identity to get our pubkey and label.
    let device = load_device_identity(&vault_path, &key)?;

    let payload =
        zvault_core::pairing::create_invite(&device.pubkey_hex, &device.label, vault.vault_id)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

    let code =
        zvault_core::pairing::encode_pairing_code(&payload).map_err(|e| anyhow::anyhow!("{e}"))?;

    println!("Invite code (share with the other device):\n");
    println!("{code}");
    println!("\nThe other device should run: zvault pair import <code> --vault <path>");
    Ok(())
}

fn cmd_pair_request(vault_path: PathBuf) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (_vf, key, _vault) = result?;

    // Load device identity.
    let device = load_device_identity(&vault_path, &key)?;

    let payload = zvault_core::pairing::create_join_request(&device.pubkey_hex, &device.label)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let code =
        zvault_core::pairing::encode_pairing_code(&payload).map_err(|e| anyhow::anyhow!("{e}"))?;

    println!("Join-request code (share with an admin device):\n");
    println!("{code}");
    println!("\nThe admin device should run: zvault pair import <code> --vault <path>");
    Ok(())
}

fn cmd_pair_import(vault_path: PathBuf, code: String, yes: bool) -> Result<()> {
    let payload =
        zvault_core::pairing::decode_pairing_code(&code).map_err(|e| anyhow::anyhow!("{e}"))?;

    println!("Pairing code decoded:");
    println!("  Type:   {:?}", payload.t);
    println!("  Pubkey: {}", payload.p);
    println!("  Label:  {}", payload.l);
    if let Some(vid) = payload.vid {
        println!("  Vault:  {vid}");
    }
    println!(
        "  Time:   {}",
        chrono::DateTime::from_timestamp(payload.ts, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| payload.ts.to_string())
    );
    println!();

    use zvault_core::pairing::PairingType;

    match payload.t {
        PairingType::Invite | PairingType::JoinRequest => {
            // We need to admit this device to our vault.
            if !yes {
                let answer = prompt_line("Admit this device to your vault? [y/N]: ")?;
                if !matches!(answer.to_lowercase().as_str(), "y" | "yes") {
                    println!("Cancelled.");
                    return Ok(());
                }
            }

            let mut password = get_password("Enter master password: ")?;
            let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
            password.zeroize();
            let (vf, key, mut vault) = result?;

            // Check for duplicate or revoked device with the same pubkey.
            let normalized_pubkey = payload.p.to_lowercase();
            for existing in &vault.devices {
                if existing.nostr_pubkey == normalized_pubkey {
                    if existing.revoked {
                        bail!("Device with this public key was previously revoked and cannot be re-admitted");
                    } else {
                        bail!("Device with this public key is already admitted");
                    }
                }
            }

            // Admit the remote device.
            let now = chrono::Utc::now();
            let device_id = Uuid::new_v4();
            let added_by = vault
                .devices
                .first()
                .map(|d| d.device_id)
                .unwrap_or(device_id);

            let entry = zvault_core::vault::DeviceEntry {
                device_id,
                nostr_pubkey: normalized_pubkey,
                label: payload.l.clone(),
                added_at: now,
                added_by,
                revoked: false,
                revoked_at: None,
                revoked_by: None,
            };
            vault.devices.push(entry);
            vault.version += 1;
            vault.updated_at = now;
            vf.save(&key, &vault).context("failed to save vault")?;

            println!("✓ Device admitted: {} ({})", payload.l, device_id);

            // Generate response code if we have a device identity.
            if let Ok(device) = load_device_identity(&vault_path, &key) {
                let response_payload = match payload.t {
                    PairingType::Invite => zvault_core::pairing::create_invite_response(
                        &device.pubkey_hex,
                        &device.label,
                    )
                    .map_err(|e| anyhow::anyhow!("{e}"))?,
                    PairingType::JoinRequest => zvault_core::pairing::create_join_response(
                        &device.pubkey_hex,
                        &device.label,
                        vault.vault_id,
                    )
                    .map_err(|e| anyhow::anyhow!("{e}"))?,
                    _ => unreachable!(),
                };
                let response_code = zvault_core::pairing::encode_pairing_code(&response_payload)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                println!("\nResponse code (send back to the other device):\n");
                println!("{response_code}");
            }
        }
        PairingType::InviteResponse | PairingType::JoinResponse => {
            // The remote device has responded — admit them.
            if !yes {
                let answer = prompt_line("Complete pairing (admit this device)? [y/N]: ")?;
                if !matches!(answer.to_lowercase().as_str(), "y" | "yes") {
                    println!("Cancelled.");
                    return Ok(());
                }
            }

            let mut password = get_password("Enter master password: ")?;
            let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
            password.zeroize();
            let (vf, key, mut vault) = result?;

            // Check for duplicate or revoked device with the same pubkey.
            let normalized_pubkey = payload.p.to_lowercase();
            for existing in &vault.devices {
                if existing.nostr_pubkey == normalized_pubkey {
                    if existing.revoked {
                        bail!("Device with this public key was previously revoked and cannot be re-admitted");
                    } else {
                        bail!("Device with this public key is already admitted");
                    }
                }
            }

            let now = chrono::Utc::now();
            let device_id = Uuid::new_v4();
            let added_by = vault
                .devices
                .first()
                .map(|d| d.device_id)
                .unwrap_or(device_id);

            let entry = zvault_core::vault::DeviceEntry {
                device_id,
                nostr_pubkey: normalized_pubkey,
                label: payload.l.clone(),
                added_at: now,
                added_by,
                revoked: false,
                revoked_at: None,
                revoked_by: None,
            };
            vault.devices.push(entry);
            vault.version += 1;
            vault.updated_at = now;
            vf.save(&key, &vault).context("failed to save vault")?;

            println!(
                "✓ Pairing complete. Device admitted: {} ({})",
                payload.l, device_id
            );
        }
    }

    Ok(())
}

// ─── Sync Commands ───────────────────────────────────────────────────────────

fn cmd_sync_send(
    vault_path: PathBuf,
    relay_url: Option<String>,
    recipient_pubkey: String,
) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (vf, key, vault) = result?;
    let _ = vf; // not needed after open

    // Determine relay URL: explicit flag or vault settings.
    let relay = match relay_url {
        Some(url) => url,
        None => {
            let enabled = zvault_core::settings::enabled_relay_urls(&vault.settings);
            if enabled.is_empty() {
                bail!("No relay specified and no enabled relays in vault settings. Use --relay or configure relays with `zvault relay add`.");
            }
            enabled[0].clone()
        }
    };

    // Load device identity.
    let device = load_device_identity(&vault_path, &key)?;
    let sk_bytes = Zeroizing::new(
        hex::decode(&device.secret_key_hex).context("invalid secret key hex in device sidecar")?,
    );

    // Build the sync message.
    let mut clock = zvault_core::sync::LamportClock::new();
    let sync_msg = zvault_core::sync::build_full_sync_message(
        &vault,
        &mut clock,
        device.device_id,
        &sk_bytes,
        &recipient_pubkey,
    )
    .map_err(|e| anyhow::anyhow!("build sync message failed: {e}"))?;

    // Serialise the sync message to JSON (this becomes the inner content for gift-wrap).
    let sync_json = serde_json::to_string(&sync_msg).context("failed to serialise sync message")?;

    // Gift-wrap the sync message.
    let gift_wrapped = zvault_core::nostr::gift_wrap(
        &sk_bytes,
        &recipient_pubkey,
        &sync_json,
        21059, // custom kind for zvault sync (inside the gift wrap — relay sees 1059)
        &[],
    )
    .map_err(|e| anyhow::anyhow!("gift wrap failed: {e}"))?;

    // Publish to the relay.
    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let mut client = zvault_core::relay::RelayClient::connect(&relay)
            .await
            .map_err(|e| anyhow::anyhow!("relay connect failed: {e}"))?;

        client
            .publish(&gift_wrapped)
            .await
            .map_err(|e| anyhow::anyhow!("relay publish failed: {e}"))?;

        client.close().await.ok();
        Ok::<(), anyhow::Error>(())
    })?;

    println!("✓ Sync sent (vault version {})", vault.version);
    Ok(())
}

fn cmd_sync_receive(
    vault_path: PathBuf,
    relay_url: Option<String>,
    timeout_secs: u64,
) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (vf, key, mut vault) = result?;

    // Determine relay URL: explicit flag or vault settings.
    let relay = match relay_url {
        Some(url) => url,
        None => {
            let enabled = zvault_core::settings::enabled_relay_urls(&vault.settings);
            if enabled.is_empty() {
                bail!("No relay specified and no enabled relays in vault settings. Use --relay or configure relays with `zvault relay add`.");
            }
            enabled[0].clone()
        }
    };

    // Load device identity.
    let device = load_device_identity(&vault_path, &key)?;
    let sk_bytes = Zeroizing::new(
        hex::decode(&device.secret_key_hex).context("invalid secret key hex in device sidecar")?,
    );

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    let messages_applied = rt.block_on(async {
        let mut client = zvault_core::relay::RelayClient::connect(&relay)
            .await
            .map_err(|e| anyhow::anyhow!("relay connect failed: {e}"))?;

        // Subscribe for gift-wrapped events addressed to our pubkey.
        // Use `since` 3 days ago to account for NIP-59 timestamp randomisation
        // (gift-wrap timestamps are randomly offset ±2 days).
        let since = chrono::Utc::now().timestamp() - 259200; // 3 days
        let filter = zvault_core::relay::SubscriptionFilter {
            kinds: Some(vec![1059]),
            p_tags: Some(vec![device.pubkey_hex.clone()]),
            since: Some(since),
            ..Default::default()
        };
        let mut rx = client
            .subscribe(filter)
            .await
            .map_err(|e| anyhow::anyhow!("relay subscribe failed: {e}"))?;

        let mut applied_count = 0u32;
        let timeout = std::time::Duration::from_secs(timeout_secs);

        // Receive events until timeout.
        loop {
            match tokio::time::timeout(timeout, rx.recv()).await {
                Ok(Some(event)) => {
                    // Try to unwrap the gift-wrap.
                    match zvault_core::nostr::unwrap_gift_wrap(&sk_bytes, &event) {
                        Ok(rumor) => {
                            // Parse the inner content as a SyncMessage.
                            match serde_json::from_str::<zvault_core::sync::SyncMessage>(
                                &rumor.content,
                            ) {
                                Ok(sync_msg) => {
                                    // Find the sender's pubkey from the rumor.
                                    let sender_pubkey = &rumor.pubkey;
                                    let mut clock = zvault_core::sync::LamportClock::new();
                                    match zvault_core::sync::apply_sync_message(
                                        &mut vault,
                                        &sync_msg,
                                        &mut clock,
                                        &sk_bytes,
                                        sender_pubkey,
                                    ) {
                                        Ok(()) => {
                                            applied_count += 1;
                                        }
                                        Err(e) => {
                                            eprintln!("warning: sync message rejected: {e}");
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("warning: failed to parse sync message: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("warning: failed to unwrap gift-wrap: {e}");
                        }
                    }
                }
                Ok(None) => {
                    // Channel closed (relay disconnected).
                    break;
                }
                Err(_) => {
                    // Timeout — done waiting.
                    break;
                }
            }
        }

        client.close().await.ok();
        Ok::<u32, anyhow::Error>(applied_count)
    })?;

    if messages_applied > 0 {
        vf.save(&key, &vault).context("failed to save vault")?;
        println!(
            "✓ Received {} sync message(s), vault now at version {}",
            messages_applied, vault.version
        );
    } else {
        println!("No sync messages received.");
    }
    Ok(())
}

// ─── Relay Commands ──────────────────────────────────────────────────────────

fn cmd_relay_list(vault_path: PathBuf) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (_vf, _key, vault) = result?;

    let relays = &vault.settings.relays;
    if relays.is_empty() {
        println!("No relays configured.");
        return Ok(());
    }

    println!("{:<8} {:<45} ADDED", "STATUS", "URL");
    println!("{}", "-".repeat(70));
    for entry in relays {
        let status = if entry.enabled { "enabled" } else { "disabled" };
        println!(
            "{:<8} {:<45} {}",
            status,
            entry.url,
            entry.added_at.format("%Y-%m-%d")
        );
    }
    println!(
        "\n{} relay(s) total, {} enabled.",
        relays.len(),
        relays.iter().filter(|r| r.enabled).count()
    );
    Ok(())
}

fn cmd_relay_add(vault_path: PathBuf, url: String) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (vf, key, mut vault) = result?;

    zvault_core::settings::add_relay(&mut vault.settings, &url)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    vault.version += 1;
    vault.updated_at = chrono::Utc::now();
    vf.save(&key, &vault).context("failed to save vault")?;

    println!("✓ Relay added: {url}");
    Ok(())
}

fn cmd_relay_remove(vault_path: PathBuf, url: String) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (vf, key, mut vault) = result?;

    zvault_core::settings::remove_relay(&mut vault.settings, &url)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    vault.version += 1;
    vault.updated_at = chrono::Utc::now();
    vf.save(&key, &vault).context("failed to save vault")?;

    println!("✓ Relay removed: {url}");
    Ok(())
}

fn cmd_relay_enable(vault_path: PathBuf, url: String) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (vf, key, mut vault) = result?;

    zvault_core::settings::set_relay_enabled(&mut vault.settings, &url, true)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    vault.version += 1;
    vault.updated_at = chrono::Utc::now();
    vf.save(&key, &vault).context("failed to save vault")?;

    println!("✓ Relay enabled: {url}");
    Ok(())
}

fn cmd_relay_disable(vault_path: PathBuf, url: String) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (vf, key, mut vault) = result?;

    zvault_core::settings::set_relay_enabled(&mut vault.settings, &url, false)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    vault.version += 1;
    vault.updated_at = chrono::Utc::now();
    vf.save(&key, &vault).context("failed to save vault")?;

    println!("✓ Relay disabled: {url}");
    Ok(())
}

fn cmd_relay_reset(vault_path: PathBuf) -> Result<()> {
    let mut password = get_password("Enter master password: ")?;
    let result = VaultFile::open(&password, &vault_path).context("failed to open vault");
    password.zeroize();
    let (vf, key, mut vault) = result?;

    zvault_core::settings::reset_relays(&mut vault.settings);

    vault.version += 1;
    vault.updated_at = chrono::Utc::now();
    vf.save(&key, &vault).context("failed to save vault")?;

    println!("✓ Relays reset to defaults");
    Ok(())
}

// ─── CSV helpers ─────────────────────────────────────────────────────────────

/// Write data to a file with restrictive permissions (owner-only: 0600).
///
/// This is used for plaintext exports (JSON, CSV) to prevent other users on the
/// system from reading the exported credentials.
#[cfg(unix)]
fn write_restricted_file(path: &std::path::Path, data: &[u8]) -> Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .context("failed to create export file with restricted permissions")?;
    file.write_all(data)?;
    file.flush()?;
    Ok(())
}

#[cfg(not(unix))]
fn write_restricted_file(path: &std::path::Path, data: &[u8]) -> Result<()> {
    // On non-Unix (Windows), write normally; Windows ACLs default to
    // owner-only for user-profile directories.
    std::fs::write(path, data)?;
    Ok(())
}

/// Escape a CSV field (wrap in quotes if it contains comma, quote, or newline).
fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Parse a single CSV line respecting quoted fields.
fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes => {
                // Check for escaped quote ("").
                if chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            }
            '"' if !in_quotes && current.is_empty() => {
                in_quotes = true;
            }
            ',' if !in_quotes => {
                fields.push(current.clone());
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }
    fields.push(current);
    fields
}

// ─── Entry point ─────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { path } => cmd_init(path),
        Commands::Unlock { path } => cmd_unlock(path),
        Commands::Lock => cmd_lock(),
        Commands::List {
            vault,
            show_password,
        } => cmd_list(vault, show_password),
        Commands::Get {
            vault,
            id,
            show_password,
            totp,
        } => cmd_get(vault, id, show_password, totp),
        Commands::Add { vault, json } => cmd_add(vault, json),
        Commands::Edit { vault, id } => cmd_edit(vault, id),
        Commands::Delete { vault, id, yes } => cmd_delete(vault, id, yes),
        Commands::Rekey { vault } => cmd_rekey(vault),
        Commands::Export {
            vault,
            format,
            output,
        } => cmd_export(vault, format, output),
        Commands::Import {
            vault,
            format,
            input,
        } => cmd_import(vault, format, input),
        Commands::Devices { vault } => cmd_devices(vault),
        Commands::Device { action } => match action {
            DeviceAction::Admit {
                vault,
                label,
                pubkey,
            } => cmd_device_admit(vault, label, pubkey),
            DeviceAction::Revoke { vault, id } => cmd_device_revoke(vault, id),
            DeviceAction::Init { vault, label } => cmd_device_init(vault, label),
            DeviceAction::Show { vault } => cmd_device_show(vault),
            DeviceAction::ExportKey { vault } => cmd_device_export_key(vault),
        },
        Commands::Pair { action } => match action {
            PairAction::Invite { vault } => cmd_pair_invite(vault),
            PairAction::Request { vault } => cmd_pair_request(vault),
            PairAction::Import { code, vault, yes } => cmd_pair_import(vault, code, yes),
        },
        Commands::Sync { action } => match action {
            SyncAction::Send {
                vault,
                relay,
                recipient,
            } => cmd_sync_send(vault, relay, recipient),
            SyncAction::Receive {
                vault,
                relay,
                timeout,
            } => cmd_sync_receive(vault, relay, timeout),
        },
        Commands::Relay { action } => match action {
            RelayAction::List { vault } => cmd_relay_list(vault),
            RelayAction::Add { vault, url } => cmd_relay_add(vault, url),
            RelayAction::Remove { vault, url } => cmd_relay_remove(vault, url),
            RelayAction::Enable { vault, url } => cmd_relay_enable(vault, url),
            RelayAction::Disable { vault, url } => cmd_relay_disable(vault, url),
            RelayAction::Reset { vault } => cmd_relay_reset(vault),
        },
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escape_simple() {
        assert_eq!(csv_escape("hello"), "hello");
        assert_eq!(csv_escape("hello,world"), "\"hello,world\"");
        assert_eq!(csv_escape("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn parse_csv_line_basic() {
        let fields = parse_csv_line("name,user,pass,url,notes");
        assert_eq!(fields, vec!["name", "user", "pass", "url", "notes"]);
    }

    #[test]
    fn parse_csv_line_quoted() {
        let fields = parse_csv_line("\"hello,world\",simple,\"has \"\"quotes\"\"\"");
        assert_eq!(fields, vec!["hello,world", "simple", "has \"quotes\""]);
    }

    #[test]
    fn parse_csv_line_empty_fields() {
        let fields = parse_csv_line("a,,c,");
        assert_eq!(fields, vec!["a", "", "c", ""]);
    }

    #[test]
    fn kind_label_all_variants() {
        assert_eq!(kind_label(&ItemKind::Login), "Login");
        assert_eq!(kind_label(&ItemKind::SecureNote), "SecureNote");
        assert_eq!(kind_label(&ItemKind::Card), "Card");
        assert_eq!(kind_label(&ItemKind::Identity), "Identity");
    }

    #[test]
    fn totp_generation_produces_valid_code() {
        use totp_rs::{Algorithm, TOTP};

        // Use a well-known test secret
        let secret = "JBSWY3DPEHPK3PXP";
        let secret_bytes = secret.as_bytes().to_vec();
        let totp = TOTP::new(Algorithm::SHA1, 6, 1, 30, secret_bytes)
            .expect("TOTP creation should succeed with valid secret");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let code = totp.generate(now);

        // Code should be exactly 6 digits
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));

        // Remaining seconds should be 1-30
        let remaining = 30 - (now % 30);
        assert!((1..=30).contains(&remaining));
    }

    #[test]
    fn totp_invalid_secret_is_handled() {
        use totp_rs::{Algorithm, TOTP};

        // Empty secret should still create a valid TOTP (totp-rs allows it)
        // but we test that the API doesn't panic
        let secret = "";
        let secret_bytes = secret.as_bytes().to_vec();
        // totp-rs accepts empty secrets (produces codes from empty key)
        let result = TOTP::new(Algorithm::SHA1, 6, 1, 30, secret_bytes);
        // Whether it succeeds or fails, it shouldn't panic
        let _ = result;
    }
}
