use anyhow::{Context, Result};
use regex::Regex;
use scraper::{Html, Selector};
use semver::Version;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn find_local_version_file(start: &Path) -> Option<PathBuf> {
    let mut root = start.to_path_buf();
    loop {
        let candidate = root.join(".terraform-version");
        if candidate.exists() {
            return Some(candidate);
        }
        if !root.pop() {
            break;
        }
    }
    None
}

pub fn resolve_version_name(tfenv_root: &Path, config_dir: &Path) -> Result<String> {
    // 1. TFENV_TERRAFORM_VERSION
    if let Ok(var) = env::var("TFENV_TERRAFORM_VERSION") {
        if !var.is_empty() {
            return resolve_requested(&var, tfenv_root, config_dir);
        }
    }
    // 2. find version file
    let cwd = env::current_dir()?;
    if let Some(f) = find_local_version_file(&cwd) {
        let s = fs::read_to_string(&f)?.trim().to_string();
        if !s.is_empty() {
            return resolve_requested(&s, tfenv_root, config_dir);
        }
    }
    // 3. $HOME/.terraform-version
    if let Some(home) = dirs::home_dir() {
        let hf = home.join(".terraform-version");
        if hf.exists() {
            let s = fs::read_to_string(hf)?.trim().to_string();
            if !s.is_empty() {
                return resolve_requested(&s, tfenv_root, config_dir);
            }
        }
    }
    // default to latest
    resolve_requested("latest", tfenv_root, config_dir)
}

pub fn list_remote_versions() -> Result<Vec<(String, String)>> {
    let product = env::var("TFENV_PRODUCT")
        .unwrap_or_else(|_| "terraform".to_string())
        .to_lowercase();
    let remote = env::var("TFENV_REMOTE").unwrap_or_else(|_| {
        if product == "terraform" {
            "https://releases.hashicorp.com/terraform/".to_string()
        } else if product == "opentofu" {
            "https://github.com/opentofu/opentofu/releases".to_string()
        } else {
            // fallback to HashiCorp-style
            "https://releases.hashicorp.com/terraform/".to_string()
        }
    });
    let body = reqwest::blocking::get(&remote)?.text()?;
    let doc = Html::parse_document(&body);
    let selector = Selector::parse("a").unwrap();
    let mut versions: Vec<Version> = Vec::new();
    for el in doc.select(&selector) {
        if let Some(href) = el.value().attr("href") {
            if product == "terraform" {
                if let Some(caps) = href.strip_prefix("/terraform/") {
                    let v = caps.trim_end_matches('/');
                    if let Ok(vers) = Version::parse(v) {
                        versions.push(vers);
                    }
                }
            } else if product == "opentofu" {
                // look for GitHub release tag links like /opentofu/opentofu/releases/tag/vX.Y.Z
                if let Some(pos) = href.find("/opentofu/opentofu/releases/tag/v") {
                    let v = &href[pos + "/opentofu/opentofu/releases/tag/v".len()..];
                    let v = v.trim_end_matches('/');
                    if let Ok(vers) = Version::parse(v) {
                        versions.push(vers);
                    }
                }
            }
        }
    }
    versions.sort();
    versions.reverse();
    Ok(versions
        .into_iter()
        .map(|v| (v.to_string(), product.clone()))
        .collect())
}

fn resolve_requested(
    requested: &str,
    _tfenv_root: &Path,
    config_dir: &Path,
) -> Result<String> {
    let mut req = requested.to_string();
    if req.starts_with('v') {
        req = req.trim_start_matches('v').to_string();
    }

    if req == "min-required" {
        if let Some(min) = min_required(config_dir)? {
            return Ok(min);
        }
        anyhow::bail!("min-required could not be determined");
    }

    if req == "latest-allowed" {
        if let Some(mapped) = latest_allowed_to_requested(config_dir)? {
            req = mapped;
        }
    }

    if req.starts_with("latest") {
        // parse regex if any
        let mut regex = r"^[0-9]+\.[0-9]+\.[0-9]+$".to_string();
        if req.contains(':') {
            if let Some(i) = req.find(':') {
                regex = req[i + 1..].to_string();
            }
        }
        // First prefer locally installed matching version
        if let Some(local) = latest_local_matching(config_dir, &regex)? {
            return Ok(local);
        }
        // If TFENV_AUTO_INSTALL true, look remote
        let auto = env::var("TFENV_AUTO_INSTALL").unwrap_or_else(|_| "true".to_string());
        if auto == "true" {
            if let Some(remote) = latest_remote_matching(&regex)? {
                return Ok(remote);
            }
            anyhow::bail!("No versions matching '{}' found in remote", regex);
        }
        anyhow::bail!(
            "No installed versions matched '{}' and auto-install disabled",
            regex
        );
    }

    Ok(req)
}

fn latest_local_matching(config_dir: &Path, regex: &str) -> Result<Option<String>> {
    let versions_dir = config_dir.join("versions");
    if !versions_dir.exists() {
        return Ok(None);
    }
    let re = Regex::new(regex).context("invalid regex for latest matching")?;
    let mut candidates: Vec<Version> = Vec::new();
    for entry in fs::read_dir(versions_dir)? {
        let e = entry?;
        if e.path().is_dir() {
            if let Some(name) = e.file_name().to_str() {
                if re.is_match(name) {
                    if let Ok(v) = Version::parse(name) {
                        candidates.push(v);
                    }
                }
            }
        }
    }
    candidates.sort();
    candidates.reverse();
    Ok(candidates.first().map(|v| v.to_string()))
}

