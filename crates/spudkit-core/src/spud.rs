/// A spud — a named app that maps to a `spud-{name}` Docker image.
#[derive(Clone, Debug)]
pub struct Spud {
    name: String,
}

impl Spud {
    /// Create a spud from a short name (e.g., "hello-world").
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }

    /// The short display name (e.g., "hello-world").
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The Docker image name (e.g., "spud-hello-world").
    pub fn image_name(&self) -> String {
        format!("spud-{}", self.name)
    }

    /// The Unix socket path for this app.
    pub fn socket_path(&self) -> String {
        format!("/tmp/spudkit-{}.sock", self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_name_has_prefix() {
        let spud = Spud::new("hello-world");
        assert_eq!(spud.image_name(), "spud-hello-world");
    }

    #[test]
    fn socket_path_uses_name() {
        let spud = Spud::new("hello-world");
        assert_eq!(spud.socket_path(), "/tmp/spudkit-hello-world.sock");
    }

    #[test]
    fn name_returns_short_name() {
        let spud = Spud::new("hello-world");
        assert_eq!(spud.name(), "hello-world");
    }
}
