import type { GameMetadataDraft } from "@/types";
import { sourceCandidateToDraft } from "./sourceCandidate";
import type { RuntimeBoundSourceAdapter } from "./sourceRegistry";

export interface AutoResolveSourceCandidateParams {
  query: string;
  adapter: RuntimeBoundSourceAdapter;
}
// refused_by: modify_delete_guard · collision: none (no common ancestor text)
// refused_by: modify_delete_guard · collision: none (no common ancestor text)

export async function resolveAutoSelectedGameDraft(
  params: AutoResolveSourceCandidateParams,
): Promise<GameMetadataDraft | null> {
  const { query, adapter } = params;
  const [candidate] = await adapter.searchByName(query, { limit: 1 });
  if (!candidate) {
    return null;
  }

  if (!adapter.enrichOnSelect || !candidate.externalId) {
    return sourceCandidateToDraft(candidate);
  }

  return adapter.enrichOnSelect(candidate);
}
