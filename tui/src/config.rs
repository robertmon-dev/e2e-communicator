use std::path::PathBuf;

const SOCKET_FILE_NAME: &str = "e2e-communicator-client.sock";

pub fn resolve_socket_path(flag: Option<PathBuf>, env: Option<String>) -> PathBuf {
    flag.or_else(|| env.map(PathBuf::from)).unwrap_or_else(|| {
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(SOCKET_FILE_NAME)
    })
}

pub fn resolve_token(flag: Option<String>, env: Option<String>) -> Option<String> {
    flag.or(env)
}
