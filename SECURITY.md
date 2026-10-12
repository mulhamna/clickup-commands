# Security Policy

clickupc handles ClickUp personal API tokens, which grant full access to the workspaces their owner can see. Security reports are taken seriously and handled privately.

## Supported versions

| Version | Supported |
| --- | --- |
| Latest `1.x` release | ✓ |
| Older `1.x` releases | Fixes ship in a new release; please upgrade |
| `0.0.0` | ✗ (name-reservation placeholder, not functional) |

## Reporting a vulnerability

**Please don't open a public issue.** Report privately through
[GitHub Security Advisories](https://github.com/mulhamna/clickup-commands/security/advisories/new).

Include, as far as you can:

- the affected version (`clickupc --version`) and install channel,
- what an attacker can do and what they need (local access, a malicious MCP client, a crafted API response, …),
- steps or a proof of concept — **with real tokens, workspace names and task data removed**.

What to expect:

- an acknowledgement within a few days,
- an assessment and, if confirmed, a fix in a patch release with a GitHub Security Advisory crediting you (unless you'd rather stay anonymous),
- coordinated disclosure: please give us a reasonable window to ship the fix before publishing details.

## Scope

In scope, for example:

- a personal token (`pk_…`) being printed, logged, written into MCP client configs, or sent anywhere other than the ClickUp API,
- destructive MCP tools running without `confirm: true`, or destructive CLI commands running without confirmation / `--yes`,
- the install scripts or the npm installer accepting a binary that doesn't match `checksums.txt`,
- path traversal or arbitrary file reads through `clickup_attachment_upload` or other inputs beyond what the caller explicitly passed,
- weaknesses in the release pipeline (workflow permissions, publishing, provenance).

Out of scope:

- actions an MCP client or AI agent performs with tools you deliberately granted it and confirmed,
- vulnerabilities in ClickUp itself — report those to ClickUp,
- issues requiring an already-compromised machine or user account,
- findings from automated scanners without a demonstrated impact.

## How clickupc protects your token

- The token lives in the clickupc config file (created with permissions `600` on Unix) or in `CLICKUP_TOKEN`; `clickupc mcp install` never writes it into client configs — entries only carry the profile name.
- It is sent only in the `Authorization` header to the configured ClickUp API base URL, and `pk_…` values are redacted from errors, logs and debug output.
- Release binaries are built in GitHub Actions with build provenance attestations; crates.io and npm packages are published through OIDC trusted publishing, so no long-lived publish tokens exist. See [INSTALL.md](INSTALL.md#verify-a-download) to verify a download.
