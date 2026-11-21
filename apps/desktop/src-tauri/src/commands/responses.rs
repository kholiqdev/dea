//! Reading, copying and saving the response of a tab, and the cookie
//! manager. The body stays in Rust; the window asks for what it shows.
//! Every call that reads a body or a jar does file IO or takes the jar
//! lock, so each runs off the UI thread.

use std::path::PathBuf;

use dea_engine::json_filter::Filtered;
use dea_engine::requests::RequestService;
use dea_engine::{
    BodyText, Clipboard as _, CookieInfo, CookieKey, Found, ResponseStore, RowsPage, TextView,
};
use dea_project::{Project, load_project};
use tauri::async_runtime::spawn_blocking;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::commands::requests::session_of;
use crate::error::{IpcError, IpcErrorKind};
use crate::state::{AppState, e2e_save_path};

/// The response of tab `id` of `window`, kept under the key its send used.
fn response_key(window: &str, id: &str) -> String {
    format!("{window}/{id}")
}

fn invalid(message: impl Into<String>) -> IpcError {
    IpcError {
        kind: IpcErrorKind::Invalid,
        message: message.into(),
    }
}

/// The text of the body as one string, in the view asked for.
#[tauri::command]
pub async fn response_text(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    view: TextView,
) -> Result<BodyText, IpcError> {
    let key = response_key(window.label(), &id);
    spawn_blocking(move || Ok(app.state::<AppState>().responses.text(&key, view)?)).await?
}

/// The body as received, byte for byte.
#[tauri::command]
pub async fn response_bytes(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
) -> Result<Response, IpcError> {
    let key = response_key(window.label(), &id);
    let bytes = spawn_blocking(move || app.state::<AppState>().responses.bytes(&key)).await??;
    Ok(Response::new(bytes))
}

/// `count` rows of the body from row `from`.
#[tauri::command]
pub async fn response_rows(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    from: u64,
    count: u32,
) -> Result<RowsPage, IpcError> {
    let key = response_key(window.label(), &id);
    spawn_blocking(move || Ok(app.state::<AppState>().responses.rows(&key, from, count)?)).await?
}

/// The next match of `query` from row `from`, in the direction asked for.
#[tauri::command]
pub async fn find_in_response(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    query: String,
    from_row: u64,
    backwards: bool,
) -> Result<Option<Found>, IpcError> {
    let key = response_key(window.label(), &id);
    spawn_blocking(move || {
        Ok(app
            .state::<AppState>()
            .responses
            .find(&key, &query, from_row, backwards)?)
    })
    .await?
}

/// The JSON body narrowed to what the JSONPath `path` selects.
#[tauri::command]
pub async fn filter_response(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    path: String,
) -> Result<Filtered, IpcError> {
    let key = response_key(window.label(), &id);
    spawn_blocking(move || Ok(app.state::<AppState>().responses.filter(&key, &path)?)).await?
}

/// Puts the raw text of the body on the clipboard.
#[tauri::command]
pub async fn copy_response(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
) -> Result<(), IpcError> {
    let key = response_key(window.label(), &id);
    spawn_blocking(move || {
        let state = app.state::<AppState>();
        let body = state.responses.text(&key, TextView::Raw)?;
        state
            .vault
            .clipboard()
            .set_text(&body.text)
            .map_err(|message| IpcError {
                kind: IpcErrorKind::Internal,
                message,
            })
    })
    .await?
}

/// Writes the body to `chosen`, and gives the name of the file it wrote.
/// `None` when nothing was chosen.
fn save_body(
    responses: &ResponseStore,
    key: &str,
    chosen: Option<PathBuf>,
) -> Result<Option<String>, IpcError> {
    let Some(dest) = chosen else {
        return Ok(None);
    };
    responses.save_to(key, &dest)?;
    Ok(Some(dest.file_name().map_or_else(String::new, |n| {
        n.to_string_lossy().into_owned()
    })))
}

/// Asks where to save the body, suggesting `name`, and writes it there.
/// Gives the name of the saved file, or `None` when the person cancels.
#[tauri::command]
pub async fn save_response(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    name: String,
) -> Result<Option<String>, IpcError> {
    let key = response_key(window.label(), &id);
    let chosen = match e2e_save_path() {
        Some(path) => Some(PathBuf::from(path)),
        None => {
            let picker = window.clone();
            spawn_blocking(move || {
                picker
                    .dialog()
                    .file()
                    .set_parent(&picker)
                    .set_file_name(name)
                    .blocking_save_file()
            })
            .await?
            .and_then(|p| p.into_path().ok())
        }
    };
    spawn_blocking(move || save_body(&app.state::<AppState>().responses, &key, chosen)).await?
}

