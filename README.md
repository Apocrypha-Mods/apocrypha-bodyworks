# Apocrypha Bodyworks

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
- 704 of 933 built meshes are the original's file byte for byte. 84 of the
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

### Another body for Skyrim (UBE)

Held against UBE 2.0 and its armour set (636 meshes, 299 sets): every mesh
reads and writes back as it was, every project and slider data file reads
and writes back, 298 sets build (the last names a mesh its package does not
have), and one project file shipped broken (a set closed twice) is refused
as it is not XML. Its heads' parts (brows, eyes) showed two faults since
mended: a part of a head left unreadable when vertices were taken out of
it, and changes that give one other vertices altogether, which left its
places out of step with the rest of each vertex (they are kept apart) and
now take them along. Two parts of a head are not merged.

### Fallout 4

Held against CBBE for Fallout 4's own shipped meshes (348 sets, the "Curvy"
option's files), with `TOOLS=data/tools/bodyslide/` for the same examples:
344 sets build the same shapes, vertices and triangles, and 193 of them are
the original's file byte for byte. The rest differ by a few bytes each: a
vertex or two whose direction comes out one step away in its last place.
The four sets that do not compare were shipped built from another mesh
than the set now names. Building, the preview
and saving presets work. In the Studio its skins (bones, weights, where
each bone stands; all 3,253 read and write back as they were,
`--example skinsame`) and its segments (all 3,195 likewise,
`--example segback`) are understood, so shapes are refined, welded, cut,
separated, duplicated, mirrored, made from files, and have weights
painted and copied, their segments holding every triangle afterwards
(`--example statics` over 1,643 meshes), two are merged, a shape with no
colours is given them, and its glow, shine, smoothness and opacity are
changed (the numbers a mesh keeps; a material file it names has its own,
which this does not read); a plain shape is parted from its bones. Not
yet for Fallout 4: parting a shape with segments from its bones (none
was found without), the kind
and switches of its lighting, adding or removing a segment (triangles are
moved between those a shape has), and a bone no shape of the mesh follows
yet (nothing in these files says where one stands).

## How fast it is

Measured on CBBE's body (13,554 vertices, 136 sliders), not assumed
(`crates/app/examples/timing.rs` for the module, built natively; a headless
browser for the screen's own script, drawing apart):

- The module: a preview read in 7 ms, a project opened in 5 ms, a brush
  stroke or any other change taken in about 5 ms, a build of both weights
  with its morph file in 19 ms.
- The screen: a slider moved in about 1 ms, a brush moved in under 1 ms, a
  stroke's end in about 15 ms.
- FBX files (in Node, the same script): the body written with its 26
  bones in 3 ms; a project of three shapes read back in 19 ms, and the same
  as Blender writes it (compressed) in 45 ms.

Larger meshes cost in proportion: every brush move looks at each vertex and
triangle of the picked shape once.

## What is not here yet

Held against what the original two programs offer, from using them and from
their documentation. Each line is something they do that this does not.

Outfit Studio:

- Brushes: a shape with none is given colours to paint only where its
  vertices are laid out one of three ways real files pair with a coloured
  one, and it shares its lighting with no other shape.
- Shapes: merging shapes that keep different things for each vertex,
  kinds of lighting beyond the four most used (plain,
  environment map, glow map, skin), and lighting switches beyond the ten
  most used (its textures, glow, shine, gloss
  and opacity are changed).
- Meshes that hang together or move in the rarer ways (collision joined by
  constraints, animation other than a node's own moving): their shapes are changed in place, but
  not taken out, copied or merged. Other meshes that follow no bones
  (weapons, clutter) take every change a skinned one does, measured over
  2,160 of the game's own (`--example statics`).
- Segments (Fallout 4): viewing and editing.
- Sliders: importing and exporting slider data as BSD or FBX (OSD and TRI
  go both ways).
- Files: FBX files written as text (the binary kind goes both ways:
  meshes, their texture places, and the bones they follow with their
  weights, a bone the project has not got being made where the file
  stands it; all checked against Blender's own reading and writing),
  Skyrim's physics files. (Fallout 4's cloth data goes out of a mesh as
  the Havok file it is and into one; all 215 measured are laid out the
  same, `--example cloth`. A head part's morph file goes both ways: it becomes sliders, and
  sliders go out as one; all 87 of a real body mod's read and write back
  the same, `--example headtri`.)
- View: bones drawn as more than a dot where each is. (What a set says of
  a shape's normals is kept and followed when it is built: left as the
  mesh has them, or a seam's sides lit as one within another angle.)
- The original's own keyboard shortcuts: this has its own (undo and redo,
  Ctrl+S and Ctrl+E, Q and the digits for the brushes, the square brackets
  and minus and equals for a brush's size and strength, X for both sides,
  W, T, F, H and G for the view).

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
tools/pack.sh                                                 # dist/bodyworks-<version>.apocaddon
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
