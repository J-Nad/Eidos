import { writable } from 'svelte/store';
import type { MetadataFilters } from '$lib/types';

export const emptyMetadataFilters: MetadataFilters = {
  extensions: [],
  kind: null,
  sizeMin: null,
  sizeMax: null,
  modifiedAfter: null,
  modifiedBefore: null,
  createdAfter: null,
  createdBefore: null,
  nameQuery: null,
  hasContentIntent: false
};

export const activeFilters = writable<MetadataFilters>({ ...emptyMetadataFilters, extensions: [] });

export function normalizeFilters(filters: MetadataFilters | null | undefined): MetadataFilters {
  return {
    extensions: [...(filters?.extensions ?? [])]
      .map((extension) => extension.trim().replace(/^\./, '').toLowerCase())
      .filter(Boolean),
    kind: filters?.kind ?? null,
    sizeMin: filters?.sizeMin ?? null,
    sizeMax: filters?.sizeMax ?? null,
    modifiedAfter: filters?.modifiedAfter ?? null,
    modifiedBefore: filters?.modifiedBefore ?? null,
    createdAfter: filters?.createdAfter ?? null,
    createdBefore: filters?.createdBefore ?? null,
    nameQuery: filters?.nameQuery?.trim() || null,
    hasContentIntent: filters?.hasContentIntent ?? false
  };
}

export function hasActiveFilters(filters: MetadataFilters | null | undefined): boolean {
  const normalized = normalizeFilters(filters);
  return Boolean(
    normalized.extensions?.length ||
      normalized.kind ||
      normalized.sizeMin ||
      normalized.sizeMax ||
      normalized.modifiedAfter ||
      normalized.modifiedBefore ||
      normalized.createdAfter ||
      normalized.createdBefore ||
      normalized.nameQuery
  );
}
