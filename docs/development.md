# Development

How to build Kition Sheet from source, run its tests, take the README's pictures and publish the
kit's package. To use Sheet, start with the [README](../README.md).

## Repository layout

| Path | Contents |
| --- | --- |
| `crates/` | The engine (Rust), in groups: `kdm`, `engine`, `formats`; compiled to WebAssembly for the editor |
| `bindings/` | The engine's two outlets: `wasm` (the in-page editor's engine, behind Kition's `KitDocument` contract) and `headless` (the `kition-sheet` program for Kition's agent and `kition-kits-mcp`) |
| `web/` | The kit's editor (React, on kition-sdk's `@kition/kit-editor`) |
| `agent/` | The agent guide and checks (KHP) |
| `conformance/` | The sample workbooks the conformance checks open (KHP) |
| `kit.json` | The kit manifest (KHP) |
| `features.yaml`, `cases/` | The feature catalogue and its cases ([quality.md](quality.md)) |
| `docs/` | Documents: entry documents at the first level, parity documents in `docs/parity/`, feature documents in `docs/features/` |
| `assets/readme/` | The README's pictures |
| `tools/` | The repository's own tools: `tools/xtask` (`cargo xtask`) |

## Build and test

```sh
cargo test --workspace
cargo build --release -p kition-sheet                  # the headless kit: target/release/kition-sheet
cd web && pnpm install && pnpm engine && pnpm test:flow
```

`pnpm engine` builds `bindings/wasm` into `web/public/engine` and copies fonts from a
[kition-fonts](https://github.com/KitionAI/kition-fonts) checkout next to this repository (or the
one `KITION_FONTS_DIR` names). `pnpm dev` serves `dev.html`, which hosts the editor the way Kition
does; `web/package.json` lists the other browser tests (`test:golden-*`, `test:chart`,
`test:export`, …).

The check every change passes is `cargo xtask ci` ([quality.md](quality.md)); `cargo xtask --help`
lists the other tasks. `cargo xtask parity` regenerates
[parity/parity-checklist.md](parity/parity-checklist.md).

### The headless kit for an agent

kition-sdk's `kition-kits-mcp` serves `kition-sheet` as MCP tools (`sheet_inspect`, `sheet_run`,
`sheet_check`, …) on a folder of workbooks, for any MCP client; it finds `kit.json` above the
executable:

```sh
cargo build --release -p kition-sheet
cargo install --git https://github.com/KitionAI/kition-sdk kition-kits --bin kition-kits-mcp
kition-kits-mcp --workspace <folder> --kit target/release/kition-sheet
```

[agent/guide.md](../agent/guide.md) is the guide a model reads first; kition-sdk's
[agent guide](https://github.com/KitionAI/kition-sdk/blob/main/docs/sdk/agent.md) covers the
server's options.

### The editor's dependencies

By default `pnpm install` in `web/` installs the published `@kition/*` packages from npm, at the
versions the committed `web/pnpm-lock.yaml` pins, so the editor needs no other checkout. To
develop against local checkouts instead, run `KITION_LINK=1 pnpm install` in `web/`:
`web/.pnpmfile.cjs` then links kition-sdk's `@kition/kdm`, `@kition/kit-editor`,
`@kition/kit-engine`, `@kition/kit-host`, `@kition/kit-ui-protocol` and `@kition/kit-dev-host`,
and Kition Tessera's `@kition/tessera-icons`, `@kition/tessera-ui` and `@kition/tessera-tokens`,
from the kition-sdk and kition-tessera checkouts next to this repository (or the ones
`KITION_SDK` and `KITION_TESSERA` name). The linked install rewrites `web/pnpm-lock.yaml`; do not
commit a lockfile changed that way.

## Take the README pictures

[`web/scripts/readme-shots.mjs`](../web/scripts/readme-shots.mjs) takes the editor pictures in
`assets/readme/` with kition-sdk's shared runner (`tools/readme-shots/` there): the development
host in Classic mode, light theme, English, 1440 × 900 at device scale 2, on the kition-showcase
samples `cafe-chain-quarterly-rollup`, `number-format-gallery`, `filtered-sales-orders` and
`warehouse-inventory-table`. It needs kition-sdk and kition-showcase checkouts next to this
repository (or `KITION_SDK` and `KITION_SHOWCASE`), the engine in `web/public/` (`pnpm engine`),
and no HTTP proxy set:

```sh
cd web
node scripts/readme-shots.mjs                 # every picture, or name some: hero filter
cd ../assets/readme
pngquant --quality 70-90 --force --ext .png hero.png formulas.png number-formats.png filter.png table-styles.png
```

Each picture stays under 400 KB. The `made-*.png` pictures are copies of kition-showcase samples'
`thumb.png`. Every file in `assets/readme/` has a row in [attribution.md](attribution.md)
(`cargo xtask assets`).

## Publish

Kition ships the kit's package, `kition-sheet-<version>.tgz` (the `sheet-ui/` folder), a release
asset of this repository tagged `package-v<version>`. It is built on GitHub, never on a developer
machine: the Package workflow (`.github/workflows/package.yml`, which calls Kition's reusable
`kit-package.yml`) builds the commit it is started on, with `gh workflow run package.yml -R
KitionAI/kition-sheet`, and publishes the Release with the package's lock entry,
`kition-sheet-<version>.json`. Kition pins it with `node scripts/pin-kits.mjs sheet` (see
`docs/kits.md` there). A version is published once: bump `version` in `kit.json` to publish a
change.
