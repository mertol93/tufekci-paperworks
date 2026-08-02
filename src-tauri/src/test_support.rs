use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

pub(crate) fn create_unique_test_directory(prefix: &str) -> PathBuf {
    assert!(
        !prefix.is_empty()
            && prefix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
        "test directory prefixes must be non-empty ASCII slugs"
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for _ in 0..256 {
        let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("failed to create a private test directory: {error}"),
        }
    }

    panic!("failed to allocate a unique private test directory")
}

#[test]
fn creates_distinct_directories_under_parallel_load() {
    let paths = (0..64)
        .map(|_| std::thread::spawn(|| create_unique_test_directory("paperworks-helper-test")))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();

    let mut unique = paths.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), paths.len());
    for path in paths {
        fs::remove_dir(path).unwrap();
    }
}
