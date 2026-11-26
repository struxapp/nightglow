# Nightglow

Nightglow is a small arcade game. Your light fades in the dark: catch fireflies to keep it burning, and keep the
shadow wisps away. They hunt you, but they fear a bright light, so the brighter you burn, the safer you are.
Every 25 seconds a new night begins: more wisps, faster wisps, and a light that fades a little quicker.

- **Move** with the arrows or WASD, or drag anywhere on a touch screen.
- **Dash** with Space, Shift or J, or the Dash button on a touch screen: a burst of speed that costs a little light.
- **Pause** with Esc or P, or the pause button. The game pauses itself when you switch tabs.
- Score: 10 points a second, 50 a firefly. Your best score and the sound setting stay in this browser
  (localStorage keys `nightglow:best` and `nightglow:sound`). No accounts, no tracking, no network calls after the
  page has loaded.

Screenshots: [docs/screenshot-dark.png](docs/screenshot-dark.png)

## Accessibility

Nightglow is a fast visual game and cannot be played with a screen reader. The menus, buttons and the light meter
are labelled and work with the keyboard. With `prefers-reduced-motion` set, there is no screen shake on a hit, the
lights hold still instead of flickering, and the player dims instead of blinking while it can't be hit.

## WebAssembly

The whole game, simulation and rendering, is Rust compiled to WebAssembly (`src/`): every frame it lights the
meadow pixel by pixel from your light, the fireflies and the sparks, then draws the wisps on top. The page
(`web/app.js`) only passes input in, copies the pixels to a canvas and plays the sounds (`web/sound.js`, Web Audio,
no sound files). The module has no imports and is about 72 KB; it needs no special headers (no threads, no
SharedArrayBuffer).

## Run it

`web/` is the folder that is served (`strux.json`, `"output": "web"`), and `web/nightglow.wasm` is committed, so
the game runs as it is from any static server:

```sh
npx serve web
```

## Test, lint and build

You need Rust with the WebAssembly target, clippy and rustfmt, and Node.js for the page's linter:

```sh
rustup target add wasm32-unknown-unknown
rustup component add clippy rustfmt

cargo test          # the game's tests
./build.sh lint     # cargo fmt --check, cargo clippy -D warnings, oxlint on web/
./build.sh          # the linters, the tests, then builds and copies web/nightglow.wasm
```

oxlint runs through `npx` at a pinned version, so the repository needs no `package.json`. Commit the new
`web/nightglow.wasm` together with the Rust change that made it.

## Code map

- `src/lib.rs`: the functions the page calls (`resize`, `start`, `tick`, `end_run`, `frame_ptr`, small getters) and
  the one table of numbers the page shares with the game (states and sound events).
- `src/game.rs`: the simulation (player, fireflies, wisps, nights, score) and its tests (`cargo test`).
- `src/game/render.rs`: the software renderer: the meadow, the lighting and the sprites.
- `web/app.js`: loading, input (keyboard, pointer and touch), the frame loop, menus and the HUD.
- `web/sound.js`: the sounds.
- `icon.svg` is the app's listing icon; `web/favicon.svg` is the same image, inside the served folder, as the page's
  favicon.

## Licences

Nightglow is MIT licensed, see [LICENSE](LICENSE). It ships no third-party code, fonts or images: the page uses the
system font, and its icons are a few inline SVG paths.
