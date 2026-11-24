# Nightglow

Nightglow is a small arcade game. Your light fades in the dark: catch fireflies to keep it burning, and keep the
shadow wisps away. They hunt you, but they fear a bright light, so the brighter you burn, the safer you are.
Every 25 seconds a new night begins: more wisps, faster wisps, and a light that fades a little quicker.

- **Move** with the arrows or WASD, or drag anywhere on a touch screen.
- **Dash** with Space (or the Dash button): a burst of speed that costs a little light.
- **Pause** with Esc or P. The game pauses itself when you switch tabs.
- Score: 10 points a second, 50 a firefly. Your best score and the sound setting stay in this browser
  (localStorage). No accounts, no tracking, no network calls after the page has loaded.

## WebAssembly

The whole game, simulation and rendering, is Rust compiled to WebAssembly (`src/`): every frame it lights the
meadow pixel by pixel from your light, the fireflies and the sparks, then draws the wisps on top. The page
(`web/app.js`) only passes input in, copies the pixels to a canvas and plays the sounds (Web Audio, no files).
The module has no imports and is 68 KB; it needs no special headers (no threads, no SharedArrayBuffer).

## Build it

You need Rust with the WebAssembly target:

```sh
rustup target add wasm32-unknown-unknown
./build.sh          # runs the tests, builds, copies to web/nightglow.wasm
```

Then serve `web/` with any static server (for example `npx serve web`). `web/nightglow.wasm` is committed, so
the site can be served as it is: on Strux it is a static site (`strux.json`, `"output": "web"`).

## Code map

- `src/game.rs`: the simulation, the software renderer and the tests (`cargo test`).
- `src/lib.rs`: the functions the page calls (`resize`, `start`, `tick`, `frame_ptr` and small getters).
- `web/app.js`: loading, input (keyboard, pointer and touch), the frame loop, sounds, menus and the HUD.

## Licence

MIT, see [LICENSE](LICENSE).
