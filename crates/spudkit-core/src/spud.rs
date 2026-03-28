/// A spud — a named app that maps to a `spud-{name}` Docker image.
#[derive(Clone, Debug)]
pub struct Spud {
    name: String,
}

impl Spud {
    pub fn new(name: &str) -> anyhow::Result<Self> {
        if name.is_empty() || name.contains('/') || name.contains("..") {
            anyhow::bail!("invalid spud name: {name:?}");
        }
        Ok(Self {
            name: name.to_string(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn socket_path(&self) -> String {
        format!("/tmp/spudkit-{}.sock", self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn socket_path_uses_name() {
        let spud = Spud::new("hello-world").unwrap();
        assert_eq!(spud.socket_path(), "/tmp/spudkit-hello-world.sock");
    }

    #[test]
    fn name_returns_short_name() {
        let spud = Spud::new("hello-world").unwrap();
        assert_eq!(spud.name(), "hello-world");
    }

    #[rstest]
    #[case::empty("")]
    #[case::slash("foo/bar")]
    #[case::dotdot("..")]
    #[case::traversal("../../etc")]
    fn rejects_invalid_names(#[case] name: &str) {
        assert!(Spud::new(name).is_err(), "should reject: {name:?}");
    }
}
