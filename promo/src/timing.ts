// Music grid: track 2 at 115 BPM. Bar k of the video starts at K(k) seconds.
export const FPS = 30;
export const BAR = 2.0870056;
export const BEAT = BAR / 4;
export const K = (k: number) => 3 + k * BAR;
export const START = K(2); // opener -> main
export const END = K(16); // end card
export const DURATION = K(16) + 3.6; // ~40 s
export const FRAMES = Math.round(DURATION * FPS);
