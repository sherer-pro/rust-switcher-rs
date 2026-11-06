//! Opt-in boundary for tests that change the Windows keyboard catalog.

const REQUIRED_ENVIRONMENT: [(&str, &str); 4] = [
    ("RUST_SWITCHER_HOSTED_E2E", "1"),
    ("GITHUB_ACTIONS", "true"),
    ("RUNNER_ENVIRONMENT", "github-hosted"),
    ("RUNNER_OS", "Windows"),
];

fn hosted_fixture_enabled(read: impl Fn(&str) -> Option<String>) -> bool {
    REQUIRED_ENVIRONMENT
        .iter()
        .all(|(name, expected)| read(name).as_deref() == Some(*expected))
}

pub(crate) fn require_hosted_keyboard_fixture() {
    assert!(
        hosted_fixture_enabled(|name| std::env::var(name).ok()),
        "Refusing to change keyboard layouts outside the explicitly enabled disposable GitHub-hosted Windows fixture"
    );
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn keyboard_fixture_rejects_local_and_partial_opt_ins() {
        assert!(!hosted_fixture_enabled(|_| None));
        for (name, _) in REQUIRED_ENVIRONMENT {
            let mut environment: HashMap<_, _> = REQUIRED_ENVIRONMENT.into_iter().collect();
            environment.remove(name);
            assert!(!hosted_fixture_enabled(|key| {
                environment.get(key).map(|value| (*value).to_owned())
            }));
            environment.insert(name, "disabled");
            assert!(!hosted_fixture_enabled(|key| {
                environment.get(key).map(|value| (*value).to_owned())
            }));
        }
    }

    #[test]
    fn keyboard_fixture_accepts_complete_hosted_opt_in() {
        let environment: HashMap<_, _> = REQUIRED_ENVIRONMENT.into_iter().collect();
        assert!(hosted_fixture_enabled(|key| {
            environment.get(key).map(|value| (*value).to_owned())
        }));
    }
}
