# tfenv-rs

A minimal Rust port of `tfenv` (MVP). This project implements a small subset of `tfenv` features:

- Command dispatch (subcommands: `exec`, `use`, `version`, `list`, `list-remote`)
- Version resolution from `TFENV_TERRAFORM_VERSION`, `./.terraform-version`, and `~/.terraform-version`
- `exec` runs the `terraform` binary located at `TFENV_CONFIG_DIR/versions/<version>/terraform`

Quick start (from the new repo root):

```powershell
# build
cargo build

# show resolved version
cargo run -- version

# run terraform with resolved version
cargo run -- exec -- --version
```

Notes:
- `install` is intentionally not implemented in this MVP.
- `list-remote` is a crude fetch and HTML parse; consider improving with proper HTML parsing or API usage.

Product support

You can install either Terraform (default) or OpenTofu by setting `TFENV_PRODUCT` to `terraform` or `opentofu`.
Examples:

```powershell
# install terraform 1.6.3
TFENV_PRODUCT=terraform cargo run -- install 1.6.3

# install opentofu 0.1.0 (will use GitHub releases by default)
TFENV_PRODUCT=opentofu cargo run -- install 0.1.0
```

Note: checksum/PGP verification is enabled by default for HashiCorp Terraform releases; for OpenTofu the installer will skip checksum verification unless you provide `TFENV_REMOTE` with appropriate checksum files or opt-in mechanisms.

Simple usage (matching `tfenv` semantics)

After building or installing the `tfenv-rs` binary, the CLI mirrors original `tfenv` behavior:

```powershell
# show program version and help
tfenv

# install resolved version from .terraform-version or TFENV_TERRAFORM_VERSION (no arg)
tfenv install

# install an explicit version
tfenv install 1.6.3

# use a version (set default)
tfenv use 1.6.3

# list installed versions
tfenv list

# list remote versions
tfenv list-remote

# run terraform with the selected version
tfenv exec -- version
```

