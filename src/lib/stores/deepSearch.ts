import { writable } from 'svelte/store';
import type { FileEntry } from '$lib/types';

export interface DeepSearchState {
  query: string;
  results: FileEntry[];
  loading: boolean;
  source: 'instant' | 'ai' | 'fallback' | null;
  error: string | null;
}

export const deepSearchState = writable<DeepSearchState>({
  query: '',
  results: [],
  loading: false,
  source: null,
  error: null
});
