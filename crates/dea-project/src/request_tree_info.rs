//! The request tree as the sidebar shows it: names, methods and URLs, with
//! paths relative to the project root written with `/`.

use std::path::Path;

use serde::Serialize;

use crate::{FileError, Folder, LoadedRequests, TreeItem};

/// The collections under `requests/` and the files that could not be used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct RequestTreeInfo {
    pub collections: Vec<FolderNode>,
    pub errors: Vec<RequestFileError>,
}

/// A request or folder file that could not be used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct RequestFileError {
    pub path: String,
    pub line: Option<u32>,
    pub message: String,
    /// The file still holds git conflict markers.
    pub merge_conflict: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub enum RequestNode {
    Folder(FolderNode),
    Request(RequestLeaf),
}

/// A collection or folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct FolderNode {
    pub path: String,
    /// The name in its `_folder.yaml`, or the name of the directory.
    pub name: String,
    /// Its `_folder.yaml` could not be used: the settings are the defaults.
    pub broken: bool,
    /// Requests in this folder and every folder below it.
    pub request_count: u32,
    pub items: Vec<RequestNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct RequestLeaf {
    pub path: String,
    pub name: String,
    pub method: String,
    pub url: String,
}

/// The tree of `loaded` as the sidebar shows it.
#[must_use]
pub fn tree_info(loaded: &LoadedRequests) -> RequestTreeInfo {
    RequestTreeInfo {
        collections: loaded.tree.collections.iter().map(folder_node).collect(),
        errors: loaded.errors.iter().map(file_error).collect(),
    }
}

fn folder_node(folder: &Folder) -> FolderNode {
    let items: Vec<RequestNode> = folder
        .items
        .iter()
        .map(|item| match item {
            TreeItem::Folder(inner) => RequestNode::Folder(folder_node(inner)),
            TreeItem::Request(entry) => RequestNode::Request(RequestLeaf {
                path: slash_path(&entry.path),
                name: entry.request.name.clone(),
                method: entry.request.method.clone(),
                url: entry.request.url.clone(),
            }),
        })
        .collect();
    let request_count = items
        .iter()
        .map(|item| match item {
            RequestNode::Folder(inner) => inner.request_count,
            RequestNode::Request(_) => 1,
        })
        .fold(0, u32::saturating_add);
    FolderNode {
        path: slash_path(&folder.dir),
        name: folder.display_name(),
        broken: folder.broken,
        request_count,
        items,
    }
}

fn file_error(e: &FileError) -> RequestFileError {
    RequestFileError {
        path: slash_path(&e.path),
        line: e.line.and_then(|l| u32::try_from(l).ok()),
        message: e.message.clone(),
        merge_conflict: e.merge_conflict,
    }
}

/// `path` with `/` between its parts, whatever the platform.
fn slash_path(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
