// Nightglow's sounds: short tones made with Web Audio, no sound files.
// Browsers only let a page start audio from a user gesture, so the AudioContext is made by wakeAudio(), which the
// page calls from Play (and from the sound button), never on load.

let audio = null;
let enabled = true;

export function setSoundEnabled(on) {
  enabled = on;
}

/** Call from a click or key press: makes (or resumes) the AudioContext when sound is on. */
export function wakeAudio() {
  if (!audio && enabled) {
    const Context = window.AudioContext ?? window.webkitAudioContext;
    if (Context) audio = new Context();
  }
  if (audio?.state === 'suspended') void audio.resume();
}

function tone(frequency, start, duration, volume, type = 'sine', slideTo = null) {
  if (!audio || !enabled) return;
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

export const sounds = {
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
