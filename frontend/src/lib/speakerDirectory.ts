import { invoke } from '@tauri-apps/api/core';

export type DirectorySpeaker = {
  id: string;
  fullName: string;
  title: string;
  department: string;
};

export function foldVietnamese(value: string): string {
  return value
    .normalize('NFD')
    .replace(/\p{M}/gu, '')
    .replace(/đ/gi, 'd')
    .toLowerCase()
    .trim();
}

export function createDirectorySpeaker(input: {
  fullName: string;
  title?: string;
  department?: string;
  id?: string;
}): DirectorySpeaker | null {
  const fullName = input.fullName.trim();
  if (!fullName) return null;
  return {
    id: input.id?.trim() || crypto.randomUUID(),
    fullName,
    title: (input.title ?? '').trim(),
    department: (input.department ?? '').trim(),
  };
}

export function formatDirectorySpeakerLabel(person: DirectorySpeaker): string {
  return [person.fullName, person.title, person.department]
    .map((part) => part.trim())
    .filter(Boolean)
    .join(' - ');
}

export function suggestDirectorySpeakers(
  query: string,
  people: DirectorySpeaker[],
  limit = 8,
): DirectorySpeaker[] {
  const needle = foldVietnamese(query);
  const matches = needle
    ? people.filter((person) => {
        const haystack = foldVietnamese(
          `${person.fullName} ${person.title} ${person.department}`,
        );
        return haystack.includes(needle);
      })
    : people;
  return matches.slice(0, limit);
}

/** When renaming a diarization label that is not in the directory, show the full list. */
export function directoryFilterQuery(
  typed: string,
  originalLabel: string,
  people: DirectorySpeaker[],
): string {
  if (typed === originalLabel && suggestDirectorySpeakers(typed, people, 1).length === 0) {
    return '';
  }
  return typed;
}

export async function getSpeakerDirectory(): Promise<DirectorySpeaker[]> {
  const raw = await invoke<DirectorySpeaker[]>('get_speaker_directory');
  return Array.isArray(raw) ? raw : [];
}

export async function saveSpeakerDirectory(
  people: DirectorySpeaker[],
): Promise<DirectorySpeaker[]> {
  return invoke<DirectorySpeaker[]>('save_speaker_directory', { people });
}
