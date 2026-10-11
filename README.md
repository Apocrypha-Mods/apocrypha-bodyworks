# Body and outfit tools for Apocrypha

An add-on for the Apocrypha mod manager: build bodies and outfits from slider
sets and presets, and edit outfits, inside the mod manager's own window. It
reads the same files BodySlide and Outfit Studio do, so existing bodies,
outfits and presets work.

It is not part of the mod manager and is not shipped with it. It is installed
from inside the mod manager, runs as WebAssembly in a frame there, and can
reach only what the mod manager hands it.

## Clean-room

BodySlide and Outfit Studio are GPL-3.0. This project is MIT and is written
without reading their source code, shaders or bundled files. File formats are
worked out from the files themselves, from format descriptions published by
others, and from comparing this project's output with the original's.
Contributions must keep to that: do not submit code translated from, or
written while looking at, the original program's source.

No bodies, presets or meshes are distributed here. Those are yours.

## Layout

| Path | What |
| --- | --- |
| `crates/nif` | Reading and writing NIF mesh files |
| `crates/bodyslide` | Slider sets, slider data, presets, and building meshes from them |
| `crates/app` | The WebAssembly module: what the interface asks of the crates, over one call |
| `ui` | The interface shown in the mod manager's frame, and `addon.json`, which describes the add-on to it |
| `tools` | `dev.cjs` runs the add-on outside the mod manager; `pack.sh` builds the file it installs |

## Working on it

```bash
rustup target add wasm32-unknown-unknown
cargo build --release -p apoc-bodyslide-app --target wasm32-unknown-unknown
node tools/dev.cjs /path/to/a/folder/laid/out/like/the/game   # then open the address it prints
tools/pack.sh                                                 # dist/bodyslide-<version>.apocaddon
```

The interface is plain script with no build step and no dependencies. What it
may ask of the mod manager is listed at the top of `ui/host.js`.

## Checks

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all --check
```

Tests that need real meshes look in `scratch/` (not tracked) and are skipped
when it is empty.
