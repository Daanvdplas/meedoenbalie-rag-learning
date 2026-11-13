use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    time::Duration,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Attempt {
    run_id: String,
    config_id: String,
    reserved_at_unix_ms: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    schema_version: u32,
    id: String,
    started_at_unix_ms: u64,
    max_generation_sessions: usize,
    max_seconds: u64,
    ai_judge_calls: usize,
    attempts: Vec<Attempt>,
}

pub struct LiveBudget {
    file: File,
    state: State,
}
impl LiveBudget {
    pub fn open(path: &Path, generations: usize, seconds: u64) -> Result<Self, String> {
        if !(1..=40).contains(&generations) || !(1..=14_400).contains(&seconds) {
            return Err("Live budget requires 1–40 sessions and 1–14400 seconds".into());
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let (mut file, new) = match options.open(path) {
            Ok(file) => (file, true),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(path)
                    .map_err(|_| "Cannot open live budget ledger")?,
                false,
            ),
            Err(_) => return Err("Cannot create live budget ledger".into()),
        };
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err("Another selected live session owns the budget ledger".into());
            }
        }
        #[cfg(not(unix))]
        return Err("Live budget locking requires macOS/Linux".into());
        if file
            .metadata()
            .map_err(|_| "Cannot inspect live budget")?
            .len()
            > 32_768
        {
            return Err("Live budget ledger is oversized".into());
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|_| "Cannot read live budget")?;
        let now = crate::experiment::budget::now_ms()?;
        let state = if new {
            State {
                schema_version: 1,
                id: format!("lab-live-{now}-{}", std::process::id()),
                started_at_unix_ms: now,
                max_generation_sessions: generations,
                max_seconds: seconds,
                ai_judge_calls: 0,
                attempts: Vec::new(),
            }
        } else {
            serde_json::from_slice::<State>(&bytes)
                .map_err(|_| "Invalid or empty live budget ledger")?
        };
        if state.schema_version != 1
            || state.id.is_empty()
            || state.id.len() > 160
            || state.max_generation_sessions != generations
            || state.max_seconds != seconds
            || state.ai_judge_calls != 0
            || state.attempts.len() > generations
            || state.started_at_unix_ms > now
            || state
                .started_at_unix_ms
                .checked_add(seconds * 1000)
                .is_none()
            || state.attempts.iter().enumerate().any(|(index, attempt)| {
                attempt.run_id.is_empty()
                    || attempt.run_id.len() > 160
                    || attempt.config_id.is_empty()
                    || attempt.config_id.len() > 200
                    || attempt.reserved_at_unix_ms < state.started_at_unix_ms
                    || attempt.reserved_at_unix_ms > now
                    || state.attempts[..index]
                        .iter()
                        .any(|earlier| earlier.run_id == attempt.run_id)
            })
        {
            return Err("Invalid live budget identity, history or changed limits".into());
        }
        let mut budget = Self { file, state };
        if new {
            budget.persist()?;
        }
        Ok(budget)
    }
    pub fn id(&self) -> &str {
        &self.state.id
    }
    pub fn used(&self) -> usize {
        self.state.attempts.len()
    }
    pub fn remaining(&self) -> Result<Duration, String> {
        let now = crate::experiment::budget::now_ms()?;
        if now < self.state.started_at_unix_ms
            || self
                .state
                .attempts
                .last()
                .is_some_and(|attempt| now < attempt.reserved_at_unix_ms)
        {
            return Err("Live budget clock moved before its reservation".into());
        }
        let remaining = (self.state.started_at_unix_ms + self.state.max_seconds * 1000)
            .checked_sub(now)
            .filter(|remaining| *remaining > 0)
            .ok_or("runtime: Live experiment timed out")?;
        Ok(Duration::from_millis(remaining.min(120_000)))
    }
    pub fn reserve(&mut self, run_id: &str, config_id: &str) -> Result<Duration, String> {
        let now = crate::experiment::budget::now_ms()?;
        let remaining = self.remaining()?;
        if now < self.state.started_at_unix_ms
            || self
                .state
                .attempts
                .last()
                .is_some_and(|attempt| now < attempt.reserved_at_unix_ms)
            || self.state.attempts.len() >= self.state.max_generation_sessions
            || run_id.is_empty()
            || run_id.len() > 160
            || config_id.is_empty()
            || config_id.len() > 200
            || self
                .state
                .attempts
                .iter()
                .any(|attempt| attempt.run_id == run_id)
        {
            return Err("Live session budget, identity or clock validation failed".into());
        }
        self.state.attempts.push(Attempt {
            run_id: run_id.into(),
            config_id: config_id.into(),
            reserved_at_unix_ms: now,
        });
        self.persist()?;
        Ok(remaining)
    }
    fn persist(&mut self) -> Result<(), String> {
        let bytes = serde_json::to_vec(&self.state).map_err(|_| "Cannot encode live budget")?;
        self.file
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.file.set_len(0))
            .and_then(|_| self.file.write_all(&bytes))
            .and_then(|_| self.file.sync_all())
            .map_err(|_| "Cannot persist live attempt reservation".into())
    }
}

impl Drop for LiveBudget {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            unsafe {
                libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path() -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "coa-live-budget-{}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }
    #[test]
    fn reserves_failed_attempts_before_execution_and_preserves_limits_and_lock() {
        let path = path();
        let mut budget = LiveBudget::open(&path, 2, 1200).unwrap();
        assert!(budget.reserve("run-1", "config-1").unwrap() <= Duration::from_secs(120));
        assert!(LiveBudget::open(&path, 2, 1200).is_err());
        assert!(budget.reserve("run-1", "config-1").is_err());
        drop(budget);
        assert!(LiveBudget::open(&path, 3, 1200).is_err());
        let mut resumed = LiveBudget::open(&path, 2, 1200).unwrap();
        resumed.reserve("run-2", "config-2").unwrap();
        assert!(resumed.reserve("run-3", "config-1").is_err());
        drop(resumed);
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["attempts"].as_array().unwrap().len(), 2);
        assert_eq!(saved["ai_judge_calls"], 0);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn refuses_expired_and_empty_or_corrupt_ledgers_instead_of_resetting() {
        let path = path();
        drop(LiveBudget::open(&path, 1, 1).unwrap());
        let mut saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        saved["started_at_unix_ms"] = 1.into();
        std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
        let mut budget = LiveBudget::open(&path, 1, 1).unwrap();
        assert!(budget.reserve("expired", "config").is_err());
        drop(budget);
        std::fs::write(&path, b"").unwrap();
        assert!(LiveBudget::open(&path, 1, 1).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
