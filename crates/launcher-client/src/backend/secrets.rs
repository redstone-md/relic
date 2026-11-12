//! Session tokens, kept in the operating system's credential store so they
//! never sit in a plain file.

use keyring::Entry;

const SERVICE: &str = "relic-launcher";

fn entry(origin: &str) -> keyring::Result<Entry> {
    Entry::new(SERVICE, origin)
}

/// The saved session for a server, if there is one.
pub fn load(origin: &str) -> Option<String> {
    match entry(origin).and_then(|e| e.get_password()) {
        Ok(token) => Some(token),
        Err(keyring::Error::NoEntry) => None,
        Err(err) => {
            tracing::warn!("could not read the saved session: {err}");
            None
        }
    }
}

/// Remembers a session. Failing only means signing in again next time.
pub fn save(origin: &str, token: &str) {
    if let Err(err) = entry(origin).and_then(|e| e.set_password(token)) {
        tracing::warn!("could not save the session: {err}");
    }
}

pub fn forget(origin: &str) {
    match entry(origin).and_then(|e| e.delete_credential()) {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(err) => tracing::warn!("could not remove the saved session: {err}"),
    }
}
