//! Device credentials. Only the vault owner can issue a new device certificate.
use super::{b64, unb64};
use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, Payload},
};
use anyhow::{Context, Result, anyhow, ensure};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Write, path::Path};
use zeroize::Zeroizing;

pub fn random<const N: usize>() -> [u8; N] {
    let mut a = [0; N];
    OsRng.fill_bytes(&mut a);
    a
}

pub fn derive(key: &[u8; 32], purpose: &[u8]) -> [u8; 32] {
    let mut out = [0; 32];
    hkdf::Hkdf::<Sha256>::new(Some(b"Margin personal vault v1"), key)
        .expand(purpose, &mut out)
        .expect("32-byte HKDF output");
    out
}

pub fn public(seed: &[u8; 32]) -> String {
    hex::encode(SigningKey::from_bytes(seed).verifying_key().as_bytes())
}

pub fn verify(key: &str, msg: &[u8], signature: &str) -> Result<()> {
    let raw: [u8; 32] = hex::decode(key)?
        .try_into()
        .map_err(|_| anyhow!("invalid public key"))?;
    VerifyingKey::from_bytes(&raw)?
        .verify_strict(msg, &Signature::from_slice(&unb64(signature)?)?)?;
    Ok(())
}

pub fn sign(seed: &[u8; 32], msg: &[u8]) -> String {
    b64(&SigningKey::from_bytes(seed).sign(msg).to_bytes())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Grant {
    pub device: String,
    pub epoch: u32,
    pub token_hash: String,
    pub signature: String,
}

impl Grant {
    fn message(&self, vault: &str) -> Vec<u8> {
        format!(
            "Margin device v1/{vault}/{}/{}/{}",
            self.device, self.token_hash, self.epoch
        )
        .into_bytes()
    }

    pub fn verify(&self, vault: &str) -> Result<()> {
        ensure!(
            self.epoch > 0
                && super::is_hex(&self.device, 64)
                && super::is_hex(&self.token_hash, 64),
            "invalid device certificate"
        );
        verify(vault, &self.message(vault), &self.signature)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Recovery {
    pub owner: [u8; 32],
    pub keys: BTreeMap<u32, [u8; 32]>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Pairing {
    pub vault: String,
    pub keys: BTreeMap<u32, [u8; 32]>,
    pub device_seed: [u8; 32],
    pub token: [u8; 32],
    pub grant: Grant,
}

impl Recovery {
    pub fn new() -> Self {
        Self {
            owner: random(),
            keys: BTreeMap::from([(1, random())]),
        }
    }

    pub fn pair(&self) -> Pairing {
        let vault = public(&self.owner);
        let device_seed = random();
        let token: [u8; 32] = random();
        let mut grant = Grant {
            device: public(&device_seed),
            epoch: *self.keys.last_key_value().expect("vault epoch").0,
            token_hash: hex::encode(Sha256::digest(token)),
            signature: String::new(),
        };
        grant.signature = sign(&self.owner, &grant.message(&vault));
        Pairing {
            vault,
            keys: self.keys.clone(),
            device_seed,
            token,
            grant,
        }
    }
}

impl Default for Recovery {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Identity {
    pub paired: Pairing,
    pub owner: Option<[u8; 32]>,
    pub local: [u8; 32],
}

impl Identity {
    pub fn from_recovery(r: &Recovery) -> Self {
        Self {
            paired: r.pair(),
            owner: Some(r.owner),
            local: random(),
        }
    }

    pub fn from_pair(paired: Pairing) -> Result<Self> {
        paired.grant.verify(&paired.vault)?;
        ensure!(
            paired.keys.keys().all(|k| *k > 0)
                && paired.keys.last_key_value().map(|(e, _)| *e) == Some(paired.grant.epoch),
            "missing content keys"
        );
        ensure!(
            paired.grant.device == public(&paired.device_seed)
                && paired.grant.token_hash == hex::encode(Sha256::digest(paired.token)),
            "pairing credential mismatch"
        );
        Ok(Self {
            paired,
            owner: None,
            local: random(),
        })
    }

    pub fn recovery(&self) -> Result<Recovery> {
        Ok(Recovery {
            owner: self
                .owner
                .context("requires the owner device or offline recovery kit")?,
            keys: self.paired.keys.clone(),
        })
    }
}

pub fn seal(key: &[u8; 32], iv: &[u8; 12], aad: &[u8], bytes: &[u8]) -> Result<Vec<u8>> {
    Aes256Gcm::new(key.into())
        .encrypt(iv.into(), Payload { msg: bytes, aad })
        .map_err(|_| anyhow!("encryption failed"))
}

pub fn open(key: &[u8; 32], iv: &[u8; 12], aad: &[u8], bytes: &[u8]) -> Result<Vec<u8>> {
    Aes256Gcm::new(key.into())
        .decrypt(iv.into(), Payload { msg: bytes, aad })
        .map_err(|_| anyhow!("authentication failed"))
}

#[derive(Serialize, Deserialize)]
struct Locked {
    slot: String,
    salt: Option<[u8; 16]>,
    iv: [u8; 12],
    ciphertext: String,
}

impl Locked {
    fn wrapping_key(&self, password: Option<&Path>, create: bool) -> Result<Zeroizing<[u8; 32]>> {
        let mut key = Zeroizing::new([0; 32]);
        if let Some(salt) = self.salt {
            let path = password.context("this device requires --passphrase-file")?;
            let value = Zeroizing::new(fs::read(path)?);
            ensure!(
                value.len() >= 12 && value.len() <= 4096,
                "use a passphrase file of 12 to 4096 bytes"
            );
            let params = argon2::Params::new(65536, 3, 1, Some(32)).map_err(|e| anyhow!(e))?;
            argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
                .hash_password_into(&value, &salt, key.as_mut())
                .map_err(|e| anyhow!(e))?;
        } else {
            ensure!(password.is_none(), "this device uses the OS keychain");
            let entry = keyring::Entry::new("Margin personal vault", &self.slot)?;
            if create {
                OsRng.fill_bytes(key.as_mut());
                entry
                    .set_secret(key.as_ref())
                    .context("could not store the vault wrapping key in the OS keychain")?;
            } else {
                let bytes = Zeroizing::new(
                    entry
                        .get_secret()
                        .context("could not unlock the vault in the OS keychain")?,
                );
                ensure!(bytes.len() == 32, "invalid keychain entry");
                key.copy_from_slice(&bytes);
            }
        }
        Ok(key)
    }
}

pub fn save_identity(dir: &Path, id: &Identity, pass: Option<&Path>, create: bool) -> Result<()> {
    let path = dir.join("device.json");
    let mut locked = if create {
        ensure!(!path.exists(), "device already exists");
        Locked {
            slot: hex::encode(random::<16>()),
            salt: pass.map(|_| random()),
            iv: random(),
            ciphertext: String::new(),
        }
    } else {
        serde_json::from_slice(&fs::read(&path)?)?
    };
    let key = locked.wrapping_key(pass, create)?;
    locked.iv = random();
    locked.ciphertext = b64(&seal(
        &key,
        &locked.iv,
        locked.slot.as_bytes(),
        &Zeroizing::new(serde_json::to_vec(id)?),
    )?);
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    serde_json::to_writer(&mut temp, &locked)?;
    temp.as_file().sync_all()?;
    if create {
        temp.persist_noclobber(path)?;
    } else {
        temp.persist(path)?;
    }
    Ok(())
}

pub fn load_identity(dir: &Path, pass: Option<&Path>) -> Result<Identity> {
    let locked: Locked =
        serde_json::from_slice(&fs::read(dir.join("device.json")).context("open vault device")?)?;
    let key = locked.wrapping_key(pass, false)?;
    let bytes = Zeroizing::new(open(
        &key,
        &locked.iv,
        locked.slot.as_bytes(),
        &unb64(&locked.ciphertext)?,
    )?);
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn write_secret(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(data)?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(path)?;
    Ok(())
}
