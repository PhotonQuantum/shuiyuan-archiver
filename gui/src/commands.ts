import {invoke} from "@tauri-apps/api";
import {TopicMeta} from "./bindings";

export const openBrowser = async () => {
  await invoke<void>("open_browser");
}
export const tokenFromOauth = async (payload: string) => {
  return await invoke<string>("token_from_oauth", {payload});
}

export const loginWithToken = async (token: string) => {
  await invoke<void>("login_with_token", {token});
}

export const fetchMeta = async (topicId: number) => {
  console.log("on the fly", topicId);
  return await invoke<TopicMeta>("fetch_meta", {topicId});
}

export const archive = async (topicMeta: TopicMeta, saveTo: string, maskUser: boolean) => {
  await invoke<void>("archive", {topicMeta, saveTo, maskUser});
}

export const sanitize = async (s: string) => {
  return await invoke<string>("sanitize", {s});
}
