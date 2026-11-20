//! Requests from a project folder, sent through the production gate, the
//! Vault and the cookie jar of their environment.

mod support;

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dea_engine::requests::{PreparedRequest, RequestService, SendError, Sent, VaultProvider};
use dea_engine::store::Store;
use dea_engine::vault::VaultError;
use dea_engine::{
    Inspection, PasswordPrompt, Prompter, SendAnswer, SendConfirmation, UseError, VaultService,
};
use dea_http::{CancellationToken, Client, ClientConfig, Event, HttpError, ProxyConfig, Response};
use dea_project::{
    ApiKeyLocation, Auth, Body, Environment, KeyValue, Project, RequestError, RequestFile,
    RequestTree, SecretKind, SendOptions, Variable, create_project, load_requests, save_request,
};
use dea_testkit::http::{Received, TestServer, Tls};
use dea_testkit::{canary, find_canary};
use dea_vault::{AuthMethod, SecretData, SecretKey};
use secrecy::SecretString;
use support::{FakeAuthenticator, FakePrompter, ManualClock};

/// `GET {{baseUrl}}/echo`, with the collection's bearer token.
const LIST: &str = "requests/shipments/list.yaml";

/// Answers `confirm_send` from a script (`Cancel` when it runs out) and
/// records every confirmation, also in the shared log.
struct RecordingPrompter {
    answers: Mutex<VecDeque<SendAnswer>>,
    asked: Mutex<Vec<SendConfirmation>>,
    log: Arc<Mutex<Vec<String>>>,
}

impl RecordingPrompter {
    fn answer(&self, answer: SendAnswer) {
        self.answers.lock().unwrap().push_back(answer);
    }

    fn asked(&self) -> Vec<SendConfirmation> {
        self.asked.lock().unwrap().clone()
    }
}

impl Prompter for RecordingPrompter {
    fn vault_password(&self, _prompt: &PasswordPrompt) -> Option<SecretString> {
        None
    }

    fn new_vault_password(&self, _prompt: &PasswordPrompt) -> Option<SecretString> {
        None
    }

    fn confirm_send(&self, confirmation: &SendConfirmation) -> SendAnswer {
        self.log.lock().unwrap().push(format!(
            "confirm {} {}",
            confirmation.method, confirmation.environment
        ));
        self.asked.lock().unwrap().push(confirmation.clone());
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(SendAnswer::Cancel)
    }
}

fn request(method: &str, url: &str) -> RequestFile {
    RequestFile {
        name: "Request".into(),
        method: method.into(),
        url: url.into(),
        path_variables: Vec::new(),
        params: Vec::new(),
        headers: Vec::new(),
        body: None,
        auth: Auth::Inherit,
        options: SendOptions::default(),
    }
}

/// A project in `dir` with `staging` and `production` pointing at
/// `server`, and a `shipments` collection signing in with the Vault's
/// `access_token`.
fn project_in(dir: &Path, server: &TestServer) -> (Project, RequestTree) {
    let mut project = create_project(dir, "harbor-api").unwrap();
    let env = |name: &str, production: bool| Environment {
        name: name.into(),
        production,
        color: None,
        variables: vec![Variable {
            name: "baseUrl".into(),
            value: server.url(""),
            description: None,
        }],
    };
    project.environments = vec![env("staging", false), env("production", true)];
    let collection = dir.join("requests/shipments");
    fs::create_dir_all(&collection).unwrap();
    fs::write(
        collection.join("_folder.yaml"),
        "auth:\n  type: bearer\n  token: '{{vault:access_token}}'\n",
    )
    .unwrap();
    save_request(
        &project,
        Path::new(LIST),
        &request("GET", "{{baseUrl}}/echo"),
    )
    .unwrap();
    let loaded = load_requests(dir).unwrap();
    assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
    (project, loaded.tree)
}

struct Fixture {
    project_dir: tempfile::TempDir,
    app_data: tempfile::TempDir,
    server: TestServer,
    project: Project,
    tree: RequestTree,
    store: Arc<Store>,
    vault: Arc<VaultService>,
    prompter: Arc<RecordingPrompter>,
    /// Confirmations and authentication reasons, in the order they came.
    log: Arc<Mutex<Vec<String>>>,
    service: RequestService,
    /// The staging `access_token`.
    secret: String,
    /// The production `access_token`.
    production_secret: String,
}

