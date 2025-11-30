use anyhow::{Context, Result};
use reqwest::blocking::Client;
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{self, File};
use std::io::{copy, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::NamedTempFile;

pub fn map_os() -> &'static str {
    match env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
}

pub fn map_arch() -> &'static str {
    match env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

pub fn terraform_binary_name() -> &'static str {
    if cfg!(windows) {
        "terraform.exe"
    } else {
        "terraform"
    }
}

pub fn asset_name(product: &str, version: &str) -> String {
    let os = map_os();
    let arch = map_arch();
    format!("{}_{}_{}_{}.zip", product, version, os, arch)
}

fn asset_url(product: &str, remote: &str, version: &str, asset: &str) -> String {
    let base = if remote.ends_with('/') {
        remote.to_string()
    } else {
        format!("{}/", remote)
    };
    if product == "terraform" {
        // HashiCorp releases: <base><version>/<asset>
        format!("{}{}{}", base, version, "/") + asset
    } else {
        // Assume GitHub-style releases download base: <base>v<version>/<asset>
        format!("{}v{}/{}", base, version, asset)
    }
}

fn fetch_to_temp(url: &str) -> Result<NamedTempFile> {
    let client = Client::builder().build()?;
    let mut resp = client.get(url).send().context("failed to fetch asset")?;
    if !resp.status().is_success() {
        anyhow::bail!("Failed to download {}: HTTP {}", url, resp.status());
    }
    let mut tmp = NamedTempFile::new().context("failed to create tempfile")?;
    copy(&mut resp, &mut tmp).context("failed to copy response to tempfile")?;
    Ok(tmp)
}

fn fetch_sha256sums(_remote: &str, version: &str) -> Result<String> {
    let client = Client::builder().build()?;
    // The above is brittle; try canonical HashiCorp path
    let candidate = format!(
        "https://releases.hashicorp.com/terraform/{}/terraform_{}_SHA256SUMS",
        version, version
    );
    let mut resp = client
        .get(&candidate)
        .send()
        .context("failed to fetch sha256sums")?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "Failed to fetch SHA256SUMS: {} status: {}",
            candidate,
            resp.status()
        );
    }
    let mut body = String::new();
    resp.read_to_string(&mut body)?;
    Ok(body)
}

fn fetch_sig(_remote: &str, version: &str) -> Result<NamedTempFile> {
    let client = Client::builder().build()?;
    let candidate = format!(
        "https://releases.hashicorp.com/terraform/{}/terraform_{}_SHA256SUMS.sig",
        version, version
    );
    let mut resp = client
        .get(&candidate)
        .send()
        .context("failed to fetch sha256sig")?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "Failed to fetch SHA256SUMS.sig: {} status: {}",
            candidate,
            resp.status()
        );
    }
    let mut tmp = NamedTempFile::new().context("failed to create tempfile for sig")?;
    copy(&mut resp, &mut tmp).context("failed to copy sig to tempfile")?;
    Ok(tmp)
}

fn verify_sig_with_gpg(tfenv_root: &Path, sig_path: &Path, sums_path: &Path) -> Result<()> {
    // Create temporary GNUPGHOME
    let gpg_home = tempfile::TempDir::new().context("failed to create tempdir for gpg")?;
    let gpg = which::which("gpg").context("gpg not found in PATH")?;
    // import bundled keys if present
    let bundled = tfenv_root.join("share").join("hashicorp-keys.pgp");
    if bundled.exists() {
        let status = std::process::Command::new(&gpg)
            .arg("--homedir")
            .arg(gpg_home.path())
            .arg("--import")
            .arg(bundled)
            .status()
            .context("failed to import key into gpg")?;
        if !status.success() {
            anyhow::bail!("gpg import failed");
        }
    }
    // verify
    let status = std::process::Command::new(&gpg)
        .arg("--homedir")
        .arg(gpg_home.path())
        .arg("--verify")
        .arg(sig_path)
        .arg(sums_path)
        .status()
        .context("failed to invoke gpg --verify")?;
    if !status.success() {
        anyhow::bail!("gpg verification failed");
    }
    Ok(())
}

