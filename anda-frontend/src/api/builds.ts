import { andaAPI } from "./client";


export interface Build {
    id:         string;
    target_id:  string;
    project_id: null | string;
    compose_id: null | string;
    status:     string;
    timestamp:  string;
    build_type: string;
}

export const getAllBuilds = () => andaAPI<Build[]>("/builds");
export const getBuild = (id: string) => andaAPI<Build>(`/builds/${id}`);
