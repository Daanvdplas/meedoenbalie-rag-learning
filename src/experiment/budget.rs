use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerState {
    pub schema_version: u32,
    pub id: String,
    pub started_at_unix_ms: u64,
    pub max_generation_sessions: usize,
    pub max_grading_sessions: usize,
    pub max_seconds: u64,
    pub generation_sessions_used: usize,
    pub grading_sessions_used: usize,
}
pub struct Ledger {
    file: File,
    pub state: LedgerState,
}
impl Ledger {
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let (mut file, is_new) = match options.open(path) {
            Ok(file) => (file, true),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(path)
                    .map_err(|_| "Cannot open experiment budget ledger")?,
                false,
            ),
            Err(_) => return Err("Cannot create experiment budget ledger".into()),
        };
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err("Another experiment owns the budget ledger".into());
            }
        }
        #[cfg(not(unix))]
        return Err("Budget locking requires macOS/Linux".into());
        if file
            .metadata()
            .map_err(|_| "Cannot inspect budget ledger")?
            .len()
            > 4096
        {
            return Err("Budget ledger is oversized".into());
        }
        let mut text = String::new();
        file.read_to_string(&mut text)
            .map_err(|_| "Cannot read budget ledger")?;
        let now = now_ms()?;
        let state = if is_new {
            LedgerState {
                schema_version: 1,
                id: format!("budget-{now}-{}", std::process::id()),
                started_at_unix_ms: now,
                max_generation_sessions: 40,
                max_grading_sessions: 56,
                max_seconds: 14400,
                generation_sessions_used: 0,
                grading_sessions_used: 0,
            }
        } else {
            serde_json::from_str::<LedgerState>(&text)
                .map_err(|_| "Invalid budget ledger schema")?
        };
        if state.schema_version != 1
            || state.max_generation_sessions != 40
            || state.max_grading_sessions != 56
            || state.max_seconds != 14400
            || state.generation_sessions_used > 40
            || state.grading_sessions_used > 56
            || state.started_at_unix_ms > now
        {
            return Err("Invalid or unsupported budget ledger limits".into());
        }
        let mut ledger = Self { file, state };
        if is_new {
            ledger.persist()?;
        }
        Ok(ledger)
    }
    pub fn remaining(&self) -> Result<std::time::Duration, String> {
        let elapsed = now_ms()?
            .checked_sub(self.state.started_at_unix_ms)
            .ok_or("System clock moved before experiment start")?;
        let remaining = (self.state.max_seconds * 1000)
            .checked_sub(elapsed)
            .filter(|v| *v > 0)
            .ok_or("Global experiment time budget exhausted")?;
        Ok(std::time::Duration::from_millis(remaining))
    }
    fn persist(&mut self) -> Result<(), String> {
        let bytes = serde_json::to_vec(&self.state).map_err(|_| "Cannot encode budget ledger")?;
        self.file
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.file.set_len(0))
            .and_then(|_| self.file.write_all(&bytes))
            .and_then(|_| self.file.sync_all())
            .map_err(|_| "Cannot persist budget reservation".into())
    }
    pub fn reserve_generation(&mut self) -> Result<(), String> {
        self.remaining()?;
        if self.state.generation_sessions_used >= self.state.max_generation_sessions {
            return Err("Global generation-session budget exhausted".into());
        }
        self.state.generation_sessions_used += 1;
        self.persist()
    }
    pub fn reserve_grading(&mut self) -> Result<(), String> {
        self.remaining()?;
        if self.state.grading_sessions_used >= self.state.max_grading_sessions {
            return Err("Global grading-session budget exhausted".into());
        }
        self.state.grading_sessions_used += 1;
        self.persist()
    }
}

pub fn now_ms() -> Result<u64, String> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "System clock unavailable")?
        .as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_empty_ledger_is_corruption_not_a_fresh_budget() {
        let path = std::env::temp_dir().join(format!("coa-empty-ledger-{}", now_ms().unwrap()));
        std::fs::write(&path, b"").unwrap();
        assert!(Ledger::open(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn ledger_persists_failed_attempts_and_locks_concurrent_sessions() {
        let path = std::env::temp_dir().join(format!(
            "coa-ledger-{}.json",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut ledger = Ledger::open(&path).unwrap();
        ledger.reserve_generation().unwrap();
        ledger.reserve_grading().unwrap();
        assert!(Ledger::open(&path).is_err());
        drop(ledger);
        let mut ledger = Ledger::open(&path).unwrap();
        assert_eq!(ledger.state.generation_sessions_used, 1);
        assert_eq!(ledger.state.grading_sessions_used, 1);
        ledger.state.started_at_unix_ms = now_ms().unwrap() - 14_400_001;
        assert!(ledger.remaining().is_err());
        ledger.state.started_at_unix_ms = now_ms().unwrap();
        ledger.state.generation_sessions_used = 40;
        assert!(ledger.reserve_generation().is_err());
        drop(ledger);
        std::fs::remove_file(path).unwrap();
    }
}
