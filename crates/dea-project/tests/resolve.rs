use std::collections::HashMap;

use dea_project::{
    DynamicSource, ResolveContext, ResolveError, Scope, ScopeKind, SecretLookup, SecretLookupError,
    SystemDynamicSource, ValueSource, Variable, resolve,
};

fn var(name: &str, value: &str) -> Variable {
    Variable {
        name: name.into(),
        value: value.into(),
        description: None,
    }
}

fn scope(kind: ScopeKind, label: &str, vars: &[(&str, &str)]) -> Scope {
    Scope {
        kind,
        label: label.into(),
        variables: vars.iter().map(|(n, v)| var(n, v)).collect(),
    }
}

#[derive(Default)]
struct FakeVault {
    secrets: HashMap<(String, String), String>,
    error: Option<SecretLookupError>,
}

impl FakeVault {
    fn with(environment: &str, name: &str, value: &str) -> Self {
        let mut vault = Self::default();
        vault
            .secrets
            .insert((environment.into(), name.into()), value.into());
        vault
    }
}

impl SecretLookup for FakeVault {
    fn lookup(&self, environment: &str, name: &str) -> Result<Option<String>, SecretLookupError> {
        if let Some(e) = &self.error {
            return Err(e.clone());
        }
        Ok(self
            .secrets
            .get(&(environment.to_string(), name.to_string()))
            .cloned())
    }
}

struct FakeDynamic;

impl DynamicSource for FakeDynamic {
    fn guid(&mut self) -> String {
        "g-1".into()
    }
    fn unix_timestamp(&mut self) -> i64 {
        1_763_371_800
    }
    fn iso_timestamp(&mut self) -> String {
        "2025-11-17T09:30:00Z".into()
    }
    fn random_int(&mut self) -> u32 {
        7
    }
}

fn run(
    template: &str,
    scopes: &[Scope],
    vault: &FakeVault,
) -> Result<dea_project::Resolution, ResolveError> {
    let mut dynamic = FakeDynamic;
    let mut ctx = ResolveContext {
        scopes,
        environment: "staging",
        secrets: vault,
        dynamic: &mut dynamic,
    };
    resolve(template, &mut ctx)
}

fn staging() -> Vec<Scope> {
    vec![scope(
        ScopeKind::Environment,
        "staging",
        &[
            ("baseUrl", "https://staging.harbor.test"),
            ("token", "{{vault:access_token}}"),
        ],
    )]
}

#[test]
fn precedence_request_folder_collection_environment_global() {
    use ScopeKind::*;
    let cases: [(&[ScopeKind], ScopeKind); 6] = [
        (&[Global], Global),
        (&[Global, Environment], Environment),
        (&[Environment, Collection], Collection),
        (&[Collection, Folder], Folder),
        (&[Folder, Request], Request),
        (&[Global, Environment, Collection, Folder, Request], Request),
    ];
    for (present, winner) in cases {
        // Pass scopes from least to most specific to prove the order comes
        // from the kind, not from the slice.
        let scopes: Vec<Scope> = present
            .iter()
            .map(|k| scope(*k, &format!("{k:?}"), &[("x", &format!("{k:?}"))]))
            .collect();
        let r = run("{{x}}", &scopes, &FakeVault::default()).unwrap();
        assert_eq!(r.text, format!("{winner:?}"), "present: {present:?}");
    }
}

#[test]
fn within_one_kind_the_first_scope_wins() {
    let scopes = [
        scope(ScopeKind::Folder, "shipments/by-id", &[("x", "inner")]),
        scope(ScopeKind::Folder, "shipments", &[("x", "outer")]),
    ];
    let r = run("{{x}}", &scopes, &FakeVault::default()).unwrap();
    assert_eq!(r.text, "inner");
}

#[test]
fn source_is_reported() {
    let r = run("{{baseUrl}}/v1", &staging(), &FakeVault::default()).unwrap();
    assert_eq!(r.text, "https://staging.harbor.test/v1");
    assert!(matches!(
        &r.used[0].source,
        ValueSource::Scope { kind: ScopeKind::Environment, label } if label == "staging"
    ));
    assert!(!r.used[0].secret);
}

#[test]
fn spaces_inside_braces_are_allowed() {
    let r = run("{{ baseUrl }}", &staging(), &FakeVault::default()).unwrap();
    assert_eq!(r.text, "https://staging.harbor.test");
}

#[test]
fn vault_reference_uses_secret_lookup_and_is_redacted() {
    let vault = FakeVault::with("staging", "access_token", "s3cr3t");
    let r = run("Bearer {{token}}", &staging(), &vault).unwrap();
    assert_eq!(r.text, "Bearer s3cr3t");
    assert_eq!(r.redacted_text, "Bearer [secret:access_token]");
    assert!(r.used.iter().any(|u| u.secret));
    assert!(r.used.iter().any(|u| matches!(
        &u.source,
        ValueSource::Vault { environment, name } if environment == "staging" && name == "access_token"
    )));
    // The variable that holds the secret is listed first and marked secret.
    assert_eq!(r.used[0].name, "token");
    assert!(r.used[0].secret);
}

#[test]
fn secret_values_are_not_expanded() {
    let vault = FakeVault::with("staging", "access_token", "a{{baseUrl}}b");
    let r = run("{{vault:access_token}}", &staging(), &vault).unwrap();
    assert_eq!(r.text, "a{{baseUrl}}b");
    assert!(r.unresolved.is_empty());
}

