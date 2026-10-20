import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

// Licensed originals live outside Git; synthetic fixture coverage runs on every CI worker.
export const originalArtRoot = resolve(process.env.DRIICHI_ORIGINAL_ART_ROOT ?? '../../../../driichi-runtime/character-packs');
export const originalArtAvailable = ['reimu', 'marisa'].every(id =>
  ['portrait.webp', 'icon.webp'].every(file => existsSync(resolve(originalArtRoot, id, file))));
