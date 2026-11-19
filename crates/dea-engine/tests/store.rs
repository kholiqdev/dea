use std::collections::BTreeMap;
use std::fs;

use dea_engine::store::{
    Appearance, MAX_CLOSED_TABS, Settings, SidebarState, Store, StoreError, TabState,
    ThemePreference, UiState,
};

fn tab(n: usize) -> TabState {
    TabState {
        id: format!("t{n}"),
        tab_type: "e2e.tab".into(),
        title: format!("Tab {n}"),
        params: serde_json::json!({ "n": n }),
    }
}

#[test]
fn missing_files_give_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let s = store.settings();
    assert_eq!(s.appearance.theme, ThemePreference::System);
    assert_eq!(s.appearance.ui_font, "IBM Plex Sans");
    assert_eq!(s.appearance.mono_font, "IBM Plex Mono");
    assert_eq!(s.appearance.font_size, 13);
    assert!(s.shortcuts.is_empty());
    assert_eq!(store.ui_state("no-project"), UiState::default());
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
    store.save_settings(&settings).unwrap();
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
    store.save_ui_state("no-project", &ui).unwrap();

    let again = Store::new(dir.path());
    assert_eq!(again.settings(), settings);
    assert_eq!(again.ui_state("no-project"), ui);
}

#[test]
fn files_use_camel_case_names() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.save_settings(&Settings::default()).unwrap();
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
            store.save_settings(&s),
            Err(StoreError::Invalid(_))
        ));
    }
    for size in [11, 20] {
        let mut s = Settings::default();
        s.appearance.font_size = size;
        store.save_settings(&s).unwrap();
    }
}

#[test]
fn an_empty_font_name_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let mut s = Settings::default();
    s.appearance.mono_font = "  ".into();
    assert!(matches!(
        store.save_settings(&s),
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

        assert_eq!(store.settings(), Settings::default(), "{text}");
        assert_eq!(
            fs::read_to_string(dir.path().join("settings.json.damaged")).unwrap(),
            text
        );
        assert!(!path.exists());

        store.save_settings(&Settings::default()).unwrap();
        assert_eq!(store.settings(), Settings::default());
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
        .save_ui_state("no-project", &UiState::default())
        .unwrap();
    let file = fs::read_dir(dir.path().join("ui"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(&file, r#"{"tabs": 5}"#).unwrap();

    assert_eq!(store.ui_state("no-project"), UiState::default());
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
    store.save_ui_state("no-project", &a).unwrap();
    store.save_ui_state("p-1", &b).unwrap();
    assert_eq!(store.ui_state("no-project"), a);
    assert_eq!(store.ui_state("p-1"), b);
    // The key never appears in a file name.
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
    store.save_ui_state("no-project", &ui).unwrap();
    let closed = store.ui_state("no-project").closed_tabs;
    assert_eq!(closed.len(), MAX_CLOSED_TABS);
    assert_eq!(closed.first().unwrap().id, "t5");
    assert_eq!(closed.last().unwrap().id, "t24");
}
