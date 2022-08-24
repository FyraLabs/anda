import { andaAPI } from "./client";

import { APIUrl } from "./client";
export interface Artifact {
  id: string;
  filename: string;
  path: string;
  url: string;
  build_id: string;
  timestamp: string;
  metadata: ArtifactMeta;
}

export interface ArtifactMeta {
  art_type: string;
  rpm: RPMArtifact | null;
  file: FileArtifact | null;
}

export interface RPMArtifact {
    name: string;
    version: string;
    release: string | null;
    epoch: string | null;
    arch: string;
}


export interface FileArtifact {
    e_tag: string;
    filename: string;
    size: number;
}

export const getAllArtifacts = () => andaAPI<Artifact[]>("/artifacts");
export const getArtifact = (id: string) =>
  andaAPI<Artifact>(`/artifacts/${id}`);

export function getArtifactUrl(id: string) {
  return `${APIUrl}/artifacts/${id}/file`;
}
