use std::path::Path;

use dea_project::{
    ProjectError, check_relative_path, is_production_name, safe_file_stem, unique_file_stem,
};

#[test]
fn safe_file_stem_rules() {
    assert_eq!(safe_file_stem("Get: shipment?"), "get-shipment");
    assert_eq!(safe_file_stem("  ..trail.  "), "trail");
    assert_eq!(safe_file_stem("CON"), "con-1");
    assert_eq!(safe_file_stem("com7"), "com7-1");
    assert_eq!(safe_file_stem("***"), "untitled");
    assert_eq!(safe_file_stem(&"a".repeat(300)).chars().count(), 80);
}

#[test]
fn safe_file_stem_keeps_unicode_letters() {
    assert_eq!(safe_file_stem("Produksi 🚀"), "produksi");
    assert_eq!(safe_file_stem("环境"), "环境");
}

#[test]
fn safe_file_stem_guards_reserved_names_before_a_dot() {
    // Windows also reserves "CON.anything".
    assert_eq!(safe_file_stem("con.old"), "con-1.old");
    assert_eq!(safe_file_stem("console"), "console");
}

#[test]
fn safe_file_stem_never_ends_with_dot_after_truncation() {
    let name = format!("{}.{}", "a".repeat(79), "b".repeat(10));
    let stem = safe_file_stem(&name);
    assert!(!stem.ends_with('.'), "{stem}");
    assert!(stem.chars().count() <= 80);
}

#[test]
fn unique_file_stem_is_case_insensitive() {
    assert_eq!(
        unique_file_stem(&["staging".into()], "Staging"),
        "staging-2"
    );
    assert_eq!(
        unique_file_stem(&["staging".into(), "staging-2".into()], "staging"),
        "staging-3"
    );
    assert_eq!(
        unique_file_stem(&["STAGING".into()], "staging"),
        "staging-2"
    );
    assert_eq!(unique_file_stem(&[], "Local"), "local");
}

#[test]
fn relative_path_over_200_chars_is_rejected() {
    assert!(matches!(
        check_relative_path(Path::new(&"a/".repeat(101))),
        Err(ProjectError::PathTooLong(_))
    ));
    assert!(check_relative_path(Path::new(&"a".repeat(200))).is_ok());
}

#[test]
fn is_production_name_matches_prod_variants() {
    for n in ["prod", "PRODUCTION", "Prd"] {
        assert!(is_production_name(n), "{n}");
    }
    for n in ["staging", "product", "preprod"] {
        assert!(!is_production_name(n), "{n}");
    }
}
