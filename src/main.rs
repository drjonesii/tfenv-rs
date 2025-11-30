use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
mod installer;
use installer::install_version;
use installer::terraform_binary_name;
mod version;
use version::resolve_version_name;

#[derive(Parser)]
#[command(name = "tfenv")]
#[command(version)]
#[command(about = "Terraform version manager (rust port)", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a command using the selected Terraform version
    Exec { args: Vec<String> },
    /// Print resolved version
    Version,
    /// Use/set a version (writes version file)
    Use { version: String },
    /// Install a version (not fully implemented)
    Install { version: Option<String> },
    /// List installed versions
    List,
    /// List remote versions (optional: filter by 'terraform' or 'opentofu')
    ListRemote { product: Option<String> },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let tfenv_root = detect_tfenv_root()?;
    let config_dir = env::var("TFENV_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| tfenv_root.clone());
    if let Some(cmd) = cli.command {
        match cmd {
            Commands::Exec { args } => run_exec(&tfenv_root, &config_dir, &args),
            // `version` prints the resolved terraform/version selection (like tfenv use/resolution)
            Commands::Version => {
                let v = resolve_version_name(&tfenv_root, &config_dir)?;
                println!("{}", v);
                Ok(())
            }
            Commands::Use { version } => set_default_version(&config_dir, &version),
            Commands::Install { version } => {
                // If no version supplied, resolve via the same rules as `use`/`exec`
                if let Some(v) = version {
                    install_version(&tfenv_root, &config_dir, Some(&v))
                } else {
                    let resolved = resolve_version_name(&tfenv_root, &config_dir)?;
                    install_version(&tfenv_root, &config_dir, Some(&resolved))
                }
            }
            Commands::List => list_installed(&config_dir),
            Commands::ListRemote { product } => list_remote(product),
        }
    } else {
        // No command: print program version and help (similar to original tfenv behavior)
        println!("tfenv-rs {}", env!("CARGO_PKG_VERSION"));
        let mut cmd = <Cli as clap::CommandFactory>::command();
        cmd.print_help()?;
        println!();
        Ok(())
    }
}

fn detect_tfenv_root() -> Result<PathBuf> {
    if let Ok(root) = env::var("TFENV_ROOT") {
        return Ok(PathBuf::from(root));
    }

    // Fallback: assume repo layout when running from source. Use executable's parent/.. like bash shim
    let exe = env::current_exe().context("failed to get current exe path")?;
    if let Some(parent) = exe.parent().and_then(|p| p.parent()) {
        return Ok(parent.to_path_buf());
    }

    Err(anyhow::anyhow!("Unable to determine TFENV_ROOT"))
}



fn run_exec(tfenv_root: &Path, config_dir: &Path, args: &[String]) -> Result<()> {
    let version = resolve_version_name(tfenv_root, config_dir)?;
    let tf_path = config_dir
        .join("versions")
        .join(&version)
        .join(terraform_binary_name());
    if !tf_path.exists() {
        // Auto-install if TFENV_AUTO_INSTALL is true (default true)
        let auto = env::var("TFENV_AUTO_INSTALL").unwrap_or_else(|_| "true".to_string());
        if auto == "true" {
            println!("Version {} not installed; auto-installing...", version);
            install_version(tfenv_root, config_dir, Some(&version))?;
        } else {
            anyhow::bail!(
                "Terraform binary for version '{}' not installed at {}",
                version,
                tf_path.display()
            );
        }
    }

    let mut cmd = Command::new(tf_path);
    if !args.is_empty() {
        cmd.args(args);
    }
    let status = cmd.status().context("failed to execute terraform")?;
    std::process::exit(status.code().unwrap_or(1));
}

fn set_default_version(config_dir: &Path, version: &str) -> Result<()> {
    let path = config_dir.join("version");
    fs::write(&path, version).context("failed to write version file")?;
    println!("Set default version to {}", version);
    Ok(())
}

fn list_installed(config_dir: &Path) -> Result<()> {
    let versions_dir = config_dir.join("versions");
    if !versions_dir.exists() {
        println!("(no versions installed)");
        return Ok(());
    }
    for entry in fs::read_dir(versions_dir)? {
        let e = entry?;
        if e.path().is_dir() {
            if let Some(name) = e.file_name().to_str() {
                println!("{}", name);
            }
        }
    }
    Ok(())
}

fn list_remote(product_filter: Option<String>) -> Result<()> {
    let versions = version::list_remote_versions()?;
    for (v, product) in versions {
        if let Some(ref filter) = product_filter {
            if product.to_lowercase() != filter.to_lowercase() {
                continue;
            }
        }
        println!("{} {}", v, product);
    }
    Ok(())
}
