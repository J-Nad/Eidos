export interface FileEntry {
  path: string;
  filename: string;
  extension: string;
  size: number;
  modifiedTimestamp: number;
  createdTimestamp: number;
  isDirectory: boolean;
  snippet: string;
  isContentMatch: boolean;
}

export interface DeepSearchResult {
  results: FileEntry[];
  source: 'instant' | 'ai' | 'fallback';
}

export interface FolderEntry {
  path: string;
  filename: string;
  extension: string;
  size: number;
  modifiedTimestamp: number;
  createdTimestamp: number;
  isDirectory: boolean;
}

export interface FileMetadata {
  path: string;
  filename: string;
  extension: string;
  size: number;
  modifiedTimestamp: number;
  isDirectory: boolean;
}

export interface IndexStats {
  filesAdded: number;
  filesRemoved: number;
  totalSize: number;
}

export interface FileTreeNode {
  id: string;
  name: string;
  path: string | null;
  source: 'local';
  kind: 'root' | 'folder' | 'file';
  children: FileTreeNode[];
}

export interface ChatMessage {
  id: string;
  role: 'assistant' | 'user' | 'system';
  content: string;
}

export type AgentMode = 'ask' | 'edit' | 'agent';

export interface ContextFile {
  path: string;
  filename: string;
}

export interface OpenSplitViewPayload {
  path: string;
  exists: boolean;
  isDirectory: boolean;
  aiAvailable: boolean;
}

export interface AiStreamEndPayload {
  streamId: string;
  content: string;
  cancelled: boolean;
}

export interface DriveEntry {
  path: string;
  label: string;
}

export interface Settings {
  selectedDrives: string[];
  defaultSaveLocation: string;
  autostart: boolean;
  geminiApiKey: string;
}

export type PreviewKind = 'text' | 'image' | 'pdf' | 'audio' | 'video' | 'folder' | 'binary' | 'missing';

export type FileKind = 'document' | 'spreadsheet' | 'image' | 'video' | 'audio' | 'code' | 'archive' | 'folder';

export interface MetadataFilters {
  extensions?: string[];
  kind?: FileKind | null;
  sizeMin?: number | null;
  sizeMax?: number | null;
  modifiedAfter?: number | null;
  modifiedBefore?: number | null;
  createdAfter?: number | null;
  createdBefore?: number | null;
  nameQuery?: string | null;
  hasContentIntent?: boolean;
}