fn compute_sha256(path: &Path) -> Result<String> {
    let mut f = File::open(path).context("failed to open downloaded file for hashing")?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn extract_zip_to_version(zip_path: &Path, versions_dir: &Path, version: &str) -> Result<()> {
    let file = File::open(zip_path).context("failed to open zip file for extraction")?;
    let mut archive = zip::ZipArchive::new(file).context("failed to read zip archive")?;
    let out_dir = versions_dir.join(version);
    fs::create_dir_all(&out_dir)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).context("zip entry error")?;
        let name = entry.name().to_string();
        // We expect a single 'terraform' binary at top-level (or terraform.exe on Windows)
        let binary_name = terraform_binary_name();
        if name.ends_with(binary_name) || name.ends_with("terraform") {
            let out_path = out_dir.join(terraform_binary_name());
            let mut outfile =
                File::create(&out_path).context("failed to create terraform output file")?;
            copy(&mut entry, &mut outfile)?;
            #[cfg(unix)]
            {
                let mut perms = outfile.metadata()?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&out_path, perms)?;
            }
            return Ok(());
        }
    }
    anyhow::bail!("terraform binary not found inside archive");
}

pub fn install_version(
    tfenv_root: &Path,
    config_dir: &Path,
    requested: Option<&str>,
) -> Result<()> {
    let version = if let Some(v) = requested {
        v.to_string()
    } else {
        "latest".to_string()
    };
    // If requested is "latest", resolve remote latest - for MVP we'll treat "latest" as error
    if version == "latest" {
        anyhow::bail!("'latest' resolution not implemented in installer; pass an explicit version");
    }
    let product = env::var("TFENV_PRODUCT")
        .unwrap_or_else(|_| "terraform".to_string())
        .to_lowercase();
    let remote = env::var("TFENV_REMOTE").unwrap_or_else(|_| {
        if product == "terraform" {
            "https://releases.hashicorp.com/terraform/".to_string()
        } else if product == "opentofu" {
            "https://github.com/opentofu/opentofu/releases/download/".to_string()
        } else {
            // fallback: empty (user must set TFENV_REMOTE)
            "".to_string()
        }
    });

    let asset = asset_name(&product, &version);
    let url = asset_url(&product, &remote, &version, &asset);
    println!("Downloading {}", url);
    let tmp = fetch_to_temp(&url)?;
    println!("Downloaded to {}", tmp.path().display());
    // For HashiCorp terraform releases we will verify SHA256SUMS where possible.
    if product == "terraform" {
        let sums = fetch_sha256sums(&remote, &version)?;
        // find line matching asset
        let mut expected: Option<String> = None;
        for line in sums.lines() {
            if line.contains(&asset) {
                // format: <sha256>  <filename>
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    expected = Some(parts[0].to_string());
                    break;
                }
            }
        }
        if expected.is_none() {
            anyhow::bail!("No checksum found for asset {} in SHA256SUMS", asset);
        }
        let expected = expected.unwrap();

        let actual = compute_sha256(tmp.path())?;
        if actual != expected {
            anyhow::bail!("SHA256 mismatch: expected {} got {}", expected, actual);
        }
        println!("Checksum verified");

        // Optional PGP verification: if TFENV_TRUST_TFENV is set or use-gpgv file exists in TFENV_ROOT
        let trust = env::var("TFENV_TRUST_TFENV").unwrap_or_else(|_| "".to_string());
        let use_gpgv_file = tfenv_root.join("use-gpgv");
        if trust == "yes" || use_gpgv_file.exists() {
            println!("Verifying SHA256SUMS signature with gpg");
            // fetch sig and verify against sums
            let sig_tmp = fetch_sig(&remote, &version)?;
            // write sums to temp file
            let mut sums_tmp =
                NamedTempFile::new().context("failed to create tempfile for sums")?;
            sums_tmp.write_all(sums.as_bytes())?;
            verify_sig_with_gpg(tfenv_root, sig_tmp.path(), sums_tmp.path())?;
            println!("GPG verification succeeded");
        }
    } else {
        println!(
            "Skipping checksum/PGP verification for product '{}' by default.",
            product
        );
    }

    let versions_dir = config_dir.join("versions");
    fs::create_dir_all(&versions_dir)?;
    extract_zip_to_version(tmp.path(), &versions_dir, &version)?;
    println!(
        "Installed terraform {} to {}",
        version,
        versions_dir.join(&version).display()
    );
    Ok(())
}
