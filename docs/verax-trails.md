# VeraX custom trails

Build with `--features verax` to give a TechnoType or a projectile its own
trails: several per type, each anchored at a body-space FLH offset, with its own
colour gradient, width, fade time, opacity and draw order. This is VERA20K's own
extension, not a Phobos or Ares feature: a stock build reads none of these keys
and a `verax` build leaves every stock path unchanged until an ART section sets
a list.

```ini
; art.ini — the same file that carries UseLineTrail
[FLAC]
VeraTrail.Types=WING_L,WING_R,EXHAUST

[DRAGON]
VeraTrail.Types=PROJ_CORE

[VeraTrailTypes]
0=WING_L
1=WING_R
2=EXHAUST
3=PROJ_CORE

[WING_L]
FLH=60,-120,40      ; forward, lateral, height — leptons, body space
Color=255,196,64    ; head colour
Color2=255,64,0     ; tail colour; absent keeps one flat colour
Width=2             ; logical pixels at zoom 1, 1..8
FadeDuration=700    ; milliseconds a sample takes to reach zero strength
SegmentLength=0     ; smallest anchor movement that appends a sample, leptons
Alpha=220           ; head opacity, 0..255
ZOrder=0            ; lower draws first (behind)
```

## Keys

| Key | Where | Meaning |
| --- | --- | --- |
| `VeraTrail.Types=` | a type's ART section | Comma list of catalog names this type draws. Up to 12 resolved trails; unknown names are skipped, never fatal |
| `[VeraTrailTypes]` | ART | Ordered catalog: each key's value names a section that holds one style |
| `FLH=` | catalog entry | Body-space offset through the native `GetFLH` transform. Ignored on a projectile, whose anchor is the bullet coordinate itself |
| `Color=` / `Color2=` | catalog entry | Head colour and optional tail colour; a segment's colour interpolates by its older sample's age. A malformed or absent colour keeps the previous value |
| `Width=` | catalog entry | Line width in logical pixels, clamped to 1–8. One pixel row each |
| `FadeDuration=` | catalog entry | A sample lives this long, in milliseconds, then is dropped; clamped to 1–4000 |
| `SegmentLength=` | catalog entry | Minimum anchor movement that appends a sample, clamped to 0–2000. `0` appends on any movement, like the retail trail |
| `Alpha=` | catalog entry | Head strength, clamped to 0–255. Strength falls linearly with each sample's age |
| `ZOrder=` | catalog entry | Draw order; equal values keep catalog order |

Colours take the native `ReadColorRGB` three-byte form: three comma-separated
channels from 0 to 255. A non-numeric value keeps the colour already in effect
and an incomplete triple keeps the reader's default; an out-of-range number wraps
to its low byte like the native reader. `Width=` and `Alpha=` are clamped, so a
typo cannot produce a 300-pixel line or a negative opacity.

## Anchor and pose

A TechnoType's anchor is the object's render coordinate plus its FLH, transformed
by the one `GetFLH` transform the fire path uses
(`util::flh_transform::native_flh_world_delta`). A Fly locomotor's drawing matrix
supplies pitch and roll, so a wing-tip or exhaust trail follows a banking jet;
Drive, Ship and Hover hulls use their ground matrices. The lateral offset is never
mirrored by burst parity: a trail is not a shot.

A projectile has no body frame. Its list ignores `FLH=` and anchors on the bullet
coordinate, exactly where the retail `UseLineTrail` ring anchors.

## Sampling, length and lifetime

- One sample is appended per composite when the anchor moved at least
  `SegmentLength=` from the newest retained sample. A stationary owner appends
  nothing and its old samples age out.
- Aging uses wall-clock milliseconds, so a trail keeps the length its
  `FadeDuration=` promised whatever the display rate is. A sample older than the
  fade is dropped.
- An owner that disappears (death, limbo, a projectile impact) stops being
  sampled; its remaining samples fade out and the trail is retired. Loading a
  scenario clears every trail.
- `DetailLevel=0` narrows each trail to one pixel and drops the colour gradient;
  the samples themselves still age normally.

## Example: Allied Intruder

`tests/fixtures/ini/verax_trails_art_example.ini` is the exact snippet the
regression test reads: two wing-tip trails at `±90` leptons of lateral offset and
one exhaust trail `80` leptons behind the hull. Rules type `[ORCA]` declares
`Image=FALC`, so the list lives in ART section `[FALC]`; FALC is a voxel
aircraft, so the wing anchors follow its Fly pitch and roll. Paste those sections
into `artmd.ini` (the ART document the game folder reads) and build with
`--features verax` to see it in play.

## Drawing

Each segment becomes one `SurfaceLine` per pixel row of its width: rows are
shifted along the segment's normal, direction-independent. Strength is the head
`Alpha=` minus the segment's newer-sample age; segments below the retail A gate
(8) are skipped. Custom trails draw after the retail `LineTrail` phase and before
RadBeam, in `ZOrder=` order. A segment whose ends are concealed by translucent
fog is not drawn.

## A projectile with a custom list drops its retail trail

A projectile that sets `VeraTrail.Types=` keeps its `UseLineTrail` key in the ART
file but does not draw the retail ring: the custom list replaces it rather than
drawing beside it. Delete the list to get the retail trail back.

## Where VERA20K owns it

- `rules/vera_trail.rs` — catalog reading, clamping and list resolution.
- `rules/art_data.rs` — `VeraTrail.Types=` on an ART section and
  `ArtRegistry::vera_trail_styles`.
- `rules/ruleset.rs` — projection onto TechnoTypes (`vera_trails_for_object`) and
  projectiles at ART install.
- `sim/projectile.rs` — the retail-trail replacement rule.
- `sim/combat/fire_coord.rs` — `flh_anchor`, the shared `GetFLH` anchor.
- `app/presentation/vera_trails.rs` — sampling, aging, gradient, width and
  ordering; `app/presentation/render/mod.rs` supplies anchors per composite.

## Residuals

- Only TechnoTypes and projectiles carry lists. AttachEffect-attached trails and
  per-instance runtime toggles are not implemented.
- A trail's samples are presentation state. Loading, a new match or a type change
  never restores them; two clients at different display rates can show slightly
  different trail histories.
- `FLH=` is read from the catalog, not from a per-object anchor, so every object
  of a type shares one offset.
- With render interpolation enabled the hull draws interpolated, but trail
  anchors read the committed tick pose, so a trail steps at the simulation rate.
- Every trail-bearing object is sampled and rasterised each composite. There is
  no distance or budget LOD beyond `DetailLevel=0`, so a map that lists trails on
  thousands of types at once pays for all of them.