impl Fixture {
    /// The Vault holds `access_token` for both environments and starts
    /// locked.
    async fn new() -> Fixture {
        let server = TestServer::start(Tls::None).await;
        let project_dir = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let (project, tree) = project_in(project_dir.path(), &server);

        let log = Arc::new(Mutex::new(Vec::new()));
        let authenticator = FakeAuthenticator {
            method: AuthMethod::DeviceOwner,
            results: Mutex::new(VecDeque::new()),
            reasons: Arc::clone(&log),
            hold: None,
        };
        let (vault, _) = support::service(
            &app_data.path().join("vault"),
            authenticator,
            FakePrompter::answering(&[]),
            ManualClock::new(),
        );
        let (secret, production_secret) = (canary(), canary());
        vault.unlock("Set up the test").unwrap();
        for (environment, value) in [("staging", &secret), ("production", &production_secret)] {
            let key = SecretKey {
                scope: project.vault_scope(),
                environment: environment.into(),
                name: "access_token".into(),
            };
            let data = SecretData::Text(SecretString::from(value.clone()));
            vault.set_secret(&key, SecretKind::Token, &data).unwrap();
        }
        vault.lock();
        log.lock().unwrap().clear();

        let prompter = Arc::new(RecordingPrompter {
            answers: Mutex::default(),
            asked: Mutex::default(),
            log: Arc::clone(&log),
        });
        let store = Arc::new(Store::new(app_data.path()));
        let service = RequestService::new(
            client(app_data.path()),
            Arc::clone(&store),
            Arc::clone(&vault),
            Arc::clone(&prompter) as Arc<dyn Prompter>,
        );
        Fixture {
            project_dir,
            app_data,
            server,
            project,
            tree,
            store,
            vault,
            prompter,
            log,
            service,
            secret,
            production_secret,
        }
    }

    /// Another service over the same app data, as after a restart.
    fn restart(&mut self) {
        self.service = RequestService::new(
            client(self.app_data.path()),
            Arc::new(Store::new(self.app_data.path())),
            Arc::clone(&self.vault),
            Arc::clone(&self.prompter) as Arc<dyn Prompter>,
        );
    }

    /// Saves `file` as `requests/shipments/<name>.yaml` and loads the tree
    /// again.
    fn add(&mut self, name: &str, file: RequestFile) -> PathBuf {
        let path = PathBuf::from(format!("requests/shipments/{name}.yaml"));
        save_request(&self.project, &path, &file).unwrap();
        self.tree = load_requests(self.project_dir.path()).unwrap().tree;
        path
    }

    fn prepare(
        &self,
        path: &Path,
        environment: Option<&str>,
    ) -> Result<PreparedRequest, SendError> {
        self.service
            .prepare(&self.project, &self.tree, path, environment)
    }

    /// Sends `prepared` and checks its cookies were saved.
    async fn send_prepared(&self, prepared: PreparedRequest) -> Response {
        let sent = self.try_send(prepared, &CancellationToken::new()).await;
        let sent = sent.unwrap();
        assert!(sent.cookies_not_saved.is_none(), "{sent:?}");
        sent.response
    }

    async fn try_send(
        &self,
        prepared: PreparedRequest,
        cancel: &CancellationToken,
    ) -> Result<Sent, SendError> {
        is_send(self.service.send(prepared, cancel, None)).await
    }

    /// The `staging` cookies saved on disk, by name.
    fn saved_cookies(&self) -> Vec<String> {
        Store::new(self.app_data.path())
            .load_cookies(&self.project.vault_scope(), "staging")
            .list()
            .into_iter()
            .map(|c| c.name)
            .collect()
    }

    /// Sends `GET {{baseUrl}}<route>` in `environment`.
    async fn send(&mut self, environment: &str, route: &str) -> Response {
        let path = self.add("route", request("GET", &format!("{{{{baseUrl}}}}{route}")));
        let prepared = self.prepare(&path, Some(environment)).unwrap();
        self.send_prepared(prepared).await
    }

    fn last_request(&self) -> Received {
        self.server.requests().pop().expect("no request arrived")
    }

    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

/// The send future can move between threads, as a Tauri command needs.
fn is_send<F: Future + Send>(future: F) -> F {
    future
}

fn client(app_data: &Path) -> Client {
    Client::new(ClientConfig {
        spill_dir: app_data.to_path_buf(),
        proxy: ProxyConfig::None,
        ..ClientConfig::default()
    })
    .unwrap()
}

#[tokio::test]
async fn production_asks_before_anything_is_sent() {
    let f = Fixture::new().await;
    f.prompter.answer(SendAnswer::Cancel);

    let result = f.prepare(Path::new(LIST), Some("production"));

    assert!(matches!(result, Err(SendError::NotConfirmed)), "{result:?}");
    assert!(f.server.requests().is_empty());
    let asked = f.prompter.asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].environment, "production");
    assert_eq!(asked[0].method, "GET");
    assert_eq!(asked[0].url, f.server.url("/echo"));
    assert!(asked[0].can_skip);
    // The Vault was neither opened nor read: it is still locked, and
    // reading a locked Vault would have failed before the question.
    assert_eq!(f.log(), ["confirm GET production"]);
    assert!(f.vault.status().locked);
}

