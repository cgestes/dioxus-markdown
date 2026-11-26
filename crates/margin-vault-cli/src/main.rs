//! `margin-vault`: create, pair, back up and serve a vault from the terminal.
use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};
use margin_note::note::Note;
use margin_vault::{
    self as vault, Vault,
    keys::{self, Identity, Pairing, Recovery},
    relay::Relay,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::{self, Read},
    net::SocketAddr,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(
    name = "margin-vault",
    about = "Personal Loro vault, paired-device sync and ciphertext relay"
)]
struct Args {
    /// Device directory (defaults to Margin's application data directory).
    #[arg(long, global = true)]
    dir: Option<PathBuf>,
    /// Optional headless unlock. Use a protected file with at least 12 bytes.
    #[arg(long, global = true)]
    passphrase_file: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a personal vault. Keep the recovery kit offline.
    Init {
        #[arg(long)]
        recovery: PathBuf,
    },
    /// Show the public vault and device identifiers.
    Info,
    /// List local and synced notes.
    List,
    /// Create a note, optionally reading Markdown from a file or "-" (stdin).
    New {
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        synced: bool,
    },
    /// Append Markdown from a file or "-" (stdin).
    Append { note: String, input: PathBuf },
    /// Print the current Markdown.
    Read { note: String },
    /// Create a synced copy of the current Markdown with fresh history.
    Publish { note: String },
    /// Add an immutable file to a note with the same privacy setting.
    Attach { note: String, file: PathBuf },
    /// List a note's attachments.
    Files { note: String },
    /// Explicitly export a decrypted attachment to a new file.
    Extract {
        note: String,
        attachment: String,
        output: PathBuf,
    },
    /// Configure the trusted relay endpoint on this device.
    Connect { url: String },
    /// Catch up once, or keep broadcasting and receiving.
    Sync {
        #[arg(long)]
        watch: bool,
    },
    /// Queue a verified full snapshot and sync it to the relay.
    Compact { note: String },
    /// Create a separate enrollment for one device. Scan the QR in person or transfer the file privately.
    Invite {
        output: PathBuf,
        #[arg(long)]
        qr: bool,
    },
    /// Enroll from a pairing kit or scanned text on stdin ("-").
    Join { input: PathBuf },
    /// Replace the credentials on an existing device using a fresh invitation for this vault.
    Refresh { input: PathBuf },
    /// Save a new offline owner recovery kit.
    Backup { output: PathBuf },
    /// Restore owner access on a new device.
    Recover { input: PathBuf },
    /// Create a fresh content key and recovery kit. Invite the remaining devices again.
    Rotate {
        #[arg(long)]
        recovery: PathBuf,
    },
    /// Run the relay on loopback. Terminate remote TLS at a reverse proxy.
    Serve {
        #[arg(long)]
        database: PathBuf,
        #[arg(long)]
        owner: String,
        #[arg(long, default_value = "127.0.0.1:8787")]
        listen: SocketAddr,
        #[arg(long, default_value_t = 64)]
        checkpoint_after: usize,
    },
    /// Revoke a device at the relay using local administrative access.
    Revoke {
        #[arg(long)]
        database: PathBuf,
        #[arg(long)]
        owner: String,
        device: String,
    },
}

#[derive(Serialize, Deserialize)]
struct Kit<T> {
    relay: Option<String>,
    secret: T,
}

