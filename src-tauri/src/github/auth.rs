use crate::error::GhResult;
use keyring::Entry;

const SERVICE: &str = "modpacksync";
/// Single account slot: the plan calls for one host PAT used across whatever
/// repos the host publishes to, entered once in Settings — not a token per repo.
const ACCOUNT: &str = "github-token";

pub fn store_token(token: &str) -> GhResult<()> {
    let entry = Entry::new(SERVICE, ACCOUNT)?;
    entry.set_password(token)?;
    Ok(())
}

pub fn get_token() -> GhResult<Option<String>> {
    let entry = Entry::new(SERVICE, ACCOUNT)?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn delete_token() -> GhResult<()> {
    let entry = Entry::new(SERVICE, ACCOUNT)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