#[test]
fn missing_secret_is_unresolved() {
    let r = run("Bearer {{token}}", &staging(), &FakeVault::default()).unwrap();
    assert_eq!(r.text, "Bearer {{vault:access_token}}");
    assert_eq!(r.unresolved, ["vault:access_token"]);
}

#[test]
fn unresolved_variables_are_listed_and_left_intact() {
    let r = run(
        "{{missing}} and {{#each items}} and {{missing}}",
        &staging(),
        &FakeVault::default(),
    )
    .unwrap();
    assert_eq!(r.text, "{{missing}} and {{#each items}} and {{missing}}");
    assert_eq!(r.redacted_text, r.text);
    assert_eq!(r.unresolved, ["missing", "#each items"]);
}

#[test]
fn nested_references_resolve() {
    let scopes = [scope(
        ScopeKind::Environment,
        "staging",
        &[
            ("host", "staging.harbor.test"),
            ("baseUrl", "https://{{host}}"),
            ("shipments", "{{baseUrl}}/shipments"),
        ],
    )];
    let r = run("{{shipments}}/1", &scopes, &FakeVault::default()).unwrap();
    assert_eq!(r.text, "https://staging.harbor.test/shipments/1");
}

#[test]
fn dynamic_variables_use_source() {
    let r = run(
        "{{$guid}}/{{$randomInt}}/{{$timestamp}}/{{$isoTimestamp}}",
        &[],
        &FakeVault::default(),
    )
    .unwrap();
    assert_eq!(r.text, "g-1/7/1763371800/2025-11-17T09:30:00Z");
    assert!(matches!(&r.used[0].source, ValueSource::Dynamic(n) if n == "$guid"));
}

#[test]
fn unknown_dynamic_variable_is_unresolved() {
    let r = run("{{$nope}}", &[], &FakeVault::default()).unwrap();
    assert_eq!(r.text, "{{$nope}}");
    assert_eq!(r.unresolved, ["$nope"]);
}

#[test]
fn cycle_is_an_error() {
    let scopes = [scope(
        ScopeKind::Environment,
        "staging",
        &[("a", "{{b}}"), ("b", "{{a}}")],
    )];
    match run("{{a}}", &scopes, &FakeVault::default()) {
        Err(ResolveError::Cycle(name)) => assert_eq!(name, "a"),
        other => panic!("expected a cycle, got {other:?}"),
    }
}

#[test]
fn references_deeper_than_ten_levels_are_an_error() {
    let vars: Vec<(String, String)> = (0..12)
        .map(|i| (format!("v{i}"), format!("{{{{v{}}}}}", i + 1)))
        .collect();
    let refs: Vec<(&str, &str)> = vars.iter().map(|(n, v)| (n.as_str(), v.as_str())).collect();
    let scopes = [scope(ScopeKind::Environment, "staging", &refs)];
    assert!(matches!(
        run("{{v0}}", &scopes, &FakeVault::default()),
        Err(ResolveError::TooDeep(_))
    ));
}

#[test]
fn locked_vault_propagates() {
    let vault = FakeVault {
        error: Some(SecretLookupError::Locked),
        ..Default::default()
    };
    match run("Bearer {{token}}", &staging(), &vault) {
        Err(ResolveError::Secret { name, error }) => {
            assert_eq!(name, "access_token");
            assert_eq!(error, SecretLookupError::Locked);
        }
        other => panic!("expected a secret error, got {other:?}"),
    }
}

#[test]
fn system_dynamic_source_values_have_the_right_shape() {
    let mut s = SystemDynamicSource;
    let guid = s.guid();
    assert_eq!(guid.len(), 36);
    assert_eq!(&guid[14..15], "4", "{guid}");
    assert!(s.unix_timestamp() > 1_700_000_000);
    let iso = s.iso_timestamp();
    assert!(iso.ends_with('Z') && iso.contains('T'), "{iso}");
    for _ in 0..200 {
        assert!(s.random_int() <= 1000);
    }
}

#[test]
fn exponential_references_stop_at_a_size_limit() {
    // Each level refers to the next ten times: 10^9 copies at the bottom.
    let mut vars: Vec<(String, String)> = (0..9)
        .map(|i| (format!("v{i}"), format!("{{{{v{}}}}}", i + 1).repeat(10)))
        .collect();
    vars.push(("v9".into(), "x".repeat(100)));
    let refs: Vec<(&str, &str)> = vars.iter().map(|(n, v)| (n.as_str(), v.as_str())).collect();
    let scopes = [scope(ScopeKind::Environment, "staging", &refs)];
    let started = std::time::Instant::now();
    let result = run("{{v0}}", &scopes, &FakeVault::default());
    assert!(
        matches!(result, Err(ResolveError::TooLarge(_))),
        "{result:?}"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}

struct CountingVault(std::cell::Cell<usize>);

impl SecretLookup for CountingVault {
    fn lookup(&self, _environment: &str, _name: &str) -> Result<Option<String>, SecretLookupError> {
        self.0.set(self.0.get() + 1);
        Ok(Some("s3cr3t".into()))
    }
}

#[test]
fn each_secret_is_looked_up_once_per_resolution() {
    let vault = CountingVault(std::cell::Cell::new(0));
    let scopes = staging();
    let mut dynamic = FakeDynamic;
    let mut ctx = ResolveContext {
        scopes: &scopes,
        environment: "staging",
        secrets: &vault,
        dynamic: &mut dynamic,
    };
    let r = resolve("{{token}} {{token}} {{vault:access_token}}", &mut ctx).unwrap();
    assert_eq!(r.text, "s3cr3t s3cr3t s3cr3t");
    assert_eq!(vault.0.get(), 1);
}