#[tokio::test]
async fn default_prompter_refuses_production() {
    let f = Fixture::new().await;
    // Implements only the Vault password questions, like the desktop app
    // until its confirmation dialog exists.
    let service = RequestService::new(
        client(f.app_data.path()),
        Arc::clone(&f.store),
        Arc::clone(&f.vault),
        FakePrompter::answering(&[]),
    );

    let result = service.prepare(&f.project, &f.tree, Path::new(LIST), Some("production"));

    assert!(matches!(result, Err(SendError::NotConfirmed)), "{result:?}");
    assert!(f.server.requests().is_empty());
    assert!(f.log().is_empty());
}

#[tokio::test]
async fn post_to_production_always_asks() {
    let mut f = Fixture::new().await;
    let post = f.add("create", request("POST", "{{baseUrl}}/echo"));
    let head = f.add("peek", request("HEAD", "{{baseUrl}}/echo"));
    let list = Path::new(LIST);

    f.prompter.answer(SendAnswer::SendAndStopAsking);
    f.prepare(list, Some("production")).unwrap();
    // The next GET and a HEAD to the same environment go without asking.
    f.prepare(list, Some("production")).unwrap();
    f.prepare(&head, Some("production")).unwrap();
    assert_eq!(f.prompter.asked().len(), 1);

    // A POST is asked every time, and cannot stop the asking.
    f.prompter.answer(SendAnswer::SendAndStopAsking);
    f.prepare(&post, Some("production")).unwrap();
    f.prompter.answer(SendAnswer::Cancel);
    let again = f.prepare(&post, Some("production"));
    assert!(matches!(again, Err(SendError::NotConfirmed)), "{again:?}");
    let asked = f.prompter.asked();
    assert_eq!(asked.len(), 3);
    assert!(!asked[1].can_skip && !asked[2].can_skip);
    assert_eq!(asked[1].method, "POST");

    // Another project's production still asks.
    let other_dir = tempfile::tempdir().unwrap();
    let (other, other_tree) = project_in(other_dir.path(), &f.server);
    assert_ne!(other.vault_scope(), f.project.vault_scope());
    f.prompter.answer(SendAnswer::Cancel);
    let result = f
        .service
        .prepare(&other, &other_tree, list, Some("production"));
    assert!(matches!(result, Err(SendError::NotConfirmed)), "{result:?}");
    assert_eq!(f.prompter.asked().len(), 4);

    // Clearing local data forgets the choice.
    f.service.forget_project(&f.project.vault_scope());
    f.prompter.answer(SendAnswer::Cancel);
    let result = f.prepare(list, Some("production"));
    assert!(matches!(result, Err(SendError::NotConfirmed)), "{result:?}");
    assert_eq!(f.prompter.asked().len(), 5);
    assert!(f.server.requests().is_empty());
}

#[tokio::test]
async fn confirmation_comes_before_production_authentication() {
    let f = Fixture::new().await;
    f.prompter.answer(SendAnswer::Send);

    let prepared = f.prepare(Path::new(LIST), Some("production")).unwrap();

    assert_eq!(
        f.log(),
        [
            "confirm GET production",
            "Send GET /echo to production using `access_token`",
        ]
    );
    f.send_prepared(prepared).await;
    let bearer = format!("Bearer {}", f.production_secret);
    assert_eq!(f.last_request().header("authorization"), Some(&*bearer));
}

#[tokio::test]
async fn locked_vault_unlocks_before_reading() {
    let f = Fixture::new().await;
    assert!(f.vault.status().locked);

    let prepared = f.prepare(Path::new(LIST), Some("staging")).unwrap();

    assert_eq!(f.log(), ["Send GET /echo to staging using `access_token`"]);
    assert!(!f.vault.status().locked);
    // Staging is not production: no confirmation.
    assert!(f.prompter.asked().is_empty());
    // Once unlocked, the next request asks nothing.
    f.prepare(Path::new(LIST), Some("staging")).unwrap();
    assert_eq!(f.log().len(), 1);

    f.send_prepared(prepared).await;
    let bearer = format!("Bearer {}", f.secret);
    assert_eq!(f.last_request().header("authorization"), Some(&*bearer));
}

#[tokio::test]
async fn sends_a_request_from_the_project() {
    let mut f = Fixture::new().await;
    // A cookie file in the app data, so the search below covers it.
    f.send("staging", "/cookies/set?session=s1").await;

    let prepared = f.prepare(Path::new(LIST), Some("staging")).unwrap();
    assert_eq!(prepared.method, "GET");
    assert_eq!(prepared.url_redacted, f.server.url("/echo"));
    assert_eq!(prepared.environment.as_deref(), Some("staging"));
    assert!(prepared.unresolved.is_empty());
    let mut shown = format!("{prepared:?}");

    let mut events = Vec::new();
    let mut record = |event: Event| events.push(event);
    let cancel = CancellationToken::new();
    let sent = f
        .service
        .send(prepared, &cancel, Some(&mut record))
        .await
        .unwrap();
    assert_eq!(sent.response.status, 200);
    assert!(
        matches!(events.first(), Some(Event::Head { status: 200, .. })),
        "{events:?}"
    );
    let received = f.last_request();
    let bearer = format!("Bearer {}", f.secret);
    assert_eq!(received.header("authorization"), Some(&*bearer));
    assert_eq!(received.header("cookie"), Some("session=s1"));

    // A secret in the URL path stays out of what Dea shows.
    let path = f.add(
        "by-token",
        request("GET", "{{baseUrl}}/v1/{{vault:access_token}}/me"),
    );
    let in_path = f.prepare(&path, Some("staging")).unwrap();
    assert_eq!(
        in_path.url_redacted,
        f.server.url("/v1/[secret:access_token]/me")
    );
    shown += &format!("{in_path:?}");
    // So does an error about such a URL.
    let broken = f.add(
        "broken",
        request("GET", "http://exa mple/{{vault:access_token}}"),
    );
    let error = f.prepare(&broken, Some("staging")).unwrap_err();
    assert!(
        matches!(&error, SendError::InvalidUrl(url) if url == "http://exa mple/[secret:access_token]"),
        "{error:?}"
    );
    shown += &format!("{error} {error:?}");
    assert!(!shown.contains(&f.secret), "{shown}");

    let roots = [f.app_data.path(), f.project_dir.path()];
    assert!(
        find_canary(&roots, &f.secret).is_empty(),
        "the secret was written to disk"
    );
}

#[tokio::test]
async fn production_cookies_stay_in_production() {
    let mut f = Fixture::new().await;
    f.send("staging", "/cookies/set?session=staging").await;
    f.send("staging", "/echo").await;
    assert_eq!(f.last_request().header("cookie"), Some("session=staging"));

    f.prompter.answer(SendAnswer::Send);
    f.send("production", "/echo").await;
    assert_eq!(f.last_request().header("cookie"), None);

    // "No environment" has a jar of its own too.
    let url = f.server.url("/echo");
    let path = f.add(
        "plain",
        RequestFile {
            auth: Auth::NoAuth,
            ..request("GET", &url)
        },
    );
    let prepared = f.prepare(&path, None).unwrap();
    f.send_prepared(prepared).await;
    assert_eq!(f.last_request().header("cookie"), None);
}

#[tokio::test]
async fn undefined_base_url_names_the_variable() {
    let mut f = Fixture::new().await;
    let path = f.add(
        "x",
        RequestFile {
            auth: Auth::NoAuth,
            ..request("GET", "{{baseUrl}}/x")
        },
    );

    // No environment selected.
    let result = f.prepare(&path, None);

    assert!(
        matches!(&result, Err(SendError::Unresolved(names)) if names == &["baseUrl"]),
        "{result:?}"
    );
}

#[tokio::test]
async fn cookies_off_sends_none() {
    let mut f = Fixture::new().await;
    f.send("staging", "/cookies/set?session=s1").await;
    let mut off = request("GET", "{{baseUrl}}/cookies/set?other=o1");
    off.options.cookies = Some(false);
    let path = f.add("no-cookies", off);

    let prepared = f.prepare(&path, Some("staging")).unwrap();
    f.send_prepared(prepared).await;
    assert_eq!(f.last_request().header("cookie"), None);

    // The cookie it got back was not kept either.
    f.send("staging", "/echo").await;
    assert_eq!(f.last_request().header("cookie"), Some("session=s1"));
}

#[tokio::test]
async fn unresolved_header_still_sends() {
    let mut f = Fixture::new().await;
    let mut traced = request("GET", "{{baseUrl}}/echo");
    traced.headers.push(KeyValue {
        name: "X-Trace".into(),
        value: "{{traceId}}".into(),
        description: None,
        enabled: true,
    });
    let path = f.add("traced", traced);

    let prepared = f.prepare(&path, Some("staging")).unwrap();
    assert_eq!(prepared.unresolved, ["traceId"]);
    f.send_prepared(prepared).await;

    assert_eq!(f.last_request().header("x-trace"), Some("{{traceId}}"));
}

#[tokio::test]
async fn url_without_scheme_uses_http() {
    let mut f = Fixture::new().await;
    let url = format!("127.0.0.1:{}/status/204", f.server.port());
    let path = f.add("bare", request("GET", &url));

    let prepared = f.prepare(&path, Some("staging")).unwrap();
    assert_eq!(prepared.url_redacted, f.server.url("/status/204"));
    let response = f.send_prepared(prepared).await;

    assert_eq!(response.status, 204);
    assert_eq!(f.last_request().path_and_query, "/status/204");
}

#[tokio::test]
async fn cookies_persist_and_clear_with_local_data() {
    let mut f = Fixture::new().await;
    f.send("staging", "/cookies/set?session=s1").await;

    // After a restart the cookie is still there.
    f.restart();
    f.send("staging", "/echo").await;
    assert_eq!(f.last_request().header("cookie"), Some("session=s1"));
    let names: Vec<String> = f
        .service
        .cookies(&f.project, Some("staging"))
        .list()
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(names, ["session"]);

    // Clear local data.
    let scope = f.project.vault_scope();
    f.store.clear_project(&scope).unwrap();
    f.service.forget_project(&scope);
    f.send("staging", "/echo").await;
    assert_eq!(f.last_request().header("cookie"), None);
    assert!(
        f.service
            .cookies(&f.project, Some("staging"))
            .list()
            .is_empty()
    );

    f.restart();
    f.send("staging", "/echo").await;
    assert_eq!(f.last_request().header("cookie"), None);
}

/// A regular file where the `cookies` folder belongs: every cookie save
/// fails, on every OS.
fn block_cookie_saves(app_data: &Path) {
    fs::write(app_data.join("cookies"), "").unwrap();
}

fn unblock_cookie_saves(app_data: &Path) {
    fs::remove_file(app_data.join("cookies")).unwrap();
}

#[tokio::test]
async fn a_failed_cookie_save_keeps_the_response() {
    let mut f = Fixture::new().await;
    block_cookie_saves(f.app_data.path());
    let path = f.add("set", request("GET", "{{baseUrl}}/cookies/set?session=s1"));

    let prepared = f.prepare(&path, Some("staging")).unwrap();
    let sent = f.try_send(prepared, &CancellationToken::new()).await;

    let sent = sent.expect("the exchange went through, so its response is kept");
    assert_eq!(sent.response.status, 200);
    assert!(sent.cookies_not_saved.is_some(), "{sent:?}");
    // The cookie is still used, and the save is tried again.
    let prepared = f.prepare(Path::new(LIST), Some("staging")).unwrap();
    let sent = f.try_send(prepared, &CancellationToken::new()).await;
    assert!(sent.unwrap().cookies_not_saved.is_some());
    assert_eq!(f.last_request().header("cookie"), Some("session=s1"));

    // Once the disk works again, the next send saves the jar, though its
    // response sets no cookie.
    unblock_cookie_saves(f.app_data.path());
    let response = f.send("staging", "/echo").await;
    assert_eq!(response.status, 200);
    assert_eq!(f.saved_cookies(), ["session"]);
}

#[tokio::test]
async fn unsaved_cookies_are_saved_after_a_failed_exchange() {
    let mut f = Fixture::new().await;
    block_cookie_saves(f.app_data.path());
    let set = f.add("set", request("GET", "{{baseUrl}}/cookies/set?session=s1"));
    let prepared = f.prepare(&set, Some("staging")).unwrap();
    let sent = f.try_send(prepared, &CancellationToken::new()).await;
    assert!(sent.unwrap().cookies_not_saved.is_some());

    // A failed exchange reports its own error, not the save's.
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let prepared = f.prepare(Path::new(LIST), Some("staging")).unwrap();
    let result = f.try_send(prepared, &cancelled).await;
    assert!(
        matches!(result, Err(SendError::Http(HttpError::Cancelled))),
        "{result:?}"
    );

    // It still saves what an earlier send could not.
    unblock_cookie_saves(f.app_data.path());
    let prepared = f.prepare(Path::new(LIST), Some("staging")).unwrap();
    let result = f.try_send(prepared, &cancelled).await;
    assert!(
        matches!(result, Err(SendError::Http(HttpError::Cancelled))),
        "{result:?}"
    );
    assert_eq!(f.saved_cookies(), ["session"]);
}

fn row(name: &str, value: &str) -> KeyValue {
    KeyValue {
        name: name.into(),
        value: value.into(),
        description: None,
        enabled: true,
    }
}

/// `GET {{baseUrl}}<route>` signed with an API key header, a header from
/// the Vault, a `Proxy-Authorization` the person wrote and a plain header.
fn with_credentials(route: &str, api_key: &str) -> RequestFile {
    RequestFile {
        headers: vec![
            row("X-From-Vault", "{{vault:access_token}}"),
            row("Proxy-Authorization", "Basic cHJveHk6cHc="),
            row("X-Keep", "1"),
        ],
        auth: Auth::ApiKey {
            key: "X-Api-Key".into(),
            value: api_key.into(),
            location: ApiKeyLocation::Header,
        },
        ..request("GET", &format!("{{{{baseUrl}}}}{route}"))
    }
}

const CREDENTIALS: [&str; 3] = ["x-api-key", "x-from-vault", "proxy-authorization"];

#[tokio::test]
async fn credentials_stay_behind_on_a_redirect_to_another_origin() {
    let mut f = Fixture::new().await;
    // Another port and another host name for the same machine.
    let other = TestServer::start(Tls::None).await;
    let target = other.url("/echo").replace("127.0.0.1", "localhost");
    for api_key in ["{{vault:access_token}}", "literal-key"] {
        let mut file = with_credentials("/redirect-to", api_key);
        file.params = vec![row("url", &target)];
        let path = f.add("away", file);

        let prepared = f.prepare(&path, Some("staging")).unwrap();
        let response = f.send_prepared(prepared).await;

        assert_eq!(response.status, 200);
        let first = f.last_request();
        for name in CREDENTIALS {
            assert!(first.header(name).is_some(), "{name} {api_key}");
        }
        let moved = other.requests().pop().expect("the redirect was followed");
        for name in CREDENTIALS {
            assert_eq!(moved.header(name), None, "{name} {api_key}");
        }
        assert_eq!(moved.header("x-keep"), Some("1"));
    }

    // Within one origin they all go along.
    let path = f.add(
        "same",
        with_credentials("/redirect/1", "{{vault:access_token}}"),
    );
    let prepared = f.prepare(&path, Some("staging")).unwrap();
    f.send_prepared(prepared).await;
    let last = f.last_request();
    assert_eq!(last.path_and_query, "/redirect/0");
    assert_eq!(last.header("x-api-key"), Some(&*f.secret));
    assert_eq!(last.header("x-from-vault"), Some(&*f.secret));
    assert_eq!(
        last.header("proxy-authorization"),
        Some("Basic cHJveHk6cHc=")
    );
}

#[tokio::test]
async fn timeout_zero_means_no_limit() {
    let mut f = Fixture::new().await;
    let mut file = request("GET", "{{baseUrl}}/delay/300");
    file.options.timeout_ms = Some(0);
    let path = f.add("patient", file);

    let prepared = f.prepare(&path, Some("staging")).unwrap();
    let response = f.send_prepared(prepared).await;

    assert_eq!(response.status, 200);
}

/// The Vault stayed locked, was never asked, and nothing was shown.
fn assert_untouched(f: &Fixture) {
    assert!(f.vault.status().locked, "the Vault was unlocked");
    assert!(f.log().is_empty(), "something was asked: {:?}", f.log());
    assert!(f.prompter.asked().is_empty());
}

fn inspect(
    f: &Fixture,
    path: &Path,
    draft: &RequestFile,
    environment: Option<&str>,
) -> Result<Inspection, SendError> {
    f.service
        .inspect(&f.project, &f.tree, path, draft, environment)
}

#[tokio::test]
async fn inspect_reports_values_sources_and_undefined_without_the_vault() {
    let mut f = Fixture::new().await;
    f.project.environments[0].variables.push(Variable {
        name: "token".into(),
        value: "{{vault:access_token}}".into(),
        description: None,
    });
    let draft = request(
        "GET",
        "{{baseUrl}}/{{carrierCode}}?t={{token}}&id={{$guid}}",
    );

    let inspection = inspect(&f, Path::new(LIST), &draft, Some("staging")).unwrap();

    let find = |name: &str| {
        inspection
            .variables
            .iter()
            .find(|v| v.name == name)
            .unwrap()
    };
    let base = find("baseUrl");
    assert_eq!(base.shown, f.server.url(""));
    assert_eq!(base.source, "Environment staging");
    assert!(!base.secret);
    // A scope variable that holds a secret keeps its scope as the source.
    let token = find("token");
    assert_eq!((token.shown.as_str(), token.secret), ("••••••••", true));
    assert_eq!(token.source, "Environment staging");
    let vault = find("vault:access_token");
    assert_eq!((vault.shown.as_str(), vault.secret), ("••••••••", true));
    assert_eq!(vault.source, "Vault, staging. The value stays hidden");
    let dynamic = find("$guid");
    assert_eq!(dynamic.source, "Dynamic, new on each send");
    assert_eq!(inspection.undefined, ["carrierCode"]);
    let json = format!(
        "{inspection:?}{}",
        serde_json::to_string(&inspection).unwrap()
    );
    assert!(
        !json.contains(&f.secret) && !json.contains("[secret:"),
        "{json}"
    );
    assert_untouched(&f);
}

/// A Vault that is not opened yet: opening it fails the test.
struct NeverOpened;

impl VaultProvider for NeverOpened {
    fn open(&self) -> Result<Arc<VaultService>, UseError> {
        panic!("the Vault was opened")
    }
}

/// A service over the fixture that gets its Vault from `vault`.
fn service_with(f: &Fixture, vault: impl VaultProvider + 'static) -> RequestService {
    RequestService::new(
        client(f.app_data.path()),
        Arc::clone(&f.store),
        vault,
        Arc::clone(&f.prompter) as Arc<dyn Prompter>,
    )
}

#[tokio::test]
async fn inspect_never_opens_the_vault() {
    let f = Fixture::new().await;
    let service = service_with(&f, NeverOpened);
    let draft = request("GET", "{{baseUrl}}/echo?t={{vault:access_token}}");

    let inspection = service
        .inspect(
            &f.project,
            &f.tree,
            Path::new(LIST),
            &draft,
            Some("production"),
        )
        .unwrap();

    let vault = inspection
        .variables
        .iter()
        .find(|v| v.name == "vault:access_token")
        .unwrap();
    assert!(vault.secret);
}

#[tokio::test]
async fn the_vault_is_opened_only_after_production_is_confirmed() {
    let f = Fixture::new().await;
    let service = service_with(&f, NeverOpened);
    f.prompter.answer(SendAnswer::Cancel);

    let result = service.prepare(&f.project, &f.tree, Path::new(LIST), Some("production"));

    assert!(matches!(result, Err(SendError::NotConfirmed)), "{result:?}");
}

/// A Vault that cannot be opened, as when the keychain is gone.
struct Unavailable;

impl VaultProvider for Unavailable {
    fn open(&self) -> Result<Arc<VaultService>, UseError> {
        Err(VaultError::Backend("keychain gone".into()).into())
    }
}

#[tokio::test]
async fn a_vault_that_cannot_be_opened_stops_the_send() {
    let f = Fixture::new().await;
    let service = service_with(&f, Unavailable);

    let result = service.prepare(&f.project, &f.tree, Path::new(LIST), Some("staging"));

    assert!(
        matches!(result, Err(SendError::Secret(UseError::Vault(_)))),
        "{result:?}"
    );
    assert!(f.server.requests().is_empty());
}

#[tokio::test]
async fn inspect_names_the_folder_and_collection_of_a_variable() {
    let mut f = Fixture::new().await;
    fs::write(
        f.project_dir.path().join("requests/shipments/_folder.yaml"),
        "name: Shipments\nvariables:\n  - name: carrier\n    value: acme\n",
    )
    .unwrap();
    let inner = f.project_dir.path().join("requests/shipments/eu");
    fs::create_dir_all(&inner).unwrap();
    fs::write(
        inner.join("_folder.yaml"),
        "variables:\n  - name: region\n    value: fr\n",
    )
    .unwrap();
    f.tree = load_requests(f.project_dir.path()).unwrap().tree;
    let draft = request("GET", "{{baseUrl}}/{{carrier}}/{{region}}");

    let path = Path::new("requests/shipments/eu/new.yaml");
    let inspection = inspect(&f, path, &draft, Some("staging")).unwrap();

    let source = |name: &str| {
        inspection
            .variables
            .iter()
            .find(|v| v.name == name)
            .map(|v| v.source.as_str())
    };
    assert_eq!(source("carrier"), Some("Collection Shipments"));
    assert_eq!(source("region"), Some("Folder eu"));
    assert!(inspection.undefined.is_empty());
}

#[tokio::test]
async fn inspect_lists_automatic_headers_and_marks_replaced_ones() {
    let f = Fixture::new().await;
    let mut draft = request("POST", "{{baseUrl}}/x");
    draft.body = Some(Body::Json { text: "{}".into() });
    draft.headers = vec![row("content-type", "text/plain")];

    let inspection = inspect(&f, Path::new(LIST), &draft, Some("staging")).unwrap();

    let names: Vec<&str> = inspection
        .automatic_headers
        .iter()
        .map(|h| h.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "Host",
            "User-Agent",
            "Accept",
            "Content-Type",
            "Content-Length"
        ]
    );
    let header = |name: &str| {
        inspection
            .automatic_headers
            .iter()
            .find(|h| h.name == name)
            .unwrap()
    };
    assert!(header("Content-Type").replaced);
    assert_eq!(
        header("Content-Type").value.as_deref(),
        Some("application/json")
    );
    assert!(!header("User-Agent").replaced);
    assert_eq!(
        header("User-Agent").value.as_deref(),
        Some(ClientConfig::default().user_agent.as_str())
    );
    assert_eq!(header("Content-Length").value, None);
    let host = f.server.url("").replace("http://", "");
    assert_eq!(header("Host").value.as_deref(), Some(host.as_str()));
}

#[tokio::test]
async fn inspect_leaves_the_host_open_while_the_url_is_unfinished() {
    let f = Fixture::new().await;
    let draft = request("GET", "{{missingHost}}/x");

    let inspection = inspect(&f, Path::new(LIST), &draft, Some("staging")).unwrap();

    assert_eq!(inspection.undefined, ["missingHost"]);
    assert_eq!(inspection.automatic_headers[0].name, "Host");
    assert_eq!(inspection.automatic_headers[0].value, None);
}

#[tokio::test]
async fn inspect_uses_the_draft_not_the_file() {
    let f = Fixture::new().await;
    // The saved file uses {{baseUrl}}; the draft uses only {{draftOnly}}.
    let draft = request("GET", "https://x.test/{{draftOnly}}");

    let inspection = inspect(&f, Path::new(LIST), &draft, Some("staging")).unwrap();

    assert_eq!(inspection.undefined, ["draftOnly"]);
    assert!(inspection.variables.iter().all(|v| v.name != "baseUrl"));
    assert_eq!(
        inspection.automatic_headers[0].value.as_deref(),
        Some("x.test")
    );
}

#[tokio::test]
async fn inspect_reports_inherited_options() {
    let mut f = Fixture::new().await;
    fs::write(
        f.project_dir.path().join("requests/shipments/_folder.yaml"),
        "options:\n  timeoutMs: 8000\n  cookies: false\n",
    )
    .unwrap();
    f.tree = load_requests(f.project_dir.path()).unwrap().tree;
    let mut draft = request("GET", "{{baseUrl}}/x");
    draft.options.max_redirects = Some(2);

    let o = inspect(&f, Path::new(LIST), &draft, Some("staging"))
        .unwrap()
        .options;

    assert_eq!(
        (
            o.timeout_ms,
            o.max_redirects,
            o.cookies,
            o.follow_redirects,
            o.verify_tls
        ),
        (8000, 2, false, true, true)
    );
}

#[tokio::test]
async fn inspect_lists_no_content_length_without_a_body() {
    let f = Fixture::new().await;
    let draft = request("GET", "{{baseUrl}}/x");

    let inspection = inspect(&f, Path::new(LIST), &draft, Some("staging")).unwrap();

    let names: Vec<&str> = inspection
        .automatic_headers
        .iter()
        .map(|h| h.name.as_str())
        .collect();
    assert_eq!(names, ["Host", "User-Agent", "Accept"]);
}

#[tokio::test]
async fn inspect_lists_no_content_length_for_an_empty_text_body() {
    let f = Fixture::new().await;
    let mut draft = request("POST", "{{baseUrl}}/x");
    draft.body = Some(Body::Json {
        text: String::new(),
    });

    let inspection = inspect(&f, Path::new(LIST), &draft, Some("staging")).unwrap();

    assert!(
        inspection
            .automatic_headers
            .iter()
            .all(|h| h.name != "Content-Length")
    );
}

#[tokio::test]
async fn inspect_works_when_a_local_file_is_not_on_this_device() {
    let f = Fixture::new().await;
    let mut draft = request("POST", "{{baseUrl}}/{{carrierCode}}");
    draft.body = Some(Body::Binary {
        file: "local:0123456789ab".into(),
    });

    let inspection = inspect(&f, Path::new(LIST), &draft, Some("staging")).unwrap();

    assert!(inspection.variables.iter().any(|v| v.name == "baseUrl"));
    assert_eq!(inspection.undefined, ["carrierCode"]);
    let header = |name: &str| {
        inspection
            .automatic_headers
            .iter()
            .find(|h| h.name == name)
            .unwrap()
    };
    assert_eq!(
        header("Content-Type").value.as_deref(),
        Some("application/octet-stream")
    );
    assert!(
        inspection
            .automatic_headers
            .iter()
            .any(|h| h.name == "Content-Length")
    );
    // Sending it still fails: the file is not here.
    let mut f = f;
    let path = f.add("binary", draft);
    let result = f.prepare(&path, Some("staging"));
    assert!(matches!(
        result,
        Err(SendError::Request(RequestError::LocalFileMissing(_)))
    ));
}

#[tokio::test]
async fn a_folder_header_replaces_the_automatic_user_agent() {
    let f = Fixture::new().await;
    fs::write(
        f.project_dir.path().join("requests/shipments/_folder.yaml"),
        "headers:\n  - name: user-agent\n    value: Harbor\n",
    )
    .unwrap();
    let mut f = f;
    f.tree = load_requests(f.project_dir.path()).unwrap().tree;
    let draft = request("GET", "{{baseUrl}}/x");

    let inspection = inspect(&f, Path::new(LIST), &draft, Some("staging")).unwrap();

    let replaced = |name: &str| {
        inspection
            .automatic_headers
            .iter()
            .find(|h| h.name == name)
            .unwrap()
            .replaced
    };
    assert!(replaced("User-Agent"));
    assert!(!replaced("Accept"));
}
