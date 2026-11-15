# Characters: making a RaceMenu preset in Bodyworks

The plan for a new part of Bodyworks: make a character's face in the mod
manager and save it as a RaceMenu preset (`.jslot`), which the player loads
in game. Written 15 Nov 2026. Steps 1 to 3 (the engine: reading and writing
presets, presets as characters, RaceMenu's named sliders in the preview) were
built on 15 Nov 2026 and committed in 309e5c4. The screen (step 4) was built
on 15 Nov 2026. The in-game check (step 5) is not
started.

## Decided (owner, 15 Nov 2026)

- It lives **inside Bodyworks**, not in an add-on of its own. Bodyworks
  already rebuilds a save's character (`crates/save`, `plugin`, `character`)
  and has the 3D view, tints, saves and BodySlide presets.
- The first version writes **only a RaceMenu preset for the player**. It does
  not change NPCs (no plugin or FaceGen files) and never writes to a save.

The repository is public (MIT), like the rest of Bodyworks.

## Clean-room

RaceMenu's source is public on GitHub (expired6978/SKSE64Plugins) with no
licence (the advisor checked with `gh`). The same rule as for BodySlide and
Pandora applies: nobody working here reads it. The `.jslot` format is worked
out from preset files, from public descriptions of it, and from what RaceMenu
does with a file in game. Add RaceMenu to the README's clean-room section when
the work starts.

## The format, from the first sample

One sample so far: `scratch/samples/jslot/frau-peach/` (Nexus 122584, a UBE
preset). It is untracked and never shipped. JSON, `formatVersion` 3.

| Key | What it holds | v1 |
| --- | --- | --- |
| `version` | `formatVersion` 3, the game and SKSE versions, a signature | write as read; new files take the sample's values (to check) |
| `actor` | `weight` 0–100, `hairColor` (a packed number), `headTexture` (`Plugin\|ID`) | edit |
| `headParts` | `type`, a load-order `formId`, and a `formIdentifier` `Plugin\|ID` | edit |
| `modNames`, `mods` | plugins the preset names; `mods` pairs a name with its load-order index | write from the load order of the save the preset is read with |
| `morphs.default` | `morphs`: the game's 19 face sliders (the 19th is 3.4e38 in the sample, probably "unset"); `presets`: nose, unused, eyes, lips (4294967295 = none) | edit |
| `morphs.custom` | RaceMenu sliders by name (`AB_LIP_Width` 0.31…), the morph names inside the head parts' chargen `.tri` files | edit (59 in the sample) |
| `morphs.sculpt` | per mesh (`host` = a chargen `.tri` path): `[vertex, dx, dy, dz]` as whole numbers ÷ `sculptDivisor` (10000) | keep as read |
| `tintInfo` | 108 layers: `index`, `color` (ARGB as one number), `texture` (the tint mask's path) | edit; 10 are in use in the sample |
| `faceTextures` | the face's diffuse, normal and subsurface paths | keep as read |
| `overrides` | overlay settings per node (`Face [Ovl0]`, `Hands [Ovl0]`…) | keep as read |
| `transforms` | bone scales per node (`NPC` 0.92, `NPC Head [Head]` 1.05) | keep as read |
| `bodyMorphs` | named body-morph values with the plugin that set them (75 in the sample, many of them XPMSE's own saved state) | keep as read |

**The preset has no race.** The player picks a race in game, then loads the
preset. So race and sex are only a "preview as" choice here, and the screen
says so.

## First version

1. **Open:** a `.jslot` from the installed mods or from disk; the race's
   default face; a save's player (from what Bodyworks already reads).
2. **Edit:** head parts (hair, eyes, brows, lashes, scars, beard), the 19
   face sliders and the 4 face presets, RaceMenu's named sliders, tint
   layers, skin tone, hair colour, and weight.
3. **Preview:** the existing character build. The body is the one the player
   already wears. A `.jslot` stores `bodyMorphs`, not a BodySlide preset, so
   no body is built from a preset here.
4. **Save:** a `.jslot` into a mod (`output.write`, under
   `SKSE/Plugins/CharGen/Presets/`), or to disk.

Kept exactly as read, not editable in v1: `sculpt`, `overrides`, `transforms`,
`faceTextures`, `bodyMorphs`, and any key this reader doesn't know.

## The gate, before anything else

1. **Round trip:** every preset read and written back gives the same JSON,
   unknown keys included. Test on several Nexus presets from different
   RaceMenu versions and races, not just the one sample.
2. **In game:** the owner loads a preset written here, in RaceMenu, and it
   looks as it did on screen. Check one head part that comes from a mod.

## Risks

- **Matching head parts to the right record.** Each part carries a
  load-order `formId` and a `Plugin|ID`. Which one RaceMenu uses on load is
  unknown. Write both from the load order of the save the screen reads the
  preset with (the mod manager doesn't say which plugins are active, so a
  save's list is the only load order there is; see step 4). Test with a part from a
  mod and with a different load order.
- **The preview can't show everything yet.** Named RaceMenu sliders are
  drawn (step 3), but the sign rule isn't checked in game and sliders from
  archives aren't read. `sculpt` and `transforms` aren't drawn at first. Where the preview
  differs from the game, the screen says so rather than hiding it.
- **Tint layers are named by texture path** in the preset, but by the race's
  `TINI` number in Bodyworks. They need mapping through the race's record.
- **`ui/app.js` is already 4,728 lines.** The character screen goes in its
  own module.

## Order of work

1. A `.jslot` reader and writer, and the round-trip test. **Done 15 Nov
   2026 (309e5c4): `crates/jslot`.**
   - The one sample writes back byte for byte.
   - 87 tests pass, including 3,000 generated documents per property and
     every truncation of a hand-built preset.
   - Limits: 4 MiB read, 64 MiB written, 32 levels deep.
   - Reviewed independently.
   - Still open, each needing more samples:
     - the one-line limit, which the sample only places between 26 and 80 bytes;
     - how RaceMenu writes non-ASCII text (raw or `\u`, UTF-8 or the Windows code page);
     - whether whole-valued floats are ever written as `1.0`;
     - the byte order of `color` and `hairColor`.
2. A `.jslot` turned into a `Look`, and a `Look` into a `.jslot`. **Done
   15 Nov 2026 (309e5c4): `crates/character/src/preset.rs`.**
   - `look_of_preset` reads a preset as a race and sex, with a report of
     everything it could not use; `new_document` starts a preset for a race.
   - `apply_look` writes a look back. It touches only what differs from
     the look the preset reads as, so unreadable values, missing sections
     and unknown keys stay as they were. It returns a report of what it
     changed, added or didn't write.
   - `modNames`, `mods` and named sliders are only added to, never taken
     out: to remove a slider, the screen sets it to 0.
   - The skin tone is written into the skin tone layer at full strength.
   - Reports list 200 notes at most, then how many more. Edits that can't
     be saved (a slider taken out, an unreadable value left as it was) are
     noted. When head parts are written, `mods` indexes follow the current
     load order.
   - The sample, read and written back, is unchanged byte for byte. Under
     another load order an unedited look writes nothing; edited parts take
     the new indexes.
   - Found from the sample and `UBE_AllRace.esp`: a tint layer's `index` is
     its mask's place in the race's list for the sex, not its `TINI` (99 of
     99 matched layers; `SkinTone.dds` is index 0, `TINI` 0x18). Nine of the
     sample's layers are not masks of the installed UBE races.
   - A head part's `type` is not its `PNAM` (the UBE face, `PNAM` 1, is
     type 0). It is kept as read; a new part takes the type of the part of
     the same `PNAM` it replaces, else its `PNAM`, which is a guess.
   - Still open: what `type` is; whether `hairColor` is drawn as a `CLFM`'s
     colour (twice it); which keys RaceMenu needs in a new preset.
3. Named RaceMenu sliders in the preview. **Done 15 Nov 2026
   (309e5c4): `crates/character/src/racemenu.rs`, `plan::plan_with`.**
   - Worked out from the ini files mods ship under
     `Meshes\Actors\Character\FaceGenMorphs\<plugin>\` (`morphs.ini`,
     `races.ini`, `sliders\*.ini`), never from RaceMenu's scripts or source.
     Only the loaded plugins' folders are read.
   - A `Slider` below nought turns on its first morph at minus the value,
     above nought its second; a `Preset` at *n* turns on `<prefix><n>` at 1
     (0 is none); `HeadPart` sliders aren't drawn. Morphs on one name add
     up, whatever the order. **Not checked in game.** The slider files'
     header writes the data as "UpperBound,LowerBound", but its own example
     and every installed line read low then high (`FACE_WidthIn,
     FACE_WidthOut`), as do the sample's signs.
   - The morphs are added to the chargen weights; a part whose chargen tri
     has `extension` tris takes them after its own, but a name its chargen
     tri (or an earlier extra) already applied is left to that one.
   - A name given twice in a slider file keeps its first (`EXPR2_Sad` in
     `JohnWick.ini`); so does a slider given by two plugins' folders.
   - Unknown sliders, `HeadPart` sliders, preset values out of range and
     morphs no part has are reported, not dropped. A look with no named
     sliders builds byte for byte as before.
   - On the owner's game: the sample's 59 sliders all resolve for each of
     the 16 `00UBE_*` races `races.ini` lists, and move the UBE head up to
     2.5 units (mean 0.34). Two extras UBE's `morphs.ini` names aren't
     installed (`ear_femalebrows.tri`, `nose_eyefemalelashes.tri`).
   - Expressive Facegen Morphs: 75 of its 91 tris have fewer places in the
     texture than vertices, which the tri reader refused. It now counts them
     apart (the layout then accounts for every byte of all 154 installed
     extra tris), and all read. Its `EFM_Nose_Depth` at 1 and
     `EFM_Eyes_Size` at -1 move a vanilla Nord woman's head up to 0.23
     units and her eyes up to 0.16.
   - `RaceMenuMorphs::needed` gives only files not yet fetched; a host gives
     a file that isn't there as no bytes, and stops when nothing is asked.
     Paths and plugin names that could leave their folder are refused and
     noted. Folders hold at most 16,384 sliders in all, and reading them is
     linear in their size.
   - Loose files only: Bodyworks has no archive reader, so RaceMenu's own
     `racemenu.esp` folder inside `RaceMenu.bsa` is not read yet.
4. The screen: open, edit, save. **Built 15 Nov 2026:
   `ui/character.js`, `ui/face.js`, `crates/app/src/preset.rs`.**
   - A third tool beside Build and Studio, Character, laid out as the
     Studio is: head and makeup on the left, the view, the sliders on the
     right, and BodySlide's bar at the foot (the mod saves go into, To
     disk…, Save). It is made of the other screens' parts: their pickers,
     sliders, categories, colour swatches, switches, sheets and view.
   - **Load order.** The mod manager says which files the mods have but
     not which plugins are active or in what order, so a preset is read
     with the plugins of a save the person picks (the save sheet the
     Studio uses, newest first). Its full plugins then its light ones are
     the load order the preset's forms are numbered by, and written
     back with. The order is the save's as it was when the game saved it:
     plugins added, taken out or moved since aren't seen. With no saves the
     screen says one is needed. A `plugins.list` request from the mod
     manager would replace this; that is the owner's call. Decided by the
     advisor, 15 Nov 2026.
   - **Open:** a `.jslot` the mods have under
     `SKSE/Plugins/CharGen/Presets/`, or one from disk. It is shown as the
     save's character's race and sex; a Race picker and Female/Male change
     what it is shown as, and the view says race and sex are only for the
     preview. Read as another race, the edits are carried over except the
     tint layers and skin tone, which belong to the race, and the screen
     says they were let go. Everything `look_of_preset` couldn't use, and
     everything the build left out, is listed under Notes… at the foot.
   - **Edit:** hair, eyes, brows, facial hair and scars, each from the
     playable head parts of its kind for the sex (UBE's lashes are facial
     hair, `PNAM` 4; the races a part is for are a form list, not read, so
     the list isn't narrowed to the race), or the race's default. A part
     whose kind isn't known (its plugin isn't in the save's, is in it but
     isn't installed, or holds no such head part) is in no slot: it is
     listed under Not drawn by its plugin, saying which of the three, and
     kept unless unticked. Picking a part for a slot replaces any part of
     that kind, but not these, as their kind isn't known; the list says to
     untick any it replaces. Hair colour and skin tone are colour swatches; the skin
     tone sets the skin tone mask's layer at full strength. Weight is a
     slider. The makeup tab lists the race's masks, in use and not, each
     with a colour and a strength. The right-hand list has the game's 18
     face sliders (the 19th is kept as read), the nose, eyes and lips
     presets, and RaceMenu's sliders for the race and sex, then any the
     preset has that the race's slider files don't. Every change makes a
     new look and the look as read is never changed, so a slider's
     double-click goes back to the file's value. A named slider the preset
     hadn't got is written only while it isn't 0, and its double-click
     takes it out again. A makeup layer not in use starts from the colour
     the preset keeps for it, and one put back to no strength is written
     as it was. Each slider's tooltip
     names the morph it turns on below and above 0, as the module has it:
     the sign rule for RaceMenu's sliders is kept in one place,
     `SliderKind::morph_at`, and is still not checked in game. A slider's
     track is hundredths, -100 to 100, wider when a value is beyond.
     `HeadPart` sliders and values JSON can't hold are shown as kept.
   - **Kept as read:** sculpting, overlays, bone scales, face textures and
     body morphs are listed with their counts and written back as read.
   - **Preview:** the existing character build with the save's plugins,
     the body the race's skin gives as installed; it is built again when
     a slider or colour is let go.
   - **Save:** `apply_look` into the preset as read, then into the mod
     builds go into at `SKSE/Plugins/CharGen/Presets/<name>.jslot`
     (`output.write`), or to disk; "What was written…" lists the report.
     Into the mods the name offered is `<name> (edited)`, not the preset's
     own, and a name the mods already have (`files.list`) is asked about
     before it is replaced; when the list can't be had, saving is asked
     about too, never done unasked. Names Windows refuses (control characters, a
     dot or space at the end, CON, PRN, AUX, NUL, COM1 to COM9, LPT1 to
     LPT9) are refused. A change after saving, or another mod picked, makes
     the button Save again.
   - **Changes not saved:** opening another preset asks first, as the
     Studio does. Picking another save carries the changes over; the tint
     layers and skin tone too when the preset is shown as the same race
     and sex, else they are let go, as they belong to the race, and the
     screen says so until the preset is read again. When the new save's
     character's race can't be shown, the race it was shown as is used; a
     save that can't be read, or whose race fails both ways, leaves the
     last one read again. The preview takes the place of the Studio's
     project, so a Studio project with changes not saved is asked about
     first, once: cancelled, the preview waits, and the view says so,
     until a preset or save is opened again. The preview itself isn't
     counted as changes. An engine
     restart says the preset and its changes went with it.
   - Still owed by the owner in game, and not guessed here: the slider
     sign direction, whether RaceMenu resolves a head part by
     `formIdentifier` or `formId`, and how `hairColor` is applied (drawn
     here as a hair colour record's, twice the colour).
5. The in-game check by the owner. Load a preset saved here after the load
   order has changed since the save it was read with, too.

## Wanted

- More sample presets: other races, male, made with older and newer RaceMenu
  versions, and some using parts from mods.
- One the owner saves in game from their own character, to compare with what
  Bodyworks reads from the same character's save.
