import { andaAPI } from "./client";

import { Artifact } from "./artifacts";
export interface Project {
  id: string;
  name: string;
  description: null | string;
  summary: null | string;
}

export function getAllProjects() {
  return andaAPI<Project[]>("/projects");
}

export function getAllProjectsPaginated(limit: number, page: number) {
  return andaAPI<Project[]>(`/projects?limit=${limit}&page=${page}`);
}

// overrides for pagination
//export const getAllProjects = (limit: number, page: number) => andaAPI<Project[]>("/projects");
export const getProject = (id: string) => andaAPI<Project>(`/projects/${id}`);

export const getArtifactsOfProject = (id: string) => andaAPI<Artifact[]>(`/projects/${id}/artifacts`);
