import { $fetch } from "ohmyfetch";

export const APIUrl = "http://100.70.196.113:8000";

export const andaAPI = $fetch.create({ baseURL: APIUrl });
