import { andaAPI } from "./client";


export interface Artifact {
    id:        string;
    filename:  string;
    path:      string;
    url:       string;
    build_id:  string;
    timestamp: string;
}


export const getAllArtifacts = () => andaAPI<Artifact[]>("/artifacts");
export const getArtifact = (id: string) => andaAPI<Artifact>(`/artifacts/${id}`);
