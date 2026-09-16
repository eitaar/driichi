export type VoiceKind = "chi" | "pon" | "kan" | "riichi" | "ron" | "tsumo";

export interface AudioSettings {
  master: number;
  sfx: number;
  voice: number;
  voiceEnabled: boolean;
}

export const DEFAULT_AUDIO_SETTINGS: AudioSettings = {
  master: 1,
  sfx: 1,
  voice: 1,
  voiceEnabled: true,
};

export const AUDIO_SETTINGS_KEY = "driichi:audio-settings";

function clamp(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : fallback;
}

function safeStorage(storage?: Storage | null): Storage | null {
  if (storage !== undefined) return storage;
  try { return typeof window !== "undefined" ? window.localStorage : null; } catch { return null; }
}

export function loadAudioSettings(storage?: Storage | null): AudioSettings {
  const target = safeStorage(storage);
  if (!target) return { ...DEFAULT_AUDIO_SETTINGS };
  try {
    const raw = target.getItem(AUDIO_SETTINGS_KEY);
    if (!raw) return { ...DEFAULT_AUDIO_SETTINGS };
    const value = JSON.parse(raw) as Partial<AudioSettings>;
    return {
      master: clamp(value.master, DEFAULT_AUDIO_SETTINGS.master),
      sfx: clamp(value.sfx, DEFAULT_AUDIO_SETTINGS.sfx),
      voice: clamp(value.voice, DEFAULT_AUDIO_SETTINGS.voice),
      voiceEnabled: typeof value.voiceEnabled === "boolean" ? value.voiceEnabled : DEFAULT_AUDIO_SETTINGS.voiceEnabled,
    };
  } catch {
    return { ...DEFAULT_AUDIO_SETTINGS };
  }
}

export function saveAudioSettings(settings: AudioSettings, storage?: Storage | null): void {
  const target = safeStorage(storage);
  if (!target) return;
  try {
    target.setItem(AUDIO_SETTINGS_KEY, JSON.stringify({
      master: clamp(settings.master, DEFAULT_AUDIO_SETTINGS.master),
      sfx: clamp(settings.sfx, DEFAULT_AUDIO_SETTINGS.sfx),
      voice: clamp(settings.voice, DEFAULT_AUDIO_SETTINGS.voice),
      voiceEnabled: Boolean(settings.voiceEnabled),
    }));
  } catch {
    // Private browsing and quota errors must never affect gameplay.
  }
}

export function voiceAssetPath(characterId: string, kind: VoiceKind): string {
  return `/assets/characters/${encodeURIComponent(characterId)}/voices/${kind === "kan" ? "kan" : kind}.ogg`;
}

export function voicePriority(kind: VoiceKind): number {
  return kind === "ron" || kind === "tsumo" ? 2 : 1;
}

interface VoiceRequest {
  characterId: string;
  kind: VoiceKind;
  order: number;
}

interface ActiveVoice {
  request: VoiceRequest;
  audio: HTMLAudioElement;
  timeout: number | undefined;
}

export interface AudioFactory {
  (source?: string): HTMLAudioElement;
}

/** Small, failure-tolerant voice queue for live table events. */
export class AudioManager {
  private settingsValue: AudioSettings;
  private readonly audioFactory: AudioFactory;
  private active: ActiveVoice | null = null;
  private queue: VoiceRequest[] = [];
  private order = 0;
  private destroyed = false;

  constructor(audioFactory?: AudioFactory, settings?: AudioSettings) {
    this.audioFactory = audioFactory ?? ((source?: string) => new Audio(source));
    this.settingsValue = settings ? { ...settings } : loadAudioSettings();
  }

  get settings(): AudioSettings {
    return { ...this.settingsValue };
  }

  setSettings(next: Partial<AudioSettings>): AudioSettings {
    this.settingsValue = {
      master: clamp(next.master, this.settingsValue.master),
      sfx: clamp(next.sfx, this.settingsValue.sfx),
      voice: clamp(next.voice, this.settingsValue.voice),
      voiceEnabled: typeof next.voiceEnabled === "boolean" ? next.voiceEnabled : this.settingsValue.voiceEnabled,
    };
    saveAudioSettings(this.settingsValue);
    if (this.active) this.active.audio.volume = this.volume;
    return this.settings;
  }

  get volume(): number {
    return this.settingsValue.master * this.settingsValue.voice;
  }

  async unlock(): Promise<void> {
    if (this.destroyed || typeof window === "undefined") return;
    try {
      const AudioContextConstructor = (window as Window & { AudioContext?: typeof AudioContext; webkitAudioContext?: typeof AudioContext }).AudioContext
        ?? (window as Window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
      if (AudioContextConstructor) {
        const context = new AudioContextConstructor();
        await context.resume().catch(() => undefined);
        void context.close?.().catch(() => undefined);
      }
    } catch {
      // AudioContext is optional and may be unavailable in an embedded browser.
    }
    try {
      const probe = this.audioFactory();
      probe.muted = true;
      const play = probe.play();
      if (play) await play.catch(() => undefined);
      probe.pause();
      probe.src = "";
    } catch {
      // Unlock failure is deliberately silent.
    }
  }

  playVoice(characterId: string | null | undefined, kind: VoiceKind): void {
    if (this.destroyed || !characterId || !this.settingsValue.voiceEnabled || this.volume <= 0) return;
    const request = { characterId, kind, order: this.order++ };
    if (!this.active) {
      this.start(request);
      return;
    }
    if (voicePriority(kind) > voicePriority(this.active.request.kind)) {
      this.stopActive();
      this.start(request);
      return;
    }
    this.queue.push(request);
  }

  /** Resolution-order events are passed in sequence and remain FIFO at equal priority. */
  playVoices(events: Array<{ characterId: string | null | undefined; kind: VoiceKind }>): void {
    events.forEach((event) => this.playVoice(event.characterId, event.kind));
  }

  stop(): void {
    this.queue = [];
    this.stopActive();
  }

  destroy(): void {
    this.destroyed = true;
    this.stop();
  }

  private start(request: VoiceRequest): void {
    if (this.destroyed) return;
    let audio: HTMLAudioElement | undefined;
    try {
      audio = this.audioFactory(voiceAssetPath(request.characterId, request.kind));
      audio.volume = this.volume;
      audio.preload = "auto";
      audio.onended = () => this.finished(audio!);
      const timeout = window.setTimeout(() => this.finished(audio!), 10_000);
      this.active = { request, audio, timeout };
      const play = audio.play();
      if (play) void play.catch(() => this.finished(audio!));
    } catch {
      if (audio) this.clearActive(audio);
      this.startNext(request);
    }
  }

  private finished(audio: HTMLAudioElement): void {
    if (this.active?.audio !== audio) return;
    const request = this.active.request;
    this.clearActive(audio);
    this.startNext(request);
  }

  private startNext(_previous: VoiceRequest): void {
    if (this.destroyed || this.active || this.queue.length === 0) return;
    const next = this.queue.shift();
    if (next) this.start(next);
  }

  private clearActive(audio: HTMLAudioElement): void {
    const active = this.active;
    if (!active || active.audio !== audio) return;
    if (active.timeout !== undefined) window.clearTimeout(active.timeout);
    try {
      audio.pause();
      audio.onended = null;
      audio.src = "";
    } catch {
      // A broken media element should not hold the queue open.
    }
    this.active = null;
  }

  private stopActive(): void {
    if (!this.active) return;
    const audio = this.active.audio;
    this.clearActive(audio);
  }
}

export const VoiceManager = AudioManager;
