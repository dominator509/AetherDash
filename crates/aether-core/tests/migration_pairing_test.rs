//! EP-003 M7: Migration pairing integration test.
//! Validates: fresh DB -> run all -> revert all -> run all.
//! Requires: DATABASE_URL pointing to a running Postgres (dev compose).
//! Run via: cargo test --workspace -- --ignored --test-threads=1

#[cfg(test)]
mod migration_pairing_tests {
    use std::process::Command;

    fn sqlx_migrate_dir() -> String {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let path = std::path::Path::new(&manifest_dir)
            .parent()
            .unwrap() // crates/
            .parent()
            .unwrap(); // repo root/
        path.join("infra/migrations").to_string_lossy().to_string()
    }

    fn run_migrations() {
        let dir = sqlx_migrate_dir();
        let output = Command::new("cargo")
            .args(["sqlx", "migrate", "run", "--source", dir.as_str()])
            .output()
            .expect("failed to run cargo sqlx migrate run");
        assert!(
            output.status.success(),
            "migrate run failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn revert_all_migrations() {
        let dir = sqlx_migrate_dir();
        let output = Command::new("cargo")
            .args(["sqlx", "migrate", "revert", "--source", dir.as_str(), "--target", "0"])
            .output()
            .expect("failed to run cargo sqlx migrate revert");
        assert!(
            output.status.success(),
            "migrate revert failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    #[ignore] // Requires running Postgres via docker compose
    fn migration_pairing_up_down_up() {
        // Phase 1: Run all migrations from clean state
        run_migrations();
        eprintln!("Phase 1 PASS: all migrations applied");

        // Phase 2: Revert all migrations
        revert_all_migrations();
        eprintln!("Phase 2 PASS: all migrations reverted");

        // Phase 3: Re-run all migrations (proves reversibility)
        run_migrations();
        eprintln!("Phase 3 PASS: all migrations re-applied after revert");
    }

    #[test]
    #[ignore] // Requires running Postgres via docker compose
    fn migration_pairing_stepwise_revert() {
        // Apply all, revert one, re-apply — proves individual reversibility
        run_migrations();
        eprintln!("All migrations applied");

        // Revert one step
        let dir = sqlx_migrate_dir();
        let output = Command::new("cargo")
            .args(["sqlx", "migrate", "revert", "--source", dir.as_str()])
            .output()
            .expect("failed to revert one step");
        assert!(
            output.status.success(),
            "stepwise revert failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        eprintln!("One migration reverted");

        // Re-apply
        run_migrations();
        eprintln!("Migration re-applied after stepwise revert");
    }

    #[test]
    fn migration_files_are_paired() {
        // Verify every up.sql has a matching down.sql (no DB needed)
        use std::fs;
        let dir = std::path::PathBuf::from(sqlx_migrate_dir());
        if !dir.exists() {
            panic!(
                "Migration directory not found at {}. \
                 Run from the repository root or set CARGO_MANIFEST_DIR correctly.",
                dir.display()
            );
        }
        let mut up_files: Vec<String> = Vec::new();
        let mut down_files: Vec<String> = Vec::new();
        for entry in fs::read_dir(dir).expect("read_dir failed") {
            let entry = entry.expect("entry failed");
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".up.sql") {
                up_files.push(name.replace(".up.sql", ""));
            } else if name.ends_with(".down.sql") {
                down_files.push(name.replace(".down.sql", ""));
            }
        }
        // Ignore .gitkeep and any other non-migration files
        up_files.sort();
        down_files.sort();
        assert_eq!(
            up_files, down_files,
            "Migration pairing mismatch:\n  up: {:?}\n  down: {:?}",
            up_files, down_files
        );
    }
}
