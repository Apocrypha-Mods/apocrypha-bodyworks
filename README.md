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

## How close it is

Measured, not claimed. CBBE ships its slider sets and also the meshes and
morph files the original program built from them, so the two can be compared
(`crates/bodyslide/examples/corpus.rs`, `trifit.rs`; the data is the user's
own and is not in this repository). Over CBBE's 479 sets for Skyrim Special
Edition:

- 468 of 478 build to the same shapes, vertices and triangles as the
  original's file. The other 10 are sets that share an output file with
  another set, or whose shipped mesh was built from slightly different data.
- 684 of 933 built meshes are the original's file byte for byte. 84 of the
  rest are the physics variant of a set whose plain variant wrote the shipped
  file. What is left differs in the normals of a few vertices on seams.
- 458 of 478 morph files (`.tri`) are the original's byte for byte; all 429
  of the original's read and write back unchanged.
- All 479, opened as a project in Outfit Studio and saved, build to the same
  bytes from the saved project as from the original (`resave.rs`). Whether
  the original program opens a project saved here has not been checked.
- Conforming an outfit to a body is this project's own method, not the
  original's. For the outfits that come with a whole reference body, what it
  gives is off the slider data they shipped with by about 10% of how far
  that data moves, 4% for the middle shape (`conformfit.rs`). The shipped
  data was also touched up by hand, so some of that is not the method's.
- Copying a body's bone weights to an outfit is likewise this project's
  own method. Given to CBBE's own underwear in place of the weights it was
  made with, 1,159 of its 1,164 vertices still follow, most of all, the bone
  they did. Nothing skinned this way has been seen moving in the game yet.

Rules found this way are written where they are used, each with what it was
found from.

## How fast it is

Measured on CBBE's body (13,554 vertices, 136 sliders), not assumed
(`crates/app/examples/timing.rs` for the module, built natively; a headless
browser for the screen's own script, drawing apart):

- The module: a preview read in 7 ms, a project opened in 5 ms, a brush
  stroke or any other change taken in about 5 ms, a build of both weights
  with its morph file in 19 ms.
- The screen: a slider moved in about 1 ms, a brush moved in under 1 ms, a
  stroke's end in about 15 ms.

Larger meshes cost in proportion: every brush move looks at each vertex and
triangle of the picked shape once.

## What is not here yet

Held against what the original two programs offer, from using them and from
their documentation. Each line is something they do that this does not.

BodySlide:

- Building to a folder outside the mod manager.

Outfit Studio:

- Brushes: the vertex tools (collapse, flip or split an edge, weld, move one
  vertex). Colour is painted only on a shape that already carries colours.
- Brush settings: spacing.
- Shapes: refining (subdividing) a mesh, merging shapes,
  removing skinning, shader properties other than the texture files, making
  a shape from an OBJ or FBX file.
- Bones: adding a bone no shape of the mesh follows, custom bones,
  transferring selected weights between shapes.
- Segments (Fallout 4): viewing and editing.
- Sliders: combined sliders, importing and exporting slider data as BSD,
  TRI or FBX (OSD goes both ways).
- Files: FBX in and out, head TRI files, physics data, packing projects for
  sharing, making a conversion reference.
- View: bones drawn as more than a dot where each is, field of view and lighting
  settings, recalculating or locking normals by hand.
- Most keyboard shortcuts: there are undo and redo, Q and the digits for the
  brushes (1 to 8), the square brackets for a brush's size, W and T for how the
  outfit is drawn.

Games: only Skyrim Special Edition has been measured. Skyrim (2011),
Fallout 4, Fallout 3, New Vegas and Oblivion meshes are read in part or not
at all, and none has been checked against files the original built.

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
node --test ui-tests/
```

Tests that need real meshes look in `scratch/` (not tracked) and are skipped
when it is empty.
