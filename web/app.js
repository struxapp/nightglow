// Nightglow's page: loads the WebAssembly game, feeds it input every frame, copies its pixels to the canvas,
// plays its sounds and shows the menus and the HUD. The game itself (simulation and lighting) is nightglow.wasm.

import {setSoundEnabled, sounds, wakeAudio} from './sound.js';

const $ = (id) => document.getElementById(id);
const canvas = $('screen');
const context = canvas.getContext('2d', {alpha: false});

// The numbers the module uses for its state and its event bits: the table in src/lib.rs (a Rust test pins them).
const STATE = {title: 0, playing: 1, over: 2};
const EVENT = {pickup: 1, hit: 2, dash: 4, night: 8, over: 16};

const BEST_KEY = 'nightglow:best';
const SOUND_KEY = 'nightglow:sound';
// The meadow looks the same on every visit.
const GROUND_SEED = 20251124;
const MOVE_KEYS = new Set(['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'KeyW', 'KeyA', 'KeyS', 'KeyD']);
const DASH_KEYS = new Set(['Space', 'ShiftLeft', 'ShiftRight', 'KeyJ']);

let game;
let image;
let paused = false;
let last = performance.now();
let dashQueued = false;
// Counts runs, so a late game-over timer from an earlier run is ignored.
let run = 0;
let overShown = false;
const keys = new Set();
const stick = {active: false, id: -1, x0: 0, y0: 0, x: 0, y: 0};

// ------------------------------------------------------------ Storage (may be blocked)

function read(key) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // The game still works; it just forgets.
  }
}

const best = () => Number(read(BEST_KEY) ?? 0) || 0;

// ------------------------------------------------------------ Sound

let soundOn = read(SOUND_KEY) !== 'off';

function setSound(on) {
  soundOn = on;
  setSoundEnabled(on);
  const button = $('sound');
  button.setAttribute('aria-pressed', String(on));
  button.setAttribute('aria-label', on ? 'Sound on' : 'Sound off');
}

// ------------------------------------------------------------ Size: a pixel buffer that fills the screen

function fit() {
  const width = window.innerWidth;
  const height = window.innerHeight;
  // Each game pixel is 2 to 6 screen pixels, so the scene keeps about the same detail on every screen.
  const pixel = Math.min(6, Math.max(2, Math.round(Math.min(width, height) / 200)));
  const w = Math.min(480, Math.max(160, Math.floor(width / pixel)));
  const h = Math.min(480, Math.max(120, Math.floor(height / pixel)));
  // The module skips the work when the size is the same, and draws a frame at the new size straight away.
  game.resize(w, h, GROUND_SEED);
  if (!image || image.width !== game.width() || image.height !== game.height()) {
    canvas.width = game.width();
    canvas.height = game.height();
    image = new ImageData(game.width(), game.height());
  }
  // Paint now: resizing clears the canvas, and a paused game draws no frames.
  draw();
}

// Resize events come in bursts while a window is dragged: fit once per animation frame at most.
let fitQueued = false;
function queueFit() {
  if (fitQueued) return;
  fitQueued = true;
  requestAnimationFrame(() => {
    fitQueued = false;
    fit();
  });
}

function draw() {
  // The wasm memory can grow, so the view is made again each time.
  const bytes = new Uint8ClampedArray(game.memory.buffer, game.frame_ptr(), game.width() * game.height() * 4);
  image.data.set(bytes);
  context.putImageData(image, 0, 0);
}

// ------------------------------------------------------------ Input

function direction() {
  let x = 0;
  let y = 0;
  if (keys.has('ArrowLeft') || keys.has('KeyA')) x -= 1;
  if (keys.has('ArrowRight') || keys.has('KeyD')) x += 1;
  if (keys.has('ArrowUp') || keys.has('KeyW')) y -= 1;
  if (keys.has('ArrowDown') || keys.has('KeyS')) y += 1;
  if (stick.active) {
    const dx = stick.x - stick.x0;
    const dy = stick.y - stick.y0;
    const reach = 48;
    x += Math.max(-1, Math.min(1, dx / reach));
    y += Math.max(-1, Math.min(1, dy / reach));
  }
  return [x, y];
}