fn input(path: &Path) -> Result<Vec<u8>> {
    if path == Path::new("-") {
        let mut v = Vec::new();
        io::stdin().read_to_end(&mut v)?;
        Ok(v)
    } else {
        Ok(fs::read(path)?)
    }
}
fn kit<T: DeserializeOwned>(path: &Path, kind: &str) -> Result<Kit<T>> {
    let bytes = input(path)?;
    let text = std::str::from_utf8(&bytes)?.trim();
    let body = text
        .strip_prefix(&format!("margin-{kind}-1."))
        .context("wrong vault kit type")?;
    let k: Kit<T> = serde_json::from_slice(&vault::unb64(body)?)?;
    if let Some(url) = &k.relay {
        vault::sync::validate_url(url)?
    }
    Ok(k)
}
fn store_kit<T: Serialize>(
    path: &Path,
    kind: &str,
    secret: T,
    relay: Option<String>,
) -> Result<String> {
    let text = format!(
        "margin-{kind}-1.{}",
        vault::b64(&serde_json::to_vec(&Kit { relay, secret })?)
    );
    keys::write_secret(path, text.as_bytes())?;
    Ok(text)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let a = Args::parse();
    let dir = match a.dir {
        Some(p) => p,
        None => vault::default_dir()?,
    };
    let pass = a.passphrase_file.as_deref();
    match a.command {
        Command::Serve {
            database,
            owner,
            listen,
            checkpoint_after,
        } => {
            return Relay::open(&database, &owner, checkpoint_after)?
                .listen(listen)
                .await;
        }
        Command::Revoke {
            database,
            owner,
            device,
        } => return Relay::open(&database, &owner, 64)?.revoke(&device),
        Command::Init { recovery } => {
            ensure!(!dir.join("device.json").exists(), "device already exists");
            let r = Recovery::new();
            store_kit(&recovery, "recovery", &r, None)?;
            let v = Vault::create(&dir, Identity::from_recovery(&r), pass)?;
            println!("{}", v.id.paired.vault);
            return Ok(());
        }
        Command::Join { input } => {
            let k = kit::<Pairing>(&input, "pair")?;
            let v = Vault::create(&dir, Identity::from_pair(k.secret)?, pass)?;
            if let Some(url) = k.relay {
                v.set_relay(&url)?
            }
            return Ok(());
        }
        Command::Recover { input } => {
            let k = kit::<Recovery>(&input, "recovery")?;
            ensure!(!k.secret.keys.is_empty(), "empty recovery kit");
            let v = Vault::create(&dir, Identity::from_recovery(&k.secret), pass)?;
            if let Some(url) = k.relay {
                v.set_relay(&url)?
            }
            return Ok(());
        }
        _ => {}
    }
    let v = Vault::open(&dir, pass)?;
    let url = || v.setting("relay")?.context("use connect to choose a relay");
    match a.command {
        Command::Info => println!(
            "Vault {}\nDevice {}\nPrivate inbox {}",
            v.id.paired.vault,
            v.id.paired.grant.device,
            v.inbox()
        ),
        Command::List => {
            for (room, shared) in v.rooms()? {
                let md = Note::from_doc(v.load(&room)?.0).markdown();
                println!(
                    "{}\t{}\t{}",
                    room,
                    if shared { "synced" } else { "device only" },
                    md.lines().next().unwrap_or("Untitled")
                );
            }
        }
        Command::New {
            input: path,
            synced,
        } => {
            let md = path.map(|p| input(&p)).transpose()?.unwrap_or_default();
            println!("{}", v.new_note(std::str::from_utf8(&md)?, synced)?);
        }
        Command::Append { note, input: path } => {
            let text = Note::from_doc(v.load(&note)?.0);
            let bytes = input(&path)?;
            if !bytes.is_empty() {
                let before = text.document().oplog_vv();
                let end = text.markdown().len();
                text.replace(end..end, std::str::from_utf8(&bytes)?)?;
                v.append_update(&note, &text.document().export(loro::ExportMode::updates(&before))?)?;
            }
        }
        Command::Read { note } => print!("{}", Note::from_doc(v.load(&note)?.0).markdown()),
        Command::Publish { note } => println!("{}", v.publish(&note)?),
        Command::Attach { note, file } => println!("{}", v.attach_path(&note, &file)?),
        Command::Files { note } => {
            for (id, name) in v.attachments(&note)? {
                println!("{id}\t{name}")
            }
        }
        Command::Extract {
            note,
            attachment,
            output,
        } => v.extract(&note, &attachment, &output)?,
        Command::Connect { url } => v.set_relay(&url)?,
        Command::Sync { watch: false } => vault::sync::once(&v, &url()?).await?,
        Command::Sync { watch: true } => {
            let (_stop, rx) = tokio::sync::watch::channel(false);
            vault::sync::follow(v.clone(), url()?, rx, |_| {}).await;
        }
        Command::Compact { note } => {
            let relay = if v.shared(&note)? { Some(url()?) } else { None };
            if let Some(url) = &relay {
                vault::sync::once(&v, url).await?;
            }
            ensure!(
                v.checkpoint(&note)?,
                "waiting for missing Loro dependencies"
            );
            if let Some(url) = &relay {
                vault::sync::once(&v, url).await?;
            }
        }
        Command::Invite { output, qr } => {
            let code = store_kit(
                &output,
                "pair",
                v.id.recovery()?.pair(),
                v.setting("relay")?,
            )?;
            if qr {
                println!(
                    "{}",
                    qrcode::QrCode::new(code)?
                        .render::<qrcode::render::unicode::Dense1x2>()
                        .build()
                )
            }
        }
        Command::Backup { output } => {
            store_kit(&output, "recovery", v.id.recovery()?, v.setting("relay")?)?;
        }
        Command::Refresh { input } => {
            let k = kit::<Pairing>(&input, "pair")?;
            let mut next = Identity::from_pair(k.secret)?;
            ensure!(
                next.paired.vault == v.id.paired.vault
                    && next.paired.grant.epoch >= v.current_epoch(),
                "wrong vault or old invitation"
            );
            ensure!(
                v.id.paired
                    .keys
                    .iter()
                    .all(|(e, key)| next.paired.keys.get(e) == Some(key)),
                "key history mismatch"
            );
            next.local = v.id.local;
            next.owner = v.id.owner;
            keys::save_identity(&dir, &next, pass, false)?;
        }
        Command::Rotate { recovery } => {
            let mut r = v.id.recovery()?;
            r.keys.insert(
                v.current_epoch()
                    .checked_add(1)
                    .context("key version exhausted")?,
                keys::random(),
            );
            store_kit(&recovery, "recovery", &r, v.setting("relay")?)?;
            let mut next = Identity::from_recovery(&r);
            next.local = v.id.local;
            keys::save_identity(&dir, &next, pass, false)?;
            if let Some(url) = v.setting("relay")? {
                vault::sync::once(&Vault::open(&dir, pass)?, &url).await?
            }
        }
        _ => bail!("invalid command context"),
    }
    Ok(())
}
