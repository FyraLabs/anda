import { andaAPI } from "./client";

export interface Project {
  id: string;
  name: string;
  description: null | string;
}

export const getAllProjects = () => andaAPI<Project[]>("/projects");
export const getProject = (id: string) => andaAPI<Project>(`/projects/${id}`);
