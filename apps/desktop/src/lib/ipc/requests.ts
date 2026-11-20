// The request files of this window's project. One wrapper per command.
// Paths are relative to the project root, with `/`.

import { invoke } from "@tauri-apps/api/core";
import type { FileRef } from "./bindings/FileRef";
import type { FolderFile } from "./bindings/FolderFile";
import type { Inspection } from "./bindings/Inspection";
import type { RequestFile } from "./bindings/RequestFile";
import type { RequestTreeInfo } from "./bindings/RequestTreeInfo";

export function requestTree(): Promise<RequestTreeInfo> {
  return invoke<RequestTreeInfo>("request_tree");
}

export function readRequest(path: string): Promise<RequestFile> {
  return invoke<RequestFile>("read_request", { path });
}

export function saveRequest(path: string, request: RequestFile): Promise<void> {
  return invoke<void>("save_request", { path, request });
}

/** The settings of a folder; `requests` holds the order of the collections. */
export function readFolder(path: string): Promise<FolderFile> {
  return invoke<FolderFile>("read_folder", { path });
}

export function saveFolder(path: string, settings: FolderFile): Promise<void> {
  return invoke<void>("save_folder", { path, settings });
}

/** Creates a request in `folder`; resolves to its path. */
export function createRequest(folder: string, name: string): Promise<string> {
  return invoke<string>("create_request", { folder, name });
}

/** Creates a folder in `parent` (`requests` for a collection); resolves to its path. */
export function createFolder(parent: string, name: string): Promise<string> {
  return invoke<string>("create_folder", { parent, name });
}

/** Resolves to the new path of the request or folder. */
export function renameRequestItem(path: string, name: string): Promise<string> {
  return invoke<string>("rename_request_item", { path, name });
}

/** Resolves to the path of the copy. */
export function duplicateRequestItem(path: string): Promise<string> {
  return invoke<string>("duplicate_request_item", { path });
}

/** Moves a request or folder into `folder` at `index`; resolves to its new path. */
export function moveRequestItem(
  path: string,
  folder: string,
  index: number,
): Promise<string> {
  return invoke<string>("move_request_item", { path, folder, index });
}

/** Resolves to the id that `restoreRequestItem` takes while Undo is offered. */
export function deleteRequestItem(path: string): Promise<number> {
  return invoke<number>("delete_request_item", { path });
}

export function restoreRequestItem(id: number): Promise<void> {
  return invoke<void>("restore_request_item", { id });
}

/** What the editor shows about an unsaved request, in this window's environment. */
export function inspectRequest(
  path: string,
  request: RequestFile,
): Promise<Inspection> {
  return invoke<Inspection>("inspect_request", { path, request });
}

/** Resolves to `null` when the person cancels. */
export function pickRequestFile(): Promise<FileRef | null> {
  return invoke<FileRef | null>("pick_request_file");
}

export function describeFileRef(value: string): Promise<FileRef> {
  return invoke<FileRef>("describe_file_ref", { value });
}
