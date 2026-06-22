import { writable } from 'svelte/store';
import type { ChatMessage, FileMetadata, OpenSplitViewPayload } from '$lib/types';

export interface SplitViewState extends OpenSplitViewPayload {
  metadata: FileMetadata | null;
  content: string;
  aiAvailable: boolean;
  chatMessages: ChatMessage[];
  isGenerating: boolean;
  activeStreamId: string | null;
}

export const splitViewState = writable<SplitViewState>({
  path: '',
  exists: false,
  isDirectory: false,
  aiAvailable: false,
  metadata: null,
  content: '',
  chatMessages: [],
  isGenerating: false,
  activeStreamId: null
});

export const messageId = () => crypto.randomUUID();
