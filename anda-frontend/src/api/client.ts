import { $fetch } from "ohmyfetch"

export const APIUrl = 'http://127.0.0.1:8000'

export const andaAPI = $fetch.create({baseURL: APIUrl})