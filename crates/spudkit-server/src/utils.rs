pub fn generate_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Resolve a command to its full path inside the container.
/// If the command doesn't start with `/`, prepend `/app/bin/`.
pub fn resolve_cmd(cmd: &[String]) -> Vec<String> {
    let mut resolved = cmd.to_vec();
    if let Some(first) = resolved.first_mut()
        && !first.starts_with('/')
    {
        *first = format!("/app/bin/{first}");
    }
    resolved
}
