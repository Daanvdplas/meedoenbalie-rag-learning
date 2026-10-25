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
    #[serde(default)]
    pub window_extensions: Vec<WindowExtension>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowExtension {
    pub previous_deadline_unix_ms: u64,
    pub new_deadline_unix_ms: u64,
    pub authorized_at_unix_ms: u64,
    pub authorization: String,
    pub reason: String,
    pub generation_sessions_used: usize,
    pub grading_sessions_used: usize,
}
impl LedgerState {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.id.is_empty()
            || self.id.len() > 160
            || self.max_generation_sessions != 40
            || self.max_grading_sessions != 56
            || self.max_seconds != 14400
            || self.generation_sessions_used > 40
            || self.grading_sessions_used > 56
            || self.window_extensions.len() > 8
        {
            return Err("Invalid or unsupported budget ledger limits".into());
        }
        let mut deadline = self
            .started_at_unix_ms
            .checked_add(14_400_000)
            .ok_or("Budget deadline overflow")?;
        let mut generation = 0;
        let mut grading = 0;
        for extension in &self.window_extensions {
            if extension.previous_deadline_unix_ms != deadline
                || extension.authorized_at_unix_ms < deadline
                || extension.new_deadline_unix_ms
                    != extension
                        .authorized_at_unix_ms
                        .checked_add(14_400_000)
                        .ok_or("Budget window overflow")?
                || extension.authorization != "product_lead_explicit"
                || extension.reason != "voortzetting na verlopen venster, taaklead akkoord"
                || extension.generation_sessions_used < generation
                || extension.grading_sessions_used < grading
                || extension.generation_sessions_used > self.generation_sessions_used
                || extension.grading_sessions_used > self.grading_sessions_used
            {
                return Err("Invalid budget continuation audit".into());
            }
            deadline = extension.new_deadline_unix_ms;
            generation = extension.generation_sessions_used;
            grading = extension.grading_sessions_used;
        }
        Ok(())
    }
    pub fn deadline_unix_ms(&self) -> Result<u64, String> {
        self.validate()?;
        Ok(self
            .window_extensions
            .last()
            .map(|w| w.new_deadline_unix_ms)
            .unwrap_or(self.started_at_unix_ms + 14_400_000))
    }
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
                window_extensions: vec![],
            }
        } else {
            serde_json::from_str::<LedgerState>(&text)
                .map_err(|_| "Invalid budget ledger schema")?
        };
        state.validate()?;
        if state.started_at_unix_ms > now
            || state
                .window_extensions
                .last()
                .is_some_and(|w| w.authorized_at_unix_ms > now)
        {
            return Err("System clock moved before budget authorization".into());
        }
        let mut ledger = Self { file, state };
        if is_new {
            ledger.persist()?;
        }
        Ok(ledger)
    }
    pub fn remaining(&self) -> Result<std::time::Duration, String> {
        let now = now_ms()?;
        if now < self.state.started_at_unix_ms {
            return Err("System clock moved before experiment start".into());
        }
        let remaining = self
            .state
            .deadline_unix_ms()?
            .checked_sub(now)
            .filter(|v| *v > 0)
            .ok_or("Global experiment time budget exhausted")?;
        Ok(std::time::Duration::from_millis(remaining))
    }
    /// Explicit product-lead continuation; never called automatically by generation/grading.
    pub fn extend_window(&mut self, seconds: u64) -> Result<(), String> {
        let now = now_ms()?;
        let previous = self.state.deadline_unix_ms()?;
        if seconds != 14400 || now < previous || self.state.window_extensions.len() >= 8 {
            return Err("Only an expired budget may receive an explicitly authorized four-hour continuation".into());
        }
        self.state.window_extensions.push(WindowExtension {
            previous_deadline_unix_ms: previous,
            new_deadline_unix_ms: now
                .checked_add(14_400_000)
                .ok_or("Budget window overflow")?,
            authorized_at_unix_ms: now,
            authorization: "product_lead_explicit".into(),
            reason: "voortzetting na verlopen venster, taaklead akkoord".into(),
            generation_sessions_used: self.state.generation_sessions_used,
            grading_sessions_used: self.state.grading_sessions_used,
        });
        self.state.validate()?;
        self.persist()
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
    fn explicit_extension_preserves_original_deadline_and_consumed_sessions() {
        let path = std::env::temp_dir().join(format!("coa-window-test-{}", now_ms().unwrap()));
        let mut ledger = Ledger::open(&path).unwrap();
        ledger.reserve_generation().unwrap();
        ledger.reserve_grading().unwrap();
        ledger.state.started_at_unix_ms = now_ms().unwrap() - 14_400_100;
        ledger.persist().unwrap();
        let original = ledger.state.started_at_unix_ms;
        let id = ledger.state.id.clone();
        assert!(ledger.remaining().is_err());
        ledger.extend_window(14400).unwrap();
        assert!(ledger.remaining().is_ok());
        assert_eq!(ledger.state.started_at_unix_ms, original);
        assert_eq!(ledger.state.id, id);
        assert_eq!(ledger.state.generation_sessions_used, 1);
        assert_eq!(ledger.state.grading_sessions_used, 1);
        assert_eq!(
            ledger.state.window_extensions[0].previous_deadline_unix_ms,
            original + 14_400_000
        );
        assert!(ledger.extend_window(14400).is_err());
        drop(ledger);
        let mut ledger = Ledger::open(&path).unwrap();
        assert_eq!(ledger.state.window_extensions.len(), 1);
        ledger.reserve_generation().unwrap();
        assert_eq!(ledger.state.generation_sessions_used, 2);
        drop(ledger);
        std::fs::remove_file(path).unwrap();
    }
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