window.addEventListener('keydown', (event) => {
  if (!game) return;
  if (event.code === 'Escape' || event.code === 'KeyP') {
    // Holding the key must not flicker between paused and playing.
    if (!event.repeat && game.state() === STATE.playing) togglePause();
    return;
  }
  if (game.state() !== STATE.playing || paused) return;
  // Space and Enter on a focused button (sound, pause) press that button; they don't also dash.
  const onButton = event.target instanceof Element && event.target.closest('button');
  if (onButton && (event.code === 'Space' || event.code === 'Enter')) return;
  if (MOVE_KEYS.has(event.code) || DASH_KEYS.has(event.code)) event.preventDefault();
  if (DASH_KEYS.has(event.code) && !event.repeat) dashQueued = true;
  keys.add(event.code);
});
window.addEventListener('keyup', (event) => keys.delete(event.code));
window.addEventListener('blur', () => {
  keys.clear();
  stick.active = false;
  if (game?.state() === STATE.playing && !paused) togglePause();
});
document.addEventListener('visibilitychange', () => {
  if (document.hidden && game?.state() === STATE.playing && !paused) togglePause();
});

canvas.addEventListener('pointerdown', (event) => {
  if (game.state() !== STATE.playing || paused || stick.active) return;
  stick.active = true;
  stick.id = event.pointerId;
  stick.x0 = stick.x = event.clientX;
  stick.y0 = stick.y = event.clientY;
  canvas.setPointerCapture(event.pointerId);
});
canvas.addEventListener('pointermove', (event) => {
  if (!stick.active || event.pointerId !== stick.id) return;
  stick.x = event.clientX;
  stick.y = event.clientY;
});
const release = (event) => {
  if (event.pointerId === stick.id) stick.active = false;
};
canvas.addEventListener('pointerup', release);
canvas.addEventListener('pointercancel', release);

$('dash').addEventListener('pointerdown', (event) => {
  event.preventDefault();
  dashQueued = true;
});

// A touch screen, even on a laptop whose main pointer is a mouse or touchpad.
const hasTouch = () => matchMedia('(any-pointer: coarse)').matches || navigator.maxTouchPoints > 0;

// ------------------------------------------------------------ Screens

// `focus`: move the focus to the panel's main button. Not on page load, where a focus ring on Play would greet every
// visitor; Tab reaches Play first.
function show(panel, focus = true) {
  for (const id of ['title', 'paused', 'over']) $(id).hidden = id !== panel;
  const playing = panel === null;
  $('hud').hidden = !(playing || panel === 'paused');
  $('dash').hidden = !playing || !hasTouch();
  if (panel === 'title') $('best-title').textContent = bestText() || 'No best score yet';
  if (panel && focus) {
    $(panel).querySelector('button.primary')?.focus();
  } else if (!panel && document.activeElement instanceof HTMLElement && document.activeElement.closest('.panel')) {
    // Don't leave the focus on a button that was just hidden.
    document.activeElement.blur();
  }
}

function bestText() {
  const value = best();
  return value ? `Best: ${value.toLocaleString('en')}` : '';
}

function setPaused(value) {
  paused = value;
  $('pause').setAttribute('aria-label', paused ? 'Resume' : 'Pause');
  last = performance.now();
}

function play() {
  wakeAudio();
  game.start((Math.random() * 0xffffffff) >>> 0 || 1);
  run += 1;
  overShown = false;
  setPaused(false);
  keys.clear();
  dashQueued = false;
  show(null);
}

function togglePause() {
  setPaused(!paused);
  show(paused ? 'paused' : null);
}

// The player quit: the game itself stops (so nothing keeps running behind the panel), then the results show.
function endRun() {
  game.end_run();
  setPaused(false);
  showOver(true);
}