/// Drops the response of tab `id`, such as when the tab closes.
#[tauri::command]
pub fn release_response(window: WebviewWindow, id: String) -> Result<(), IpcError> {
    window
        .state::<AppState>()
        .responses
        .release(&response_key(window.label(), &id));
    Ok(())
}

fn project_of(app: &AppHandle, window: &str) -> Result<Project, IpcError> {
    let session = session_of(app, window)?;
    Ok(load_project(&session.root)?.project)
}

/// The unexpired cookies of `environment`, sorted by domain, path and name.
#[tauri::command]
pub async fn list_cookies(
    app: AppHandle,
    window: WebviewWindow,
    environment: Option<String>,
) -> Result<Vec<CookieInfo>, IpcError> {
    let label = window.label().to_string();
    spawn_blocking(move || {
        let project = project_of(&app, &label)?;
        Ok(app
            .state::<AppState>()
            .requests
            .list_cookies(&project, environment.as_deref()))
    })
    .await?
}

/// Replaces cookie `original` with `cookie`. A cookie that is refused
/// leaves `original` where it was.
fn replace_cookie(
    requests: &RequestService,
    project: &Project,
    environment: Option<&str>,
    original: &CookieKey,
    cookie: &CookieInfo,
) -> Result<(), IpcError> {
    let stored = cookie.to_stored().map_err(invalid)?;
    requests.change_cookies(project, environment, |jar| {
        let before = jar.list().into_iter().find(|c| {
            (&c.domain, &c.path, &c.name) == (&original.domain, &original.path, &original.name)
        });
        jar.remove(&original.domain, &original.path, &original.name);
        jar.insert(stored).map_err(|e| {
            // A bad edit must not lose the cookie.
            if let Some(before) = before {
                let _ = jar.insert(before);
            }
            invalid(e.to_string())
        })
    })?
}

/// Removes the cookie `key`.
fn remove_cookie(
    requests: &RequestService,
    project: &Project,
    environment: Option<&str>,
    key: &CookieKey,
) -> Result<(), IpcError> {
    requests.change_cookies(project, environment, |jar| {
        jar.remove(&key.domain, &key.path, &key.name);
    })?;
    Ok(())
}

/// Removes every cookie of exactly `domain`, and counts them.
fn remove_domain(
    requests: &RequestService,
    project: &Project,
    environment: Option<&str>,
    domain: &str,
) -> Result<u32, IpcError> {
    let count = requests.change_cookies(project, environment, |jar| {
        jar.list()
            .iter()
            .filter(|c| c.domain == domain)
            .filter(|c| jar.remove(&c.domain, &c.path, &c.name))
            .count()
    })?;
    Ok(u32::try_from(count).unwrap_or(u32::MAX))
}

/// Replaces the cookie `original` with `cookie`, which may have another
/// name, domain or path.
#[tauri::command]
pub async fn save_cookie(
    app: AppHandle,
    window: WebviewWindow,
    environment: Option<String>,
    original: CookieKey,
    cookie: CookieInfo,
) -> Result<(), IpcError> {
    let label = window.label().to_string();
    spawn_blocking(move || {
        let project = project_of(&app, &label)?;
        let state = app.state::<AppState>();
        replace_cookie(
            &state.requests,
            &project,
            environment.as_deref(),
            &original,
            &cookie,
        )
    })
    .await?
}

/// Deletes one cookie.
#[tauri::command]
pub async fn delete_cookie(
    app: AppHandle,
    window: WebviewWindow,
    environment: Option<String>,
    key: CookieKey,
) -> Result<(), IpcError> {
    let label = window.label().to_string();
    spawn_blocking(move || {
        let project = project_of(&app, &label)?;
        let state = app.state::<AppState>();
        remove_cookie(&state.requests, &project, environment.as_deref(), &key)
    })
    .await?
}