fn latest_remote_matching(regex: &str) -> Result<Option<String>> {
    let product = env::var("TFENV_PRODUCT")
        .unwrap_or_else(|_| "terraform".to_string())
        .to_lowercase();
    let remote = env::var("TFENV_REMOTE").unwrap_or_else(|_| {
        if product == "terraform" {
            "https://releases.hashicorp.com/terraform/".to_string()
        } else if product == "opentofu" {
            "https://github.com/opentofu/opentofu/releases".to_string()
        } else {
            "https://releases.hashicorp.com/terraform/".to_string()
        }
    });
    let body = reqwest::blocking::get(&remote)?.text()?;
    let doc = Html::parse_document(&body);
    let selector = Selector::parse("a").unwrap();
    let re = Regex::new(regex).context("invalid regex for latest remote matching")?;
    let mut versions: Vec<Version> = Vec::new();
    for el in doc.select(&selector) {
        if let Some(href) = el.value().attr("href") {
            if product == "terraform" {
                if let Some(caps) = href.strip_prefix("/terraform/") {
                    let v = caps.trim_end_matches('/');
                    if re.is_match(v) {
                        if let Ok(vers) = Version::parse(v) {
                            versions.push(vers);
                        }
                    }
                }
            } else if product == "opentofu" {
                if let Some(pos) = href.find("/opentofu/opentofu/releases/tag/v") {
                    let v = &href[pos + "/opentofu/opentofu/releases/tag/v".len()..];
                    let v = v.trim_end_matches('/');
                    if re.is_match(v) {
                        if let Ok(vers) = Version::parse(v) {
                            versions.push(vers);
                        }
                    }
                }
            }
        }
    }
    versions.sort();
    versions.reverse();
    Ok(versions.first().map(|v| v.to_string()))
}

fn min_required(_config_dir: &Path) -> Result<Option<String>> {
    // search TFENV_DIR (cwd) and config_dir? We'll search cwd
    let cwd = env::current_dir()?;
    let mut combined = String::new();
    // read *.tf and *.tf.json in cwd
    if let Ok(entries) = fs::read_dir(&cwd) {
        for ent in entries.flatten() {
            if let Some(name) = ent.file_name().to_str() {
                if name.ends_with(".tf") || name.ends_with(".tf.json") {
                    if let Ok(s) = fs::read_to_string(ent.path()) {
                        combined.push_str(&s);
                        combined.push('\n');
                    }
                }
            }
        }
    }
    // find lines with required_version
    let mut versions: Vec<String> = Vec::new();
    let re_line =
        Regex::new(r#"(?m)^\s*[^#]*required_version\s*[:=]?\s*\(?"?(?P<spec>[^"]+)"?\)?"#).unwrap();
    for cap in re_line.captures_iter(&combined) {
        if let Some(spec) = cap.name("spec") {
            versions.push(spec.as_str().to_string());
        }
    }
    if versions.is_empty() {
        return Ok(None);
    }
    // take first found, attempt to extract numeric part
    let first = &versions[0];
    // use find numeric sequence
    let re_ver = Regex::new(r"([~=!<>]{0,2}\s*)([0-9]+(?:\.[0-9]+){0,2})(-[a-z]+[0-9]+)?").unwrap();
    if let Some(cap) = re_ver.captures(first) {
        let qualifier = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        if qualifier.trim_start().starts_with("!=") {
            return Ok(None);
        }
        let mut found = cap.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
        if let Some(post) = cap.get(3) {
            found.push_str(post.as_str());
        }
        // pad to x.y.z
        let pad_re = Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+$").unwrap();
        while !pad_re.is_match(&found) {
            found.push_str(".0");
        }
        return Ok(Some(found));
    }
    Ok(None)
}

fn latest_allowed_to_requested(_config_dir: &Path) -> Result<Option<String>> {
    // replicate tfenv-resolve-version's logic for latest-allowed
    // find required_version spec
    let cwd = env::current_dir()?;
    let mut spec_line = String::new();
    if let Ok(entries) = fs::read_dir(&cwd) {
        for ent in entries.flatten() {
            if let Some(name) = ent.file_name().to_str() {
                if name.ends_with(".tf") || name.ends_with(".tf.json") {
                    if let Ok(s) = fs::read_to_string(ent.path()) {
                        for line in s.lines() {
                            if line.contains("required_version") {
                                spec_line = line.to_string();
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    if spec_line.is_empty() {
        return Ok(None);
    }
    // crude extract version spec between quotes
    let parts: Vec<&str> = spec_line.split('"').collect();
    let version_spec = if parts.len() >= 2 {
        parts[1]
    } else {
        spec_line.as_str()
    };
    let version_num = Regex::new(r"[0-9.]+")?
        .find(version_spec)
        .map(|m| m.as_str())
        .unwrap_or("");
    // determine mapping
    if version_spec.trim_start().starts_with('>') {
        return Ok(Some("latest".to_string()));
    }
    if version_spec.trim_start().starts_with("<=") || version_spec.trim_start().starts_with('<') {
        return Ok(Some(version_num.to_string()));
    }
    if version_spec.trim_start().starts_with("~>") {
        // remove rightmost component
        if let Some(pos) = version_num.rfind('.') {
            let prefix = &version_num[..pos];
            return Ok(Some(format!("latest:^{}\\.", prefix)));
        }
    }
    Ok(None)
}
