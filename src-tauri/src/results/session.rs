//! Exclusive ownership of one instance's temporary results. All callers cooperate through OS file locks.
// Core
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const COORDINATOR: &str = ".coordinator.lock";
const LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const CLEANUP_ENTRIES: usize = 4096;
static NEXT_SESSION: AtomicU64 = AtomicU64::new(0);

pub struct ResultSession {
    root: PathBuf,
    dir: PathBuf,
    lease_path: PathBuf,
    lease: Option<File>,
    closed: bool,
}

impl ResultSession {
    pub fn open(root: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&root)?;
        if !fs::symlink_metadata(&root)?.file_type().is_dir() {
            return Err(io::Error::other("result cache must be a directory, not a link"));
        }
        let _coordinator = coordinate(&root, LOCK_TIMEOUT)?;
        // Cleanup is best effort: uncertain ownership or an inaccessible old entry must preserve its files.
        let _ = reap_stale(&root, CLEANUP_ENTRIES);
        for _ in 0..64 {
            let name = session_name();
            let dir = root.join(&name);
            let lease_path = root.join(format!("{name}.lock"));
            match fs::create_dir(&dir) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
            let lease = OpenOptions::new().read(true).write(true).create_new(true).open(&lease_path);
            let lease = match lease {
                Ok(lease) => lease,
                Err(error) => {
                    let _ = fs::remove_dir(&dir);
                    if error.kind() == io::ErrorKind::AlreadyExists {
                        continue;
                    }
                    return Err(error);
                }
            };
            // Creation and cleanup hold the coordinator, so nobody can reap this session before its lease locks.
            lease.try_lock().map_err(io::Error::from)?;
            return Ok(Self { root, dir, lease_path, lease: Some(lease), closed: false });
        }
        Err(io::Error::other("could not allocate an exclusive result session"))
    }

    pub fn dir(&self) -> io::Result<&Path> {
        if self.closed {
            return Err(io::Error::new(io::ErrorKind::NotConnected, "result session has closed"));
        }
        Ok(&self.dir)
    }

    pub fn clear(&mut self) -> io::Result<()> {
        self.closed = true;
        if self.lease.is_none() {
            return Ok(());
        }
        let _coordinator = coordinate(&self.root, LOCK_TIMEOUT)?;
        // No other opener/cleaner can intervene while the coordinator is held. Close before deleting on Windows.
        drop(self.lease.take());
        remove_directory(&self.dir)?;
        remove_file(&self.lease_path)
    }
}

impl Drop for ResultSession {
    fn drop(&mut self) {
        let _ = self.clear();
    }
}

fn session_name() -> String {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let sequence = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    format!("session-{}-{stamp}-{sequence}", std::process::id())
}

fn open_lock(path: &Path, create: bool) -> io::Result<File> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(io::Error::other("result lease must be a regular file, not a link"));
        }
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    OpenOptions::new().read(true).write(true).create(create).truncate(false).open(path)
}

#[derive(Debug)]
struct Coordinator(File);

impl Drop for Coordinator {
    fn drop(&mut self) {
        // A concurrent subprocess spawn may briefly inherit the descriptor before exec closes it.
        // Explicit unlock ends this critical section even while that inherited descriptor is still open.
        let _ = self.0.unlock();
    }
}

fn coordinate(root: &Path, timeout: Duration) -> io::Result<Coordinator> {
    let file = open_lock(&root.join(COORDINATOR), true)?;
    let start = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(Coordinator(file)),
            Err(TryLockError::WouldBlock) if start.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(TryLockError::WouldBlock) => {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "result cache coordination is busy"));
            }
            Err(TryLockError::Error(error)) => return Err(error),
        }
    }
}

fn is_session_name(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix("session-").filter(|_| name.len() <= 80) else { return false };
    let parts: Vec<_> = suffix.split('-').collect();
    parts.len() == 3 && parts.iter().all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

/// The caller holds the coordinator throughout collection, including lease closure and deletion.
fn reap_stale(root: &Path, budget: usize) -> io::Result<()> {
    for entry in fs::read_dir(root)?.take(budget).flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() && is_session_name(name) {
            let _ = reap_session(root, name);
        } else if kind.is_file()
            && let Some(session) = name.strip_suffix(".lock").filter(|name| is_session_name(name))
            && fs::symlink_metadata(root.join(session)).is_err_and(|e| e.kind() == io::ErrorKind::NotFound)
        {
            // An interrupted cleanup may leave only a lease file.
            let _ = reap_session(root, session);
        }
    }
    Ok(())
}

