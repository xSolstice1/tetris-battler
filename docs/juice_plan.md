# Juice Plan — making every action feel good

Goal: every input gets instant, readable feedback; big plays (quads, T-spins, combos,
perfect clears) feel like fireworks. Effects must never slow down play — this becomes a
competitive battler.

## Core principles

1. **Logic stays instant, animation is presentation only.** `game.rs` keeps clearing lines
   immediately (no line-clear delay). The renderer animates a snapshot of what was cleared
   over the already-updated board. Input never waits on an effect.
2. **Events, not polling.** The game emits a rich `Event` stream; an `Fx` layer turns events
   into particles, tweens, shake and sound. Game logic knows nothing about effects.
3. **Escalation.** Feedback scales with the play: single < double < triple < quad < T-spin;
   combo N sounds/looks bigger than combo N-1. That ladder is the "woohoo".
4. **Separate RNG for effects.** Particles use their own randomness so the game's seeded
   bag stays deterministic (replays/netcode later).
5. **Toggleable.** Screen shake, flashes and particle density get settings (comfort /
   photosensitivity).

## Architecture changes

### `game.rs` — replace `ClearEvent` with `Event`

```rust
pub enum Event {
    Moved { dx: i32 },
    Rotated { clockwise: bool, kicked: bool },
    SoftDropped,
    HardDropped { kind, rot, x, from_y, to_y },
    Locked { kind, cells: [(i32, i32); 4] },
    LinesCleared {
        rows: Vec<usize>,          // board row indices that were full (pre-clear)
        snapshot: Vec<Row>,        // their contents, for the dissolve animation
        lines, t_spin, back_to_back, combo, perfect_clear,
    },
    Held,
    Spawned { kind },
    LevelUp { level },
    GameOver,
}
```

Also expose `lock_progress() -> f32` (0..1) so the active piece can visibly "charge" before
locking.

### New `src/fx/` module

| File | Responsibility |
|------|----------------|
| `mod.rs` | `Fx` struct: `handle(&Event)`, `update(dt)`, draw hooks per layer |
| `particles.rs` | Pooled particles (pos, vel, gravity, drag, life, size, colour); `swap_remove`, cap ~2000 |
| `tween.rs` | Easing fns: `out_cubic`, `out_back` (overshoot pop), `out_elastic`, `in_quad` |
| `shake.rs` | Trauma-based shake: offset = max * trauma², trauma decays per second |
| `popup.rs` | Callout text (moved out of `render.rs`) with scale-pop and stacking |
| `audio.rs` | Procedurally synthesised SFX (no asset files) |

`render.rs` draws in layers: background → board (with collapse offsets) → ghost/active →
particles → callouts → overlays, all under the shake offset applied to `View`.

## Effects catalogue

### Tier 1 — core feel (do first)

| Effect | Trigger | Description |
|--------|---------|-------------|
| **Line clear dissolve** | `LinesCleared` | Rows flash white (~60 ms), then cells burst into particles from the centre column outward (~250 ms), coloured by their piece. |
| **Stack collapse** | `LinesCleared` | Rows above the gap are drawn offset upward by the number of rows cleared beneath them, easing to 0 with `out_back` (slight bounce) over ~180 ms. |
| **Hard drop** | `HardDropped` | Vertical light streaks from start to landing, dust puff at the impact edge, short downward shake scaled by drop distance. |
| **Lock flash** | `Locked` | Locked cells glow white and fade over ~120 ms. |
| **Lock charge** | `lock_progress()` | Active piece dims/pulses as lock delay runs out — readable "about to stick". |
| **Screen shake** | quads, T-spins, hard drops | Small for drops, big for quads/T-spin triples. |
| **Callout pop** | `LinesCleared` | Text scales 0 → 1.2 → 1.0 (`out_back`), colour per clear type, drifts up and fades. Combo counter grows with combo size. |

### Tier 2 — sound (biggest single satisfaction lever)

Needs macroquad's `audio` feature, which needs `libasound2-dev` on Linux
(`sudo apt install libasound2-dev`).

Synthesise SFX at startup (square/saw/noise + envelopes → WAV bytes → `load_sound_from_bytes`):
move tick, rotate blip, kick variant, soft-drop tick, hard-drop thud, lock click, hold whoosh,
line clear chime **pitched up per combo step** (rising scale = the classic dopamine hook),
quad/T-spin stinger, perfect-clear fanfare, level-up arpeggio, game-over descend.

Original, procedurally generated — no borrowed audio.

### Tier 3 — big moments

| Effect | Description |
|--------|-------------|
| **Perfect clear** | Shockwave ring from board centre, confetti burst, huge "PERFECT CLEAR". |
| **Level up** | Board border colour shifts per level, ring pulse, brief speed lines. |
| **T-spin tell** | Sparkle on the T's corners when a rotation satisfies the 3-corner rule. |
| **Combo heat** | Board border glows hotter with combo length; resets with a cool-down fade when the combo breaks. |
| **Danger state** | Stack reaches the top 4 rows: edges pulse red, low heartbeat. |
| **Hit-stop** | 60–80 ms visual freeze on quad / T-spin triple (visual only — logic keeps running). |

### Tier 4 — polish

| Effect | Description |
|--------|-------------|
| Spawn ease-in | New piece fades in and drops 4 px into place. |
| Rotation tween | Piece visually rotates from old orientation over ~50 ms (never delays logic). |
| Hold swap | Piece flies from field to hold box; held piece flies in. |
| Next queue slide | Queue slides up one slot when a piece is consumed. |
| Game over | Stack greys out row by row from the top, then blocks tumble off with physics. |
| Background | Slow drifting grid/particles that pulse on clears. |

### Battle-mode effects (when the versus layer lands)

Attack projectiles fly from the attacker's board to the defender's garbage meter; meter
fills with a shake; cancellation produces a clash spark mid-screen; garbage rows rise with
a rumble; KO explodes the loser's board.

## Phases

| Phase | Scope | Done when |
|-------|-------|-----------|
| 1 ✅ | `Event` enum + `Fx` skeleton + particles/tween/shake + line-clear dissolve + collapse + hard drop + lock flash + callout pop | Clearing a quad looks good with no added input latency |
| 2 | Procedural audio, combo pitch ladder | Every action has a sound; combos audibly climb |
| 3 | Perfect clear, level up, combo heat, danger, T-spin tell, hit-stop | Big plays clearly feel bigger than small ones |
| 4 | Spawn/rotation/hold/queue tweens, game over sequence, background | Nothing on screen "teleports" |
| 5 | Settings: shake/flash/particle toggles, volume | Effects can be dialled down |

## Testing

- Unit tests in `game.rs` for event emission: cleared row indices and snapshots are correct
  (including non-adjacent clears, e.g. rows 37 and 39); `HardDropped` distances; `LevelUp`
  fires once per threshold.
- Debug keys (debug builds only): F1–F4 load preset boards (quad-ready, T-spin-ready,
  perfect-clear-ready, near top-out) to trigger each effect on demand.
- Frame-time check: particle cap holds 60 fps in the wasm build.
