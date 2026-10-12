# Changelog

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