fn reap_session(root: &Path, name: &str) -> io::Result<()> {
    let dir = root.join(name);
    match fs::symlink_metadata(&dir) {
        Ok(metadata) if !metadata.file_type().is_dir() => return Ok(()),
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let lease_path = root.join(format!("{name}.lock"));
    let lease = match open_lock(&lease_path, false) {
        Ok(lease) => {
            match lease.try_lock() {
                Ok(()) => {}
                Err(TryLockError::WouldBlock) => return Ok(()),
                Err(TryLockError::Error(error)) => return Err(error),
            }
            Some(lease)
        }
        // A creator that died before making its lease cannot still hold the coordinator we acquired.
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    drop(lease);
    remove_directory(&dir)?;
    remove_file(&lease_path)
}

fn remove_directory(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn remove_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    // Core
    use std::io::{Read, Write};
    use std::process::{Child, Command, Stdio};
    // Domain
    use super::*;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("fileforge-test-{}", session_name()));
            fs::create_dir(&dir).expect("exclusive test directory");
            Self(dir)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn sessions_never_share_files_and_clear_only_their_own_directory() {
        let root = TestDir::new();
        let mut first = ResultSession::open(root.0.clone()).expect("first");
        let first_path = first.dir().expect("open").join("1.pdf");
        fs::write(&first_path, b"first").expect("write");
        let second = ResultSession::open(root.0.clone()).expect("second");
        let second_path = second.dir().expect("open").join("1.pdf");
        fs::write(&second_path, b"second").expect("write");
        assert_ne!(first_path, second_path);
        assert_eq!(fs::read(&first_path).expect("first intact"), b"first");
        first.clear().expect("clear");
        first.clear().expect("idempotent");
        assert!(!first_path.exists());
        assert!(first.dir().is_err());
        assert_eq!(fs::read(&second_path).expect("second intact"), b"second");
        drop(second);
        assert!(!second_path.exists());
        assert!(root.0.join(COORDINATOR).is_file());
    }

    #[test]
    fn simultaneous_openers_have_distinct_live_leases() {
        let root = TestDir::new();
        let sessions = std::thread::scope(|scope| {
            let threads: Vec<_> =
                (0..8).map(|_| scope.spawn(|| ResultSession::open(root.0.clone()).expect("open"))).collect();
            threads.into_iter().map(|thread| thread.join().expect("thread")).collect::<Vec<_>>()
        });
        let paths: std::collections::HashSet<_> = sessions.iter().map(|s| s.dir().expect("live")).collect();
        assert_eq!(paths.len(), sessions.len());
        for session in &sessions {
            let lease = open_lock(&session.lease_path, false).expect("lease");
            assert!(matches!(lease.try_lock(), Err(TryLockError::WouldBlock)));
        }
    }

    #[test]
    fn coordination_has_a_deadline_and_never_breaks_an_existing_lock() {
        let root = TestDir::new();
        let held = coordinate(&root.0, LOCK_TIMEOUT).expect("hold");
        let inherited = held.0.try_clone().expect("duplicate descriptor");
        assert_eq!(coordinate(&root.0, Duration::ZERO).expect_err("busy").kind(), io::ErrorKind::TimedOut);
        drop(held);
        assert!(coordinate(&root.0, Duration::ZERO).is_ok());
        drop(inherited);
    }

    #[test]
    fn cleanup_preserves_unrecognized_entries_and_removes_abandoned_sessions() {
        let root = TestDir::new();
        let stale = root.0.join("session-1-2-3");
        fs::create_dir(&stale).expect("stale");
        fs::write(stale.join("1.pdf"), b"abandoned").expect("stale result");
        fs::write(root.0.join("session-1-2-3.lock"), b"").expect("unlocked lease");
        let orphan = root.0.join("session-1-2-4.lock");
        fs::write(&orphan, b"").expect("orphan lease");
        let foreign = root.0.join("saved.pdf");
        fs::write(&foreign, b"keep").expect("foreign");
        let unknown = root.0.join("session-not-ours");
        fs::create_dir(&unknown).expect("unknown");
        let _session = ResultSession::open(root.0.clone()).expect("open");
        assert!(!stale.exists());
        assert!(!root.0.join("session-1-2-3.lock").exists());
        assert!(!orphan.exists());
        assert_eq!(fs::read(foreign).expect("foreign retained"), b"keep");
        assert!(unknown.is_dir());
    }

    #[test]
    fn uncertain_lease_and_live_orphan_lease_are_preserved() {
        let root = TestDir::new();
        let uncertain = root.0.join("session-1-2-3");
        fs::create_dir(&uncertain).expect("session");
        fs::create_dir(root.0.join("session-1-2-3.lock")).expect("invalid lease");
        let orphan = root.0.join("session-1-2-4.lock");
        let held = open_lock(&orphan, true).expect("live orphan");
        held.try_lock().expect("lock");
        let _session = ResultSession::open(root.0.clone()).expect("open despite uncertain entry");
        assert!(uncertain.is_dir());
        assert!(orphan.is_file());
    }

    #[test]
    fn bounded_collection_can_defer_entries_without_deleting_live_sessions() {
        let root = TestDir::new();
        let stale = root.0.join("session-1-2-3");
        fs::create_dir(&stale).expect("stale");
        let coordinator = coordinate(&root.0, LOCK_TIMEOUT).expect("coordinate");
        reap_stale(&root.0, 0).expect("budget exhausted");
        assert!(stale.exists());
        reap_stale(&root.0, CLEANUP_ENTRIES).expect("next cleanup");
        assert!(!stale.exists());
        drop(coordinator);
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_never_follows_session_or_lease_links() {
        let root = TestDir::new();
        let outside = TestDir::new();
        let original = outside.0.join("original.pdf");
        fs::write(&original, b"original").expect("original");
        let linked = root.0.join("session-1-2-3");
        std::os::unix::fs::symlink(&outside.0, &linked).expect("directory link");
        let bad_lease = root.0.join("session-1-2-4");
        fs::create_dir(&bad_lease).expect("session");
        std::os::unix::fs::symlink(&original, root.0.join("session-1-2-4.lock")).expect("lease link");
        let stale = root.0.join("session-1-2-5");
        fs::create_dir(&stale).expect("stale");
        std::os::unix::fs::symlink(&outside.0, stale.join("nested")).expect("nested link");
        let _session = ResultSession::open(root.0.clone()).expect("open");
        assert_eq!(fs::read(original).expect("outside intact"), b"original");
        assert!(linked.is_symlink());
        assert!(bad_lease.exists());
        assert!(!stale.exists());
    }

    #[cfg(unix)]
    #[test]
    fn cache_and_coordinator_symlinks_are_refused() {
        let root = TestDir::new();
        let outside = TestDir::new();
        let linked = root.0.join("cache-link");
        std::os::unix::fs::symlink(&outside.0, &linked).expect("cache link");
        assert!(ResultSession::open(linked).is_err());
        let target = outside.0.join("lock");
        fs::write(&target, b"keep").expect("target");
        std::os::unix::fs::symlink(&target, root.0.join(COORDINATOR)).expect("coordinator link");
        assert!(ResultSession::open(root.0.clone()).is_err());
        assert_eq!(fs::read(target).expect("target intact"), b"keep");
    }

    struct Owner {
        child: Child,
        dir: PathBuf,
    }

    impl Owner {
        fn start(root: &Path, mode: &str) -> Self {
            let ready = root.join(format!("ready-{}", session_name()));
            let child = Command::new(std::env::current_exe().expect("test binary"))
                .args(["subprocess_owner", "--nocapture"])
                .env("FILEFORGE_SESSION_TEST_ROOT", root)
                .env("FILEFORGE_SESSION_TEST_READY", &ready)
                .env("FILEFORGE_SESSION_TEST_MODE", mode)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("owner process");
            let mut owner = Self { child, dir: PathBuf::new() };
            let start = Instant::now();
            while !ready.exists() {
                assert!(owner.child.try_wait().expect("poll").is_none(), "owner exited before readiness");
                assert!(start.elapsed() < Duration::from_secs(10), "owner readiness timed out");
                std::thread::sleep(Duration::from_millis(10));
            }
            owner.dir = PathBuf::from(fs::read_to_string(&ready).expect("ready path"));
            assert!(owner.dir.starts_with(root));
            owner
        }

        fn crash(&mut self) {
            self.child.kill().expect("terminate owned process");
            self.child.wait().expect("reap owned process");
        }

        fn finish(&mut self) {
            self.child.stdin.take().expect("stdin").write_all(b"q").expect("request exit");
            let start = Instant::now();
            loop {
                if let Some(status) = self.child.try_wait().expect("poll") {
                    assert!(status.success());
                    break;
                }
                assert!(start.elapsed() < Duration::from_secs(10), "owner exit timed out");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    impl Drop for Owner {
        fn drop(&mut self) {
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
            }
            let _ = self.child.wait();
        }
    }

    // Invoked in a separate test process; no environment changes are made in the parent test runner.
    fn publish_ready(ready: &Path, dir: &Path) {
        let partial = ready.with_extension("partial");
        fs::write(&partial, dir.to_str().expect("test path")).expect("ready contents");
        fs::rename(partial, ready).expect("publish complete readiness");
    }

    fn wait_for_parent() {
        if std::io::stdin().read_exact(&mut [0]).is_err() {
            // Parent teardown may close stdin while termination is being delivered. Preserve crash semantics.
            std::process::exit(92);
        }
    }

    #[test]
    fn subprocess_owner() {
        let Some(root) = std::env::var_os("FILEFORGE_SESSION_TEST_ROOT") else { return };
        let ready = PathBuf::from(std::env::var_os("FILEFORGE_SESSION_TEST_READY").expect("ready path"));
        let root = PathBuf::from(root);
        if std::env::var("FILEFORGE_SESSION_TEST_MODE").as_deref() == Ok("initializing") {
            let _coordinator = coordinate(&root, LOCK_TIMEOUT).expect("creating owner");
            let dir = root.join(session_name());
            fs::create_dir(&dir).expect("partial initialization");
            publish_ready(&ready, &dir);
            wait_for_parent();
        } else {
            let session = ResultSession::open(root).expect("owner session");
            fs::write(session.dir().expect("open").join("1.pdf"), b"child result").expect("result");
            publish_ready(&ready, session.dir().expect("open"));
            wait_for_parent();
        }
    }

    #[test]
    fn live_process_survives_another_open_and_exit() {
        let root = TestDir::new();
        let mut owner = Owner::start(&root.0, "active");
        let other = ResultSession::open(root.0.clone()).expect("other process opens");
        assert_eq!(fs::read(owner.dir.join("1.pdf")).expect("child intact"), b"child result");
        drop(other);
        assert_eq!(fs::read(owner.dir.join("1.pdf")).expect("child still intact"), b"child result");
        owner.finish();
        assert!(!owner.dir.exists());
    }

    #[test]
    fn crashed_process_is_reaped_without_touching_a_live_session() {
        let root = TestDir::new();
        let mut owner = Owner::start(&root.0, "active");
        let live = ResultSession::open(root.0.clone()).expect("live");
        let live_result = live.dir().expect("live").join("1.pdf");
        fs::write(&live_result, b"live").expect("live result");
        owner.crash();
        assert!(owner.dir.exists(), "forced exit skips cleanup");
        let _next = ResultSession::open(root.0.clone()).expect("restart");
        assert!(!owner.dir.exists());
        assert!(!owner.dir.with_extension("lock").exists());
        assert_eq!(fs::read(live_result).expect("live retained"), b"live");
    }

    #[test]
    fn interrupted_creation_is_reaped_only_after_its_coordinator_releases() {
        let root = TestDir::new();
        let mut owner = Owner::start(&root.0, "initializing");
        assert_eq!(coordinate(&root.0, Duration::ZERO).expect_err("creator active").kind(), io::ErrorKind::TimedOut);
        assert!(owner.dir.exists());
        owner.crash();
        let _next = ResultSession::open(root.0.clone()).expect("restart");
        assert!(!owner.dir.exists());
    }
}