/// Deletes every cookie of `domain`, and says how many there were.
#[tauri::command]
pub async fn delete_domain_cookies(
    app: AppHandle,
    window: WebviewWindow,
    environment: Option<String>,
    domain: String,
) -> Result<u32, IpcError> {
    let label = window.label().to_string();
    spawn_blocking(move || {
        let project = project_of(&app, &label)?;
        let state = app.state::<AppState>();
        remove_domain(&state.requests, &project, environment.as_deref(), &domain)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use dea_engine::{ResponseError, Sent};
    use dea_http::{HttpVersion, ResponseBody, Timing};
    use dea_project::create_project;

    use super::*;
    use crate::state::take_line;
    use crate::testing::{RecordingWindows, state_with};

    fn kept(store: &ResponseStore, key: &str, body: &str) {
        store.insert(
            key,
            Sent {
                response: dea_http::Response {
                    version: HttpVersion::Http11,
                    status: 200,
                    reason: "OK".into(),
                    headers: vec![("content-type".into(), "application/json".into())],
                    body: ResponseBody::Memory(Bytes::from(body.to_string())),
                    truncated: false,
                    content_encoding: None,
                    timing: Timing::default(),
                    url: "http://example.test/".into(),
                    redirects: Vec::new(),
                },
                url: "http://example.test/".into(),
                cookies_not_saved: None,
            },
        );
    }

    fn cookie(name: &str, path: &str) -> CookieInfo {
        CookieInfo {
            name: name.into(),
            value: "v".into(),
            domain: "api.test".into(),
            path: path.into(),
            expires: None,
            secure: false,
            http_only: false,
            host_only: true,
        }
    }

    fn key_of(c: &CookieInfo) -> CookieKey {
        CookieKey {
            domain: c.domain.clone(),
            path: c.path.clone(),
            name: c.name.clone(),
        }
    }

    fn with_jar(cookies: &[CookieInfo]) -> (tempfile::TempDir, Project, std::sync::Arc<AppState>) {
        let dir = tempfile::tempdir().unwrap();
        let project = create_project(&dir.path().join("harbor"), "harbor").unwrap();
        let (windows, ..) = RecordingWindows::new(&["main"], Some("main"));
        let state = state_with(&dir.path().join("data"), windows);
        state
            .requests
            .change_cookies(&project, None, |jar| {
                for c in cookies {
                    jar.insert(c.to_stored().unwrap()).unwrap();
                }
            })
            .unwrap();
        (dir, project, state)
    }

    #[test]
    fn missing_response_is_reported_as_missing() {
        let store = ResponseStore::default();
        let error: IpcError = store.text("main/t1", TextView::Raw).unwrap_err().into();
        assert_eq!(error.kind, IpcErrorKind::Missing);
        let error: IpcError = ResponseError::NotText.into();
        assert_eq!(error.kind, IpcErrorKind::Invalid);
        let error: IpcError = ResponseError::Io("disk".into()).into();
        assert_eq!(error.kind, IpcErrorKind::Io);
    }

    #[test]
    fn save_response_writes_to_the_e2e_path() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.json");
        let pick = dir.path().join("save.txt");
        std::fs::write(&pick, format!("{}\n", out.display())).unwrap();
        let store = ResponseStore::default();
        kept(&store, "main/t1", "{}");

        let chosen = take_line(&pick).map(PathBuf::from);
        let name = save_body(&store, "main/t1", chosen).unwrap();

        assert_eq!(name.as_deref(), Some("out.json"));
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "{}");
        assert_eq!(save_body(&store, "main/t1", None).unwrap(), None);
    }

    #[test]
    fn saving_a_released_response_is_missing() {
        let store = ResponseStore::default();
        let dest = tempfile::tempdir().unwrap().keep().join("x");
        let error = save_body(&store, "main/t1", Some(dest)).unwrap_err();
        assert_eq!(error.kind, IpcErrorKind::Missing);
    }

    #[test]
    fn cookie_edit_replaces_the_original() {
        let old = cookie("sid", "/");
        let (_dir, project, state) = with_jar(std::slice::from_ref(&old));
        let edited = cookie("sid", "/v2");

        replace_cookie(&state.requests, &project, None, &key_of(&old), &edited).unwrap();

        let listed = state.requests.list_cookies(&project, None);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, "/v2");
    }

    #[test]
    fn a_refused_edit_keeps_the_original() {
        let old = cookie("sid", "/");
        let (_dir, project, state) = with_jar(std::slice::from_ref(&old));
        let mut bad = cookie("sid", "/v2");
        bad.name = "bad name;".into();
        let error =
            replace_cookie(&state.requests, &project, None, &key_of(&old), &bad).unwrap_err();
        assert_eq!(error.kind, IpcErrorKind::Invalid);
        let mut dated = cookie("sid", "/v2");
        dated.expires = Some("tomorrow".into());
        let error =
            replace_cookie(&state.requests, &project, None, &key_of(&old), &dated).unwrap_err();
        assert_eq!(error.kind, IpcErrorKind::Invalid);

        let listed = state.requests.list_cookies(&project, None);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, "/");
    }

    #[test]
    fn deleting_a_domain_removes_only_that_domain() {
        let mut other = cookie("sid", "/");
        other.domain = "other.test".into();
        let (_dir, project, state) =
            with_jar(&[cookie("a", "/"), cookie("b", "/x"), other.clone()]);

        let count = remove_domain(&state.requests, &project, None, "api.test").unwrap();

        assert_eq!(count, 2);
        let listed = state.requests.list_cookies(&project, None);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].domain, "other.test");
        remove_cookie(&state.requests, &project, None, &key_of(&other)).unwrap();
        assert!(state.requests.list_cookies(&project, None).is_empty());
    }
}
