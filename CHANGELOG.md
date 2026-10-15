# Changelog

## [1.3.0](https://github.com/mulhamna/clickup-commands/compare/v1.2.0...v1.3.0) (2026-10-15)


### Features

* **cli:** accept several workspaces in workspace use ([90663a8](https://github.com/mulhamna/clickup-commands/commit/90663a8b21295082c0dd2ae8741da4ad399daad8))
* **cli:** fan out task list and standup over active workspaces ([f61993a](https://github.com/mulhamna/clickup-commands/commit/f61993a6a2afefeb874e2b384f9966d6de7fb81b))
* **core:** add active_workspaces to profiles ([d549988](https://github.com/mulhamna/clickup-commands/commit/d5499882b3b169fcaf935e3b8f99e701c688169d))
* **mcp:** fan out task search and standup over active workspaces ([2a81bf0](https://github.com/mulhamna/clickup-commands/commit/2a81bf078f3c1653640b0ed298ac4cf314072d45))
* query several active workspaces per profile ([9006a0c](https://github.com/mulhamna/clickup-commands/commit/9006a0cb6be8248a9fe8e070d266a9dd86a4e09d))


### Bug Fixes

* **core:** leave multi-workspace mode on login ([8abe0d7](https://github.com/mulhamna/clickup-commands/commit/8abe0d7df8dc3a51b3b2fef48445552bd00358d3))

## [1.2.0](https://github.com/mulhamna/clickup-commands/compare/v1.1.0...v1.2.0) (2026-10-14)


### Features

* **cli:** add tokens per workspace on login ([a8e51bc](https://github.com/mulhamna/clickup-commands/commit/a8e51bc1cb7994f16500fb250d6ce906121c5cd6))
* **core:** resolve workspaces across all profile tokens ([8e28c82](https://github.com/mulhamna/clickup-commands/commit/8e28c821c5a7a6eff36a5da9725470944028cb3d))
* **core:** store per-workspace tokens in profiles ([aa9c600](https://github.com/mulhamna/clickup-commands/commit/aa9c600196d65c1456a546919c1bd9c4beeb9a17))
* **mcp:** switch token with the workspace override ([dced73f](https://github.com/mulhamna/clickup-commands/commit/dced73fe31e7b5b15b40ed718055f3e1a2d2b100))
* support per-workspace tokens in one profile ([04f3739](https://github.com/mulhamna/clickup-commands/commit/04f37398fa74c4b7e477e65c6da402c254165bc4))


### Bug Fixes

* **deps:** bump rustls to 0.23.46 ([675a6ea](https://github.com/mulhamna/clickup-commands/commit/675a6ea43f17213e0b499bfaf6138fe2509b7787))

## [1.1.0](https://github.com/mulhamna/clickup-commands/compare/v1.0.2...v1.1.0) (2026-10-12)


### Features

* **cli:** multi-account login, --workspace and multi-target mcp install ([e9d54f4](https://github.com/mulhamna/clickup-commands/commit/e9d54f4cad45a7c9b6233259936ad112a312e9bd))
* **core:** look up workspaces by name and suggest profile names ([75bdf7e](https://github.com/mulhamna/clickup-commands/commit/75bdf7ecbd6e4f969dffe1f43839a9b04a7f273a))
* **mcp:** let every tool target a profile and workspace ([9bd9d6a](https://github.com/mulhamna/clickup-commands/commit/9bd9d6a55f561215e1edc005eb658644e8da3738))
* multiple accounts, workspaces and multi-client MCP install ([8a86bc3](https://github.com/mulhamna/clickup-commands/commit/8a86bc3ffbe62a388b7c1d6bf9517d0a14d2af89))


### Bug Fixes

* **cli:** keep CLICKUP_PROFILE as the first-login profile ([5989fbd](https://github.com/mulhamna/clickup-commands/commit/5989fbde9bd6ee718776ef54a432d0c4184b21c5))
* **cli:** register the MCP server as clickupc by default ([e4ad572](https://github.com/mulhamna/clickup-commands/commit/e4ad572e668eb36d252a30a416bcd490a3e7b705))
* **cli:** register the MCP server as clickupc by default ([e32e485](https://github.com/mulhamna/clickup-commands/commit/e32e485c0c00a6d1c3c2c7fde053f539aed06522))

## [1.0.2](https://github.com/mulhamna/clickup-commands/compare/v1.0.1...v1.0.2) (2026-10-12)


### Bug Fixes

* **cli:** describe every command and subcommand in --help ([58501cf](https://github.com/mulhamna/clickup-commands/commit/58501cfdd0b5cc748b2916c3bb16d242a8a0ad9c))
* complete CLI help and rewrite READMEs for every channel ([a457cc2](https://github.com/mulhamna/clickup-commands/commit/a457cc26f38554db04432d742243fdd98dd0dd65))

## [1.0.1](https://github.com/mulhamna/clickup-commands/compare/v1.0.0...v1.0.1) (2026-10-12)


### Bug Fixes

* **ci:** accept first-release CHANGELOG heading in release validation ([1a68791](https://github.com/mulhamna/clickup-commands/commit/1a687910a440ff8bb48c226c98664357abc09ac6))
* **ci:** accept the first-release CHANGELOG heading in release validation ([4b03557](https://github.com/mulhamna/clickup-commands/commit/4b0355756cf1729fc5e7241c475b3ae0067639dd))

## 1.0.0 (2026-10-12)


### Features

* add checksum-verified install scripts for Unix and Windows ([2a613fe](https://github.com/mulhamna/clickup-commands/commit/2a613fef5269cededb94965733b7cb6a774d33d4))
* **cli:** add clickupc commands for tasks, hierarchy, time and docs ([e92f9c8](https://github.com/mulhamna/clickup-commands/commit/e92f9c86d94ab40fa248180707885a0450003244))
* **cli:** show the clickupc banner in long help ([f189494](https://github.com/mulhamna/clickup-commands/commit/f1894943fa9436590781e1261f7667a507ab4a37))
* **core:** add ClickUp client, resolver and models ([53875a6](https://github.com/mulhamna/clickup-commands/commit/53875a64532601bd5b538d6657f757f12153a6b0))
* **core:** add shared human-input operations ([64d90b7](https://github.com/mulhamna/clickup-commands/commit/64d90b78d86429088b3ed8cf2b58afaf2823c9f4))
* **core:** expose timezone name validation ([0c6fdd3](https://github.com/mulhamna/clickup-commands/commit/0c6fdd3b8641cceeb8918d15607755f50cf5a0a0))
* **core:** support exact-match custom field filter operators ([164d2d2](https://github.com/mulhamna/clickup-commands/commit/164d2d2f9ae8374c24c6689129d8a529f39fed3b))
* initial clickupc CLI, MCP server and OIDC release pipeline ([ef3e203](https://github.com/mulhamna/clickup-commands/commit/ef3e2039ce2b9b5ebe336d4a83f324702ab775d2))
* **mcp:** expose ClickUp as 55 clickup_* MCP tools ([b335b1e](https://github.com/mulhamna/clickup-commands/commit/b335b1eb6920c0fdeeb03ab58395df4b9b15992a))


### Bug Fixes

* **cli:** show prerequisites before the mcp install picker ([4f52a63](https://github.com/mulhamna/clickup-commands/commit/4f52a63d2983cb5d0ad1f6a4cdd79335ad52f88d))
* **core:** never fall back to the email when displaying a user ([62a3360](https://github.com/mulhamna/clickup-commands/commit/62a336067b979f27aa90a0bb9f4f286e266aea84))
* **core:** reject out-of-range relative dates and durations ([cc08dba](https://github.com/mulhamna/clickup-commands/commit/cc08dba58f05599eeed9b10ce3fb816dbd2dd03c))
* **core:** resolve numeric list ids without a hierarchy walk ([5558aad](https://github.com/mulhamna/clickup-commands/commit/5558aad4607118df0f644fefc71fe4ed93dc06a5))
* **core:** stop relative-date parsing from panicking on odd input ([87920be](https://github.com/mulhamna/clickup-commands/commit/87920be43bc15bbc3d8d04dc623bc677958dc728))
