use shellwego_storage::zfs::{ZfsManager, ZfsCli};
use uuid::Uuid;

fn zfs_pool_required() {
    let output = std::process::Command::new("zpool")
        .arg("list")
        .arg("shellwego")
        .output();

    match output {
        Ok(output) => {
            if !output.status.success() {
                panic!(
                    "SKIPPING: ZFS pool 'shellwego' not found. Run: sudo zpool create shellwego <devices>"
                );
            }
        }
        Err(_) => {
            panic!("SKIPPING: ZFS not available. Install zfsutils-linux first.");
        }
    }
}

#[tokio::test]
async fn test_zfs_manager_new_pool_missing() {
    let result = ZfsManager::new("nonexistent_pool_12345").await;
    assert!(result.is_err(), "Should fail for missing pool");
}

#[tokio::test]
async fn test_zfs_manager_new_pool_exists() {
    zfs_pool_required();

    let result = ZfsManager::new("shellwego").await;
    assert!(result.is_ok(), "Should succeed for existing pool 'shellwego'");
}

#[tokio::test]
async fn test_zfs_clone_speed_tc_i2() {
    zfs_pool_required();

    let mgr = ZfsManager::new("shellwego").await.expect("Run: sudo zpool create shellwego ...");
    let app_id = Uuid::new_v4();

    let start = std::time::Instant::now();
    let result = mgr.init_app_storage(app_id).await;
    let duration = start.elapsed();

    assert!(result.is_ok(), "Failed to initialize app storage: {:?}", result.err());
    assert!(
        duration.as_millis() < 500,
        "ZFS too slow: {:?} (expected <500ms)",
        duration
    );

    let storage = result.unwrap();
    assert_eq!(storage.app_id, app_id);
    assert!(storage.rootfs.contains(&app_id.to_string()));
    assert!(storage.data.contains(&app_id.to_string()));

    let cleanup = mgr.cleanup_app(app_id).await;
    assert!(cleanup.is_ok(), "Cleanup failed: {:?}", cleanup.err());
}

#[tokio::test]
async fn test_zfs_dataset_lifecycle_tc_i2() {
    zfs_pool_required();

    let mgr = ZfsManager::new("shellwego").await.expect("Pool missing");
    let cli = ZfsCli::new();
    let app_id = Uuid::new_v4();
    let dataset_name = format!("shellwego/test-app-{}", app_id);

    let storage = mgr.init_app_storage(app_id).await.expect("Init failed");

    let snapshot_name = format!("{}/@pre-update", storage.rootfs);
    let clone_name = format!("{}-clone", storage.rootfs);

    let result = cli.snapshot(&storage.rootfs, "pre-update").await;
    assert!(result.is_ok(), "Snapshot creation failed: {:?}", result.err());

    let result = cli.clone_snapshot(&snapshot_name, &clone_name).await;
    assert!(result.is_ok(), "Clone creation failed: {:?}", result.err());

    let clone_exists = cli.dataset_exists(&clone_name).await;
    assert!(clone_exists.unwrap_or(false), "Cloned dataset should exist");

    let cleanup = mgr.cleanup_app(app_id).await;
    assert!(cleanup.is_ok(), "Cleanup failed");
}

#[tokio::test]
async fn test_zfs_dataset_hierarchy() {
    zfs_pool_required();

    let mgr = ZfsManager::new("shellwego").await.expect("Pool missing");
    let app_id = Uuid::new_v4();

    let storage = mgr.init_app_storage(app_id).await.expect("Init failed");

    assert!(storage.rootfs.ends_with("/rootfs"));
    assert!(storage.data.ends_with("/data"));
    assert!(storage.snapshots.ends_with("/.snapshots"));

    let _ = mgr.cleanup_app(app_id).await;
}

#[tokio::test]
async fn test_zfs_app_storage_structure() {
    zfs_pool_required();

    let mgr = ZfsManager::new("shellwego").await.expect("Pool missing");
    let app_id = Uuid::new_v4();

    let storage = mgr.init_app_storage(app_id).await.expect("Init failed");

    assert!(storage.rootfs.starts_with("shellwego/shellwego/apps/"));
    assert!(storage.rootfs.contains("/rootfs"));

    let _ = mgr.cleanup_app(app_id).await;
}

#[tokio::test]
async fn test_zfs_cleanup_is_idempotent() {
    zfs_pool_required();

    let mgr = ZfsManager::new("shellwego").await.expect("Pool missing");
    let app_id = Uuid::new_v4();

    let _ = mgr.init_app_storage(app_id).await.expect("Init failed");
    let first_cleanup = mgr.cleanup_app(app_id).await;
    let second_cleanup = mgr.cleanup_app(app_id).await;

    assert!(first_cleanup.is_ok(), "First cleanup should succeed");
    assert!(second_cleanup.is_ok(), "Second cleanup should also succeed (idempotent)");
}

#[tokio::test]
async fn test_zfs_snapshot_and_rollback() {
    zfs_pool_required();

    let mgr = ZfsManager::new("shellwego").await.expect("Pool missing");
    let cli = ZfsCli::new();
    let app_id = Uuid::new_v4();

    let _ = mgr.init_app_storage(app_id).await.expect("Init failed");

    let snapshot_result = mgr.snapshot_volume(app_id, "test-snap").await;
    assert!(snapshot_result.is_ok(), "Snapshot should succeed");

    let rollback_result = mgr.rollback_volume(app_id, "test-snap").await;
    assert!(rollback_result.is_ok(), "Rollback should succeed");

    let _ = mgr.cleanup_app(app_id).await;
}

#[tokio::test]
async fn test_zfs_volume_creation() {
    zfs_pool_required();

    let mgr = ZfsManager::new("shellwego").await.expect("Pool missing");
    let volume_id = Uuid::new_v4();

    let result = mgr.create_volume(volume_id, 1).await;
    assert!(result.is_ok(), "Volume creation should succeed");

    let volume_info = result.unwrap();
    assert!(volume_info.name.contains(&volume_id.to_string()));
}

#[tokio::test]
async fn test_zfs_mountpoint_verification() {
    zfs_pool_required();

    let mgr = ZfsManager::new("shellwego").await.expect("Pool missing");
    let app_id = Uuid::new_v4();

    let storage = mgr.init_app_storage(app_id).await.expect("Init failed");

    let cli = ZfsCli::new();
    let info = cli.get_info(&storage.rootfs).await.expect("Get info failed");

    assert!(info.mountpoint.is_some(), "Dataset should have a mountpoint");

    let _ = mgr.cleanup_app(app_id).await;
}
