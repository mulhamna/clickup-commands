# Contributing to clickupc

Thanks for helping! Bug reports, docs fixes, new commands and MCP tools are all welcome.

- **Questions or ideas** → open an issue with the *Feature request* form.
- **Bugs** → the *Bug report* form (remove tokens and workspace data first).
- **Security issues** → privately, see [SECURITY.md](SECURITY.md).

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## Project layout

```
crates/
├── clickupc-core/   library: config, HTTP client, rate limiting, models, task refs,
│                    filters, name → id resolver, shared human-input operations
├── clickupc-mcp/    library: MCP server (rmcp), one `clickup_*` tool per operation
└── clickupc/        the binary: clap CLI, interactive prompts, `clickupc mcp …`
docs/                CLI and MCP specs, architecture, ClickUp API notes, roadmap
packaging/npm/       npm wrapper that downloads the release binary
install.sh / .ps1    checksum-verified install scripts
```

The rule that keeps this maintainable: **only `clickupc-core` talks to ClickUp.** The CLI and MCP crates never build URLs or look at HTTP statuses; they call core with typed input and render typed results. Anything that takes human input (names, `tomorrow`, option names) goes through `clickupc_core::ops` so the CLI and MCP behave the same.

## Development setup

You need Rust stable (`rustup`), and optionally Node.js 18+ for the npm wrapper.

```bash
git clone https://github.com/mulhamna/clickup-commands
cd clickup-commands
cargo build
cargo run -p clickupc -- --help
```

Try it against your own workspace with a throwaway profile so your real config isn't touched:

```bash
export CLICKUP_CONFIG_DIR=/tmp/clickupc-dev
cargo run -p clickupc -- auth login
cargo run -p clickupc -- mcp serve        # or point an MCP inspector at it
```

## Before you open a PR

Run the same gate CI runs:

```bash
make smoke
# = cargo fmt --all -- --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --all
#   cargo build --all
```

CI additionally runs the tests on Linux, macOS and Windows, `cargo deny`, `cargo audit`, CodeQL, ClusterFuzzLite on the input parsers, `shellcheck` on `install.sh` and a parse check of `install.ps1`.

## Tests

- **Never call the real ClickUp API from tests.** Use [`wiremock`](https://docs.rs/wiremock) and point the client at it (`ClickUpConfig::with_token(…, server.uri())` or `CLICKUP_BASE_URL`).
- New client methods: cover the happy path and the request shape (method, path, query, body). Retry / error behavior is covered centrally in `crates/clickupc-core/tests/client.rs`.
- New parsers of user input: add unit tests, and make sure `crates/clickupc-core/tests/parsers_no_panic.rs` still passes — user text must produce an error, never a panic.
- CLI behavior that depends on exit codes, prompts or output belongs in `crates/clickupc/tests/cli.rs` (runs the real binary).
- MCP tools: `crates/clickupc-mcp/tests/mcp.rs` talks to the server over an in-process transport.
- Tests must not contain real tokens, emails or workspace data.

## Adding a ClickUp endpoint

1. Check it in the official reference (<https://developer.clickup.com>, OpenAPI specs linked from "OpenAPI Specification") and note it in [`docs/CLICKUP-API.md`](docs/CLICKUP-API.md).
2. Add a typed method to `clickupc-core/src/client/<resource>.rs`. Task-scoped methods take `&TaskRef` so custom ids work automatically.
3. Expose it in the CLI (`crates/clickupc/src/cli/`) and/or as an MCP tool (`models.rs` input, `app.rs` logic, `server.rs` registration). Tool descriptions are prompts for the model — say *when* to use the tool and its gotchas.
4. Destructive operations: CLI confirms unless `--yes`; MCP requires `confirm: true`.
5. Update [`docs/CLI.md`](docs/CLI.md) / [`docs/MCP.md`](docs/MCP.md) in the same PR.

## Commits and pull requests

Commits follow [Conventional Commits](https://www.conventionalcommits.org) — release-please derives versions and the changelog from them:

| Type | Effect |
| --- | --- |
| `feat:` | minor release |
| `fix:`, `perf:`, `refactor:` | patch release |
| `feat!:` or a `BREAKING CHANGE:` footer | major release |
| `docs:`, `test:`, `ci:`, `build:`, `chore:` | no release |

Scopes: `core`, `cli`, `mcp`, `docs`, `ci`. Keep commits focused (one logical change each) and explain *why* in the body.

Pull requests: fill in the template (summary, changes, test plan), keep CI green, and mention any change to CLI flags, JSON output, MCP tool schemas or the `clickupc-core` public API — those are user-facing.

## Releases

Maintainers only, and fully automated: running the *release-please* workflow opens a release PR; merging it tags `vX.Y.Z`, which builds all targets and publishes to GitHub Releases, crates.io, npm, Homebrew and Scoop. Never bump versions, edit `CHANGELOG.md` or push tags by hand.

## License

By contributing you agree that your contributions are licensed under the [MIT License](LICENSE).
