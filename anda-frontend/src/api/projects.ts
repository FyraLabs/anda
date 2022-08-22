import { andaAPI } from "./client";

import { Artifact } from "./artifacts";
export interface Project {
  id: string;
  name: string;
  description: null | string;
}

export const getAllProjects = () => andaAPI<Project[]>("/projects");
export const getProject = (id: string) => andaAPI<Project>(`/projects/${id}`);

export const getArtifactsOfProject = (id: string) => andaAPI<Artifact[]>(`/projects/${id}/artifacts`);