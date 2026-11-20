use std::collections::BTreeMap;
use std::error::Error;
use std::fs;

use std::path::{Path, PathBuf};

use dea_engine::store::{
    Appearance, MAX_CLOSED_TABS, Security, Settings, SidebarState, Store, StoreError,
    TabEnvironment, TabState, ThemePreference, UiState,
};
use dea_http::{CookieJar, StoredCookie};
use dea_vault::{AutoLock, Policy};

fn tab(n: usize) -> TabState {
    TabState {
        id: format!("t{n}"),
        tab_type: "e2e.tab".into(),
        title: format!("Tab {n}"),
        params: serde_json::json!({ "n": n }),
        environment: None,
    }
}

#[test]
fn missing_files_give_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let s = store.load_settings();
    assert_eq!(s.appearance.theme, ThemePreference::System);
    assert_eq!(s.appearance.ui_font, "IBM Plex Sans");
    assert_eq!(s.appearance.mono_font, "IBM Plex Mono");
    assert_eq!(s.appearance.font_size, 13);
    assert!(s.shortcuts.is_empty());
    assert_eq!(store.load_ui_state("no-project", 0), UiState::default());
    // Reading creates nothing.
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn settings_and_ui_state_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let settings = Settings {
        appearance: Appearance {
            theme: ThemePreference::Light,
            ui_font: "Inter".into(),
            mono_font: "JetBrains Mono".into(),
            font_size: 15,
        },
        shortcuts: BTreeMap::from([
            ("requests.open".into(), Some("Mod+Shift+1".into())),
            ("shell.reopenTab".into(), None),
        ]),
        ..Settings::default()
    };
    store.save_settings(settings.clone()).unwrap();
    let ui = UiState {
        active_module: Some("terminal".into()),
        rail_expanded: true,
        sidebars: BTreeMap::from([(
            "requests".into(),
            SidebarState {
                visible: false,
                width: 320,
            },
        )]),
        tabs: vec![tab(1), tab(2)],
        active_tab: Some("t2".into()),
        closed_tabs: vec![tab(0)],
        ..UiState::default()
    };
    store.save_ui_state("no-project", 0, ui.clone()).unwrap();

    let again = Store::new(dir.path());
    assert_eq!(again.load_settings(), settings);
    assert_eq!(again.load_ui_state("no-project", 0), ui);
}

#[test]
fn files_use_camel_case_names() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.save_settings(Settings::default()).unwrap();
    let text = fs::read_to_string(dir.path().join("settings.json")).unwrap();
    assert!(text.contains("\"fontSize\": 13"), "{text}");
    assert!(text.contains("\"theme\": \"system\""), "{text}");
}

#[test]
fn font_size_outside_11_to_20_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    for size in [10, 21] {
        let mut s = Settings::default();
        s.appearance.font_size = size;
        assert!(matches!(
            store.save_settings(s),
            Err(StoreError::Invalid(_))
        ));
    }
    for size in [11, 20] {
        let mut s = Settings::default();
        s.appearance.font_size = size;
        store.save_settings(s).unwrap();
    }
}

#[test]
fn an_empty_font_name_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let mut s = Settings::default();
    s.appearance.mono_font = "  ".into();
    assert!(matches!(
        store.save_settings(s),
        Err(StoreError::Invalid(_))
    ));
    assert!(!dir.path().join("settings.json").exists());
}

#[test]
fn damaged_settings_fall_back_to_defaults_and_are_kept() {
    let newer = r#"{"version": 99, "appearance": {"theme": "dark"}}"#;
    for text in ["{ not json", newer] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, text).unwrap();
        let store = Store::new(dir.path());

        assert_eq!(store.load_settings(), Settings::default(), "{text}");
        assert_eq!(
            fs::read_to_string(dir.path().join("settings.json.damaged")).unwrap(),
            text
        );
        assert!(!path.exists());

        store.save_settings(Settings::default()).unwrap();
        assert_eq!(store.load_settings(), Settings::default());
        assert_eq!(
            fs::read_to_string(dir.path().join("settings.json.damaged")).unwrap(),
            text,
            "the damaged copy is left alone"
        );
    }
}

#[test]
fn damaged_ui_state_falls_back_to_defaults_and_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store
        .save_ui_state("no-project", 0, UiState::default())
        .unwrap();
    let scope_dir = fs::read_dir(dir.path().join("ui"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let file = scope_dir.join("0.json");
    fs::write(&file, r#"{"tabs": 5}"#).unwrap();

    assert_eq!(store.load_ui_state("no-project", 0), UiState::default());
    let mut damaged = file.clone().into_os_string();
    damaged.push(".damaged");
    assert_eq!(fs::read_to_string(damaged).unwrap(), r#"{"tabs": 5}"#);
}

#[test]
fn ui_state_is_kept_per_key() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let a = UiState {
        active_module: Some("requests".into()),
        ..UiState::default()
    };
    let b = UiState {
        active_module: Some("database".into()),
        ..UiState::default()
    };
    store.save_ui_state("no-project", 0, a.clone()).unwrap();
    store.save_ui_state("p-1", 0, b.clone()).unwrap();
    assert_eq!(store.load_ui_state("no-project", 0), a);
    assert_eq!(store.load_ui_state("p-1", 0), b);
    // The scope never appears in a file or folder name.
    for entry in fs::read_dir(dir.path().join("ui")).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        assert!(!name.contains("project") && !name.contains("p-1"), "{name}");
    }
}

#[test]
fn closed_tabs_keep_the_newest_twenty() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let ui = UiState {
        closed_tabs: (0..25).map(tab).collect(),
        ..UiState::default()
    };
    store.save_ui_state("no-project", 0, ui.clone()).unwrap();
    let closed = store.load_ui_state("no-project", 0).closed_tabs;
    assert_eq!(closed.len(), MAX_CLOSED_TABS);
    assert_eq!(closed.first().unwrap().id, "t5");
    assert_eq!(closed.last().unwrap().id, "t24");
}

#[test]
fn settings_that_are_not_utf8_are_set_aside() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let bytes = [0xff, 0xfe, b'{', b'}'];
    fs::write(&path, bytes).unwrap();
    let store = Store::new(dir.path());

    assert_eq!(store.load_settings(), Settings::default());
    assert_eq!(
        fs::read(dir.path().join("settings.json.damaged")).unwrap(),
        bytes
    );
    assert!(!path.exists());
}

#[test]
fn a_write_error_names_the_path_and_keeps_the_cause_separate() {
    let dir = tempfile::tempdir().unwrap();
    // A file where the store expects its folder makes every write fail.
    let blocked = dir.path().join("data");
    fs::write(&blocked, "").unwrap();

    let err = Store::new(&blocked)
        .save_settings(Settings::default())
        .unwrap_err();

    let StoreError::Io { path, .. } = &err else {
        panic!("expected an I/O error, got {err:?}");
    };
    assert_eq!(path, &blocked);
    let cause = err
        .source()
        .expect("the I/O error is the source")
        .to_string();
    // The cause comes through source(), so a printed chain shows it once.
    assert!(
        !err.to_string().contains(&cause),
        "{err} repeats its cause {cause}"
    );
}

#[test]
fn ui_state_is_kept_per_project_and_slot() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let state = |m: &str| UiState {
        active_module: Some(m.into()),
        ..UiState::default()
    };
    store.save_ui_state("p", 0, state("a")).unwrap();
    store.save_ui_state("p", 1, state("b")).unwrap();
    store.save_ui_state("q", 0, state("c")).unwrap();
    assert_eq!(store.load_ui_state("p", 0), state("a"));
    assert_eq!(store.load_ui_state("p", 1), state("b"));
    assert_eq!(store.load_ui_state("q", 0), state("c"));
}

#[test]
fn ui_state_from_plan_3a_still_loads() {
    let state: UiState =
        serde_json::from_str(r#"{"version":1,"activeModule":"requests"}"#).unwrap();
    assert_eq!(state.active_environment, None);
    assert_eq!(state.active_module.as_deref(), Some("requests"));
}

#[test]
fn clearing_a_project_removes_every_slot_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let state = UiState {
        active_environment: Some("staging".into()),
        ..UiState::default()
    };
    store.save_ui_state("p", 0, state.clone()).unwrap();
    store.save_ui_state("p", 1, state.clone()).unwrap();
    store.save_ui_state("q", 0, state.clone()).unwrap();
    store.clear_ui_state("p").unwrap();
    assert_eq!(store.load_ui_state("p", 0), UiState::default());
    assert_eq!(store.load_ui_state("p", 1), UiState::default());
    assert_eq!(store.load_ui_state("q", 0), state);
    // Clearing a project with nothing saved is fine.
    store.clear_ui_state("never-saved").unwrap();
}

#[test]
fn a_tab_keeps_its_environment_across_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let prod = TabState {
        environment: Some(TabEnvironment {
            name: "production".into(),
            production: true,
        }),
        ..tab(1)
    };
    let state = UiState {
        tabs: vec![prod.clone(), tab(2)],
        ..UiState::default()
    };
    store.save_ui_state("p", 0, state).unwrap();
    let loaded = store.load_ui_state("p", 0);
    assert_eq!(loaded.tabs, vec![prod, tab(2)]);
}

#[test]
fn ui_state_without_tab_environments_still_loads() {
    let state: UiState = serde_json::from_str(
        r#"{"version":1,"tabs":[{"id":"t1","tabType":"e2e.tab","title":"Tab 1","params":null}]}"#,
    )
    .unwrap();
    assert_eq!(state.tabs[0].environment, None);
}

#[test]
fn older_settings_get_the_default_vault_minutes() {
    let dir = tempfile::tempdir().unwrap();
    let older = r#"{"version": 1, "appearance": {"theme": "dark"}, "shortcuts": {}}"#;
    fs::write(dir.path().join("settings.json"), older).unwrap();
    let store = Store::new(dir.path());

    let s = store.load_settings();
    assert_eq!(
        s.appearance.theme,
        ThemePreference::Dark,
        "the file was kept"
    );
    assert_eq!(
        s.security,
        Security {
            auto_lock_minutes: Some(15),
            production_minutes: 10,
        }
    );
    assert_eq!(s.security.policy().unwrap(), Policy::default());
    assert!(!dir.path().join("settings.json.damaged").exists());
}

#[test]
fn vault_minutes_outside_their_ranges_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let with = |auto_lock_minutes, production_minutes| Settings {
        security: Security {
            auto_lock_minutes,
            production_minutes,
        },
        ..Settings::default()
    };
    let refused = |s: Settings| match store.save_settings(s) {
        Err(StoreError::Invalid(message)) => message,
        other => panic!("expected Invalid, got {other:?}"),
    };

    for minutes in [0, 241] {
        assert_eq!(
            refused(with(Some(minutes), 10)),
            "Lock when idle must be from 1 to 240 minutes"
        );
    }
    for minutes in [0, 61] {
        assert_eq!(
            refused(with(Some(15), minutes)),
            "Ask again for production must be from 1 to 60 minutes"
        );
    }
    assert!(!dir.path().join("settings.json").exists());

    for (auto_lock, production) in [(None, 10), (Some(1), 1), (Some(240), 60)] {
        let s = with(auto_lock, production);
        store.save_settings(s.clone()).unwrap();
        assert_eq!(store.load_settings(), s);
        assert!(s.security.policy().is_ok());
    }
    assert_eq!(
        with(None, 10).security.policy().unwrap().auto_lock,
        AutoLock::Never
    );
}

/// A jar with one session cookie `name=value` for `api.test`.
fn jar_with(name: &str, value: &str) -> CookieJar {
    let jar = CookieJar::new();
    jar.insert(StoredCookie {
        name: name.into(),
        value: value.into(),
        domain: "api.test".into(),
        path: "/".into(),
        expires: None,
        secure: false,
        http_only: false,
        host_only: true,
    })
    .unwrap();
    jar
}

fn cookie_names(jar: &CookieJar) -> Vec<String> {
    jar.list().into_iter().map(|c| c.name).collect()
}

/// Every file under `dir`.
fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            pending.extend(fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
        } else {
            found.push(path);
        }
    }
    found.sort();
    found
}

#[test]
fn cookies_are_kept_per_project_and_environment() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    // Reading creates nothing.
    assert!(store.load_cookies("p-1", "staging").list().is_empty());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);

    store
        .save_cookies("p-1", "staging", &jar_with("a", "1"))
        .unwrap();
    store
        .save_cookies("p-1", "production", &jar_with("b", "2"))
        .unwrap();
    store.save_cookies("p-1", "", &jar_with("c", "3")).unwrap();
    store
        .save_cookies("q-2", "staging", &jar_with("d", "4"))
        .unwrap();

    let again = Store::new(dir.path());
    assert_eq!(cookie_names(&again.load_cookies("p-1", "staging")), ["a"]);
    assert_eq!(
        cookie_names(&again.load_cookies("p-1", "production")),
        ["b"]
    );
    assert_eq!(cookie_names(&again.load_cookies("p-1", "")), ["c"]);
    assert_eq!(cookie_names(&again.load_cookies("q-2", "staging")), ["d"]);
    assert!(again.load_cookies("q-2", "production").list().is_empty());

    // `cookies/<hash of the project>/<hash of the environment>.json`, and
    // neither name shows in a path.
    let files = files_in(&dir.path().join("cookies"));
    assert_eq!(files.len(), 4, "{files:?}");
    for file in &files {
        let relative = file.strip_prefix(dir.path()).unwrap().to_string_lossy();
        for name in ["p-1", "q-2", "staging", "production"] {
            assert!(!relative.contains(name), "{relative}");
        }
        assert_eq!(file.extension().unwrap(), "json");
    }
    let text = files
        .iter()
        .map(|f| fs::read_to_string(f).unwrap())
        .find(|t| t.contains("\"production\""))
        .unwrap();
    let saved: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(saved["version"], 1);
    assert_eq!(saved["environment"], "production");
    let jar = CookieJar::from_json(&saved["cookies"].to_string()).unwrap();
    assert_eq!(cookie_names(&jar), ["b"]);
}

#[test]
fn damaged_cookie_file_is_set_aside() {
    let newer = r#"{"version": 2, "environment": "staging", "cookies": []}"#;
    let not_a_jar = r#"{"version": 1, "environment": "staging", "cookies": {"a": 1}}"#;
    for text in ["{ not json", newer, not_a_jar] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        store
            .save_cookies("p", "staging", &jar_with("a", "1"))
            .unwrap();
        let files = files_in(&dir.path().join("cookies"));
        let [file] = &files[..] else {
            panic!("expected one cookie file, got {files:?}");
        };
        fs::write(file, text).unwrap();

        assert!(
            store.load_cookies("p", "staging").list().is_empty(),
            "{text}"
        );
        let mut damaged = file.clone().into_os_string();
        damaged.push(".damaged");
        assert_eq!(fs::read_to_string(&damaged).unwrap(), text);
        assert!(!file.exists());

        // The next save starts a new file and leaves the damaged one alone.
        store
            .save_cookies("p", "staging", &jar_with("b", "2"))
            .unwrap();
        assert_eq!(cookie_names(&store.load_cookies("p", "staging")), ["b"]);
        assert_eq!(fs::read_to_string(&damaged).unwrap(), text);
    }
}

#[test]
fn clear_project_removes_ui_state_and_cookies() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let state = UiState {
        active_environment: Some("staging".into()),
        ..UiState::default()
    };
    for scope in ["p", "q"] {
        store.save_ui_state(scope, 0, state.clone()).unwrap();
        store
            .save_cookies(scope, "staging", &jar_with("a", "1"))
            .unwrap();
        store.save_cookies(scope, "", &jar_with("b", "2")).unwrap();
    }

    store.clear_project("p").unwrap();
    assert_eq!(store.load_ui_state("p", 0), UiState::default());
    assert!(store.load_cookies("p", "staging").list().is_empty());
    assert!(store.load_cookies("p", "").list().is_empty());
    // Another project keeps everything.
    assert_eq!(store.load_ui_state("q", 0), state);
    assert_eq!(cookie_names(&store.load_cookies("q", "staging")), ["a"]);
    assert_eq!(cookie_names(&store.load_cookies("q", "")), ["b"]);
    assert_eq!(files_in(&dir.path().join("cookies")).len(), 2);
    // Clearing a project with nothing saved is fine.
    store.clear_project("never-saved").unwrap();
}

#[test]
fn add_local_file_reuses_the_id_for_the_same_path() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let a = store
        .add_local_file("p-1", Path::new("/tmp/a.bin"))
        .unwrap();
    let b = store
        .add_local_file("p-1", Path::new("/tmp/b.bin"))
        .unwrap();
    let again = store
        .add_local_file("p-1", Path::new("/tmp/a.bin"))
        .unwrap();
    assert!(dea_project::is_local_ref(&a), "{a}");
    assert_eq!(a, again);
    assert_ne!(a, b);
    let map = Store::new(dir.path()).local_files("p-1");
    assert_eq!(map.len(), 2);
    assert_eq!(map[&a], PathBuf::from("/tmp/a.bin"));
    assert_eq!(map[&b], PathBuf::from("/tmp/b.bin"));
    assert!(store.local_files("q-2").is_empty());
}

#[test]
fn clear_project_forgets_local_files() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store
        .add_local_file("p-1", Path::new("/tmp/a.bin"))
        .unwrap();
    store
        .add_local_file("q-2", Path::new("/tmp/b.bin"))
        .unwrap();
    store.clear_project("p-1").unwrap();
    assert!(store.local_files("p-1").is_empty());
    assert_eq!(store.local_files("q-2").len(), 1);
}

#[test]
fn damaged_local_files_are_set_aside() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store
        .add_local_file("p-1", Path::new("/tmp/a.bin"))
        .unwrap();
    let file = fs::read_dir(dir.path().join("local-files"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(&file, "not json").unwrap();
    assert!(store.local_files("p-1").is_empty());
    assert!(file.with_extension("json.damaged").exists());
}
