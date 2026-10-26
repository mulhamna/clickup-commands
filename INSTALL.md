# Installing clickupc

Every channel installs the same single `clickupc` binary, which is both the CLI and the MCP server (`clickupc mcp serve`). Prebuilt binaries exist for:

| OS | x86_64 | aarch64 / arm64 |
| --- | --- | --- |
| Linux (glibc 2.17+) | ✓ | ✓ |
| macOS | ✓ | ✓ (Apple silicon) |
| Windows | ✓ | ✓ |

- [Install script (macOS / Linux)](#install-script-macos--linux)
- [Install script (Windows)](#install-script-windows)
- [Homebrew](#homebrew) · [Scoop](#scoop) · [npm](#npm) · [Cargo](#cargo) · [Manual download](#manual-download)
- [Verify a download](#verify-a-download)
- [After installing](#after-installing) · [Set up an AI client](#set-up-an-ai-client)
- [Upgrade](#upgrade) · [Uninstall](#uninstall) · [Troubleshooting](#troubleshooting)

## Install script (macOS / Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/mulhamna/clickup-commands/main/install.sh | bash
```

The script detects your OS and architecture, downloads the matching archive from the latest GitHub Release, verifies it against the release's `checksums.txt`, and installs `clickupc` to `~/.local/bin`.

| Variable | Default | Purpose |
| --- | --- | --- |
| `CLICKUPC_VERSION` | latest release | Install a specific version, e.g. `v1.1.0` |
| `CLICKUPC_INSTALL_DIR` | `~/.local/bin` | Install directory |

```bash
curl -fsSL https://raw.githubusercontent.com/mulhamna/clickup-commands/main/install.sh \
  | CLICKUPC_VERSION=v1.1.0 CLICKUPC_INSTALL_DIR=/usr/local/bin bash
```

If the install directory isn't on your `PATH`, the script prints the line to add to your shell profile.

## Install script (Windows)

```powershell
irm https://raw.githubusercontent.com/mulhamna/clickup-commands/main/install.ps1 | iex
```

Installs `clickupc.exe` to `%LOCALAPPDATA%\Programs\clickupc\bin` after verifying the checksum, and adds that folder to your user `PATH` (restart the terminal afterwards). With options:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/mulhamna/clickup-commands/main/install.ps1))) `
  -Version v1.1.0 -InstallDir "C:\Tools\clickupc" -SkipPathUpdate
```

| Parameter | Default | Purpose |
| --- | --- | --- |
| `-Version` | latest release | Install a specific version |
| `-InstallDir` | `%LOCALAPPDATA%\Programs\clickupc\bin` | Install directory |
| `-SkipPathUpdate` | off | Don't touch the user `PATH` |

## Homebrew

macOS and Linux:

```bash
brew install mulhamna/tap/clickupc
```

## Scoop

Windows:

```powershell
scoop bucket add mulhamna https://github.com/mulhamna/scoop-bucket
scoop install mulhamna/clickupc
```

## npm

```bash
npm i -g @mulham28/clickupc
```

The package is a thin wrapper: on install it downloads the native binary for your platform from the matching GitHub Release and verifies its SHA-256 against `checksums.txt`. Node.js 18+ is required for the installer only — `clickupc` itself doesn't run on Node.

Use it without a global install, e.g. straight from an MCP client:

```json
{ "mcpServers": { "clickupc": { "command": "npx", "args": ["-y", "@mulham28/clickupc", "mcp", "serve"] } } }
```

## Cargo

Builds from source (Rust stable required):

```bash
cargo install clickupc
```

## Manual download

Grab an archive from the [Releases](https://github.com/mulhamna/clickup-commands/releases) page:

| Platform | Archive | Binary inside |
| --- | --- | --- |
| Linux x86_64 | `clickupc-linux-x86_64.tar.gz` | `clickupc-linux-x86_64` |
| Linux aarch64 | `clickupc-linux-aarch64.tar.gz` | `clickupc-linux-aarch64` |
| macOS Intel | `clickupc-macos-x86_64.tar.gz` | `clickupc-macos-x86_64` |
| macOS Apple silicon | `clickupc-macos-aarch64.tar.gz` | `clickupc-macos-aarch64` |
| Windows x86_64 | `clickupc-windows-x86_64.zip` | `clickupc-windows-x86_64.exe` |
| Windows arm64 | `clickupc-windows-aarch64.zip` | `clickupc-windows-aarch64.exe` |

Extract it, rename the binary to `clickupc` (`clickupc.exe` on Windows) and put it somewhere on your `PATH`.

## Verify a download

Every release ships a `checksums.txt` (SHA-256) and [build provenance attestations](https://github.com/mulhamna/clickup-commands/attestations) produced in GitHub Actions.

```bash
# checksum
shasum -a 256 -c checksums.txt --ignore-missing      # macOS
sha256sum -c checksums.txt --ignore-missing          # Linux

# provenance (requires the GitHub CLI)
gh attestation verify clickupc-macos-aarch64.tar.gz --repo mulhamna/clickup-commands
```

The npm package is published with npm provenance, and the crates are published from the same workflow via crates.io trusted publishing.

## After installing

```bash
clickupc --version
clickupc auth login        # personal API token: ClickUp → Settings → Apps → API Token
clickupc tree
```

`auth login` validates the token, lets you pick a workspace, and stores both in a profile. Run it again to add another account — on a terminal it asks whether to add a new profile or update an existing one.

| | Config (`config.toml`, permissions `600`) | Cache (hierarchy, members, fields) |
| --- | --- | --- |
| Linux | `~/.config/clickupc/` | `~/.cache/clickupc/` |
| macOS | `~/Library/Application Support/clickupc/` | `~/Library/Caches/clickupc/` |
| Windows | `%APPDATA%\clickupc\` | `%LOCALAPPDATA%\clickupc\` |

`CLICKUP_CONFIG_DIR` and `CLICKUP_CACHE_DIR` override these locations; `CLICKUP_TOKEN`, `CLICKUP_WORKSPACE_ID` and `CLICKUP_PROFILE` override what's stored in the profile.

## Set up an AI client

```bash
clickupc mcp install            # checklist, then pick clients (and profiles) with checkboxes
clickupc mcp install --client claude-code --client cursor
clickupc mcp doctor             # binary on PATH, credentials, client config
```

Supported targets: `claude-code`, `claude-code-cli`, `claude-desktop`, `cursor`, `gemini-cli`, `codex`, `vscode`, `copilot-cli`, `opencode`, `windsurf`, `zed`, `openclaw`, `hermes`, `antigravity-cli`, `antigravity-desktop`, `arsy-code`, and `generic-json` (prints a snippet to paste anywhere).

The entry is named `clickupc`, so it coexists with ClickUp's official server. With several profiles selected, each becomes its own entry (`clickupc-<profile>`). The token is never written into client configs — the server reads it from the clickupc config.

## Upgrade

```bash
clickupc update            # upgrade through the channel that installed clickupc
clickupc update --check    # only report whether a newer release exists
```

`clickupc update` asks before it runs the command below for your channel (`--yes` skips the prompt). An install-script binary is reinstalled into its current directory.

| Channel | Command |
| --- | --- |
| Install script | re-run the install command |
| Homebrew | `brew upgrade clickupc` |
| Scoop | `scoop update clickupc` |
| npm | `npm i -g @mulham28/clickupc@latest` |
| Cargo | `cargo install clickupc --force` |

MCP client entries point at the `clickupc` binary, so they pick up the new version on the client's next restart.

## Uninstall

| Channel | Command |
| --- | --- |
| Install script (macOS / Linux) | `rm ~/.local/bin/clickupc` (or your `CLICKUPC_INSTALL_DIR`) |
| Install script (Windows) | delete `%LOCALAPPDATA%\Programs\clickupc` and remove it from your user `PATH` |
| Homebrew | `brew uninstall clickupc` |
| Scoop | `scoop uninstall clickupc` |
| npm | `npm rm -g @mulham28/clickupc` |
| Cargo | `cargo uninstall clickupc` |

To remove your stored tokens as well, run `clickupc auth logout` per profile or delete the config directory above. Remove the `clickupc` entry from your MCP clients' configs (or re-run `clickupc mcp doctor` to see where they live).

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| `clickupc: command not found` | The install directory isn't on `PATH`; add it (the install script prints the exact line) or open a new terminal on Windows. |
| npm: `binary not installed yet` | The postinstall step was skipped (e.g. `--ignore-scripts`). Run `npm rebuild -g @mulham28/clickupc`. |
| `Checksum mismatch` | The download was corrupted or tampered with — retry; if it persists, [open an issue](https://github.com/mulhamna/clickup-commands/issues). |
| macOS blocks a manually downloaded binary | Remove the quarantine flag: `xattr -d com.apple.quarantine /path/to/clickupc` (the install script, Homebrew and npm don't need this). |
| MCP client shows no `clickup_*` tools | `clickupc mcp doctor`, then restart the client. Check that `clickupc auth status` shows a token and workspace. |
| `rate_limited` / `Rate limited until …` | You hit ClickUp's per-token limit (100 req/min on most plans). clickupc already waits and retries; heavy bulk work just takes longer. |
| Token prompt shows nothing when pasting | Upgrade to 1.1.0+, where input is masked (`****`). Alternatively: `echo "$TOKEN" \| clickupc auth login --token -`. |