// Shows the results of the current run once, and saves the best score once.
function showOver(quit) {
  if (overShown) return;
  overShown = true;
  $('over-heading').textContent = quit ? 'Run ended' : 'Your light went out';
  const score = game.score();
  const previous = best();
  if (score > previous) write(BEST_KEY, String(score));
  $('final-score').textContent = score.toLocaleString('en');
  $('final-night').textContent = String(game.night());
  $('final-fireflies').textContent = String(game.fireflies_caught());
  const seconds = Math.floor(game.seconds());
  $('final-time').textContent = `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
  $('best-over').textContent = score > previous && previous > 0 ? 'A new best!' : bestText();
  show('over');
}

let toastTimer = 0;
function toast(text) {
  const element = $('toast');
  element.textContent = text;
  element.classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => element.classList.remove('show'), 1800);
}

function updateHud() {
  const light = game.light();
  const meter = $('light-meter');
  $('light-fill').style.width = `${light}%`;
  meter.setAttribute('aria-valuenow', String(Math.round(light)));
  meter.classList.toggle('low', light < 25);
  $('hud-night').textContent = `Night ${game.night()}`;
  $('hud-score').textContent = game.score().toLocaleString('en');
  $('dash').style.setProperty('--ready', game.dash_ready().toFixed(2));
}

// ------------------------------------------------------------ Frame loop

function frame(now) {
  const dt = Math.min(50, now - last);
  last = now;
  if (!paused) {
    const [x, y] = direction();
    const events = game.tick(dt, x, y, dashQueued ? 1 : 0);
    dashQueued = false;
    if (events & EVENT.pickup) sounds.pickup();
    if (events & EVENT.hit) sounds.hit();
    if (events & EVENT.dash) sounds.dash();
    if (events & EVENT.night) {
      sounds.night();
      toast(`Night ${game.night()}`);
    }
    if (events & EVENT.over) {
      sounds.over();
      // No Dash button over the fading light.
      $('dash').hidden = true;
      // Let the light fade out first; a new run started meanwhile cancels this.
      const ended = run;
      setTimeout(() => {
        if (ended === run) showOver(false);
      }, 1200);
    }
    draw();
    if (game.state() === STATE.playing) updateHud();
  }
  requestAnimationFrame(frame);
}

// ------------------------------------------------------------ Start

class StartError extends Error {
  constructor(message, cause) {
    super(message, {cause});
    this.name = 'StartError';
  }
}

async function load() {
  const url = new URL('nightglow.wasm', import.meta.url);
  let bytes;
  try {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    bytes = await response.arrayBuffer();
  } catch (error) {
    throw new StartError('Nightglow could not load its game file. Check your connection and reload the page.', error);
  }
  try {
    const {instance} = await WebAssembly.instantiate(bytes, {});
    return instance.exports;
  } catch (error) {
    throw new StartError('Nightglow could not start: this browser cannot run WebAssembly.', error);
  }
}

async function main() {
  try {
    game = await load();
  } catch (error) {
    console.error(error);
    $('title').querySelector('.lede').textContent = error instanceof StartError ? error.message : String(error);
    $('play').disabled = true;
    return;
  }
  // No screen shake, flicker or blinking for players who asked for reduced motion.
  const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
  const applyMotion = () => game.set_reduced_motion(reducedMotion.matches ? 1 : 0);
  applyMotion();
  reducedMotion.addEventListener('change', applyMotion);
  fit();
  window.addEventListener('resize', queueFit);
  setSound(soundOn);
  $('play').addEventListener('click', play);
  $('again').addEventListener('click', play);
  $('resume').addEventListener('click', togglePause);
  $('quit').addEventListener('click', endRun);
  $('pause').addEventListener('click', togglePause);
  $('sound').addEventListener('click', () => {
    setSound(!soundOn);
    // A click is a user gesture, so audio may start here when sound was off at Play.
    if (soundOn) wakeAudio();
  });
  show('title', false);
  requestAnimationFrame(frame);
}

void main();
