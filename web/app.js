// Nightglow's page: loads the WebAssembly game, feeds it input every frame, copies its pixels to the canvas,
// plays its sounds and shows the menus and the HUD. The game itself (simulation and lighting) is nightglow.wasm.

const $ = (id) => document.getElementById(id);
const canvas = $('screen');
const context = canvas.getContext('2d', {alpha: false});

const EVENT = {pickup: 1, hit: 2, dash: 4, night: 8, over: 16};
const STATE = {title: 0, playing: 1, over: 2};
const BEST_KEY = 'nightglow:best';
const SOUND_KEY = 'nightglow:sound';

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

// ------------------------------------------------------------ Sound (made with Web Audio, no files)

let audio = null;
let soundOn = read(SOUND_KEY) !== 'off';

function ensureAudio() {
  if (!audio && soundOn) {
    const Context = window.AudioContext ?? window.webkitAudioContext;
    if (Context) audio = new Context();
  }
  if (audio?.state === 'suspended') void audio.resume();
}

function tone(frequency, start, duration, volume, type = 'sine', slideTo = null) {
  if (!audio || !soundOn) return;
  const t = audio.currentTime + start;
  const osc = audio.createOscillator();
  const gain = audio.createGain();
  osc.type = type;
  osc.frequency.setValueAtTime(frequency, t);
  if (slideTo) osc.frequency.exponentialRampToValueAtTime(slideTo, t + duration);
  gain.gain.setValueAtTime(0.0001, t);
  gain.gain.exponentialRampToValueAtTime(volume, t + 0.01);
  gain.gain.exponentialRampToValueAtTime(0.0001, t + duration);
  osc.connect(gain).connect(audio.destination);
  osc.start(t);
  osc.stop(t + duration + 0.02);
}

const sounds = {
  pickup() {
    tone(880, 0, 0.12, 0.08);
    tone(1320, 0.06, 0.16, 0.07);
  },
  hit() {
    tone(160, 0, 0.35, 0.18, 'triangle', 55);
  },
  dash() {
    tone(300, 0, 0.18, 0.06, 'sawtooth', 900);
  },
  night() {
    tone(196, 0, 1.4, 0.07);
    tone(293.7, 0.05, 1.4, 0.05);
  },
  over() {
    tone(392, 0, 0.4, 0.08, 'triangle');
    tone(311, 0.25, 0.5, 0.08, 'triangle');
    tone(196, 0.55, 0.9, 0.09, 'triangle');
  },
};

function setSound(on) {
  soundOn = on;
  write(SOUND_KEY, on ? 'on' : 'off');
  const button = $('sound');
  button.setAttribute('aria-pressed', String(on));
  button.setAttribute('aria-label', on ? 'Sound on' : 'Sound off');
  if (on) ensureAudio();
}

// ------------------------------------------------------------ Size: a pixel buffer that fills the screen

function fit() {
  const width = window.innerWidth;
  const height = window.innerHeight;
  // Each game pixel is 2 to 6 screen pixels, so the scene keeps about the same detail on every screen.
  const pixel = Math.min(6, Math.max(2, Math.round(Math.min(width, height) / 200)));
  const w = Math.min(480, Math.max(160, Math.floor(width / pixel)));
  const h = Math.min(480, Math.max(120, Math.floor(height / pixel)));
  game.resize(w, h, 20251124);
  canvas.width = game.width();
  canvas.height = game.height();
  image = new ImageData(game.width(), game.height());
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
  if (event.code === 'Escape' || event.code === 'KeyP') {
    if (game.state() === STATE.playing) togglePause();
    return;
  }
  if (game.state() !== STATE.playing || paused) return;
  if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Space'].includes(event.code)) event.preventDefault();
  if ((event.code === 'Space' || event.code === 'ShiftLeft' || event.code === 'ShiftRight' || event.code === 'KeyJ') && !event.repeat) {
    dashQueued = true;
  }
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

// ------------------------------------------------------------ Screens

function show(panel) {
  for (const id of ['title', 'paused', 'over']) $(id).hidden = id !== panel;
  const playing = panel === null;
  $('hud').hidden = !(playing || panel === 'paused');
  $('dash').hidden = !playing || !matchMedia('(pointer: coarse)').matches;
  if (panel) $(panel).querySelector('button.primary')?.focus();
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
  ensureAudio();
  game.start((Math.random() * 0xffffffff) >>> 0 || 1);
  run += 1;
  overShown = false;
  setPaused(false);
  keys.clear();
  show(null);
  canvas.focus?.();
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
      // Let the light fade out first; a new run started meanwhile cancels this.
      const ended = run;
      setTimeout(() => {
        if (ended === run) showOver(false);
      }, 1200);
    }
    // The wasm memory can grow, so the view is made again each frame.
    const bytes = new Uint8ClampedArray(game.memory.buffer, game.frame_ptr(), game.width() * game.height() * 4);
    image.data.set(bytes);
    context.putImageData(image, 0, 0);
    if (game.state() === STATE.playing) updateHud();
  }
  requestAnimationFrame(frame);
}

// ------------------------------------------------------------ Start

async function load() {
  const url = new URL('nightglow.wasm', import.meta.url);
  let instance;
  try {
    ({instance} = await WebAssembly.instantiateStreaming(fetch(url), {}));
  } catch {
    // A server that sends another content type than application/wasm: fall back to bytes.
    const bytes = await (await fetch(url)).arrayBuffer();
    ({instance} = await WebAssembly.instantiate(bytes, {}));
  }
  return instance.exports;
}

async function main() {
  try {
    game = await load();
  } catch (error) {
    console.error(error);
    $('title').querySelector('.lede').textContent = 'Nightglow could not start: this browser cannot run WebAssembly.';
    $('play').disabled = true;
    return;
  }
  fit();
  window.addEventListener('resize', fit);
  setSound(soundOn);
  $('best-title').textContent = bestText();
  $('play').addEventListener('click', play);
  $('again').addEventListener('click', play);
  $('resume').addEventListener('click', togglePause);
  $('quit').addEventListener('click', endRun);
  $('pause').addEventListener('click', togglePause);
  $('sound').addEventListener('click', () => setSound(!soundOn));
  show('title');
  requestAnimationFrame(frame);
}

void main();
