import { invoke } from '@tauri-apps/api/core';

export type SpeakerHotkeys = Record<string, string>;

export function emptySpeakerHotkeys(): SpeakerHotkeys {
  const slots: SpeakerHotkeys = {};
  for (let i = 1; i <= 9; i += 1) {
    slots[String(i)] = '';
  }
  return slots;
}

export async function getSpeakerHotkeys(): Promise<SpeakerHotkeys> {
  const raw = await invoke<SpeakerHotkeys>('get_speaker_hotkeys');
  return { ...emptySpeakerHotkeys(), ...raw };
}

export async function saveSpeakerHotkeys(hotkeys: SpeakerHotkeys): Promise<SpeakerHotkeys> {
  return invoke<SpeakerHotkeys>('save_speaker_hotkeys', { hotkeys });
}

export async function insertLiveSpeaker(name: string): Promise<boolean> {
  return invoke<boolean>('insert_live_speaker', { name });
}
