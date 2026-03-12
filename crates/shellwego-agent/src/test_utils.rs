//! Test utilities and mocks for VMM testing
//!
//! Provides mock implementations that work without KVM/PVM hardware.
//! All tests using these utilities can run on any VPS.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Mock Firecracker binary state for testing
#[derive(Debug, Default)]
pub struct MockFirecrackerState {
    /// Number of VMs started
    pub vms_started: AtomicU64,
    /// Number of VMs stopped  
    pub vms_stopped: AtomicU64,
    /// Whether the mock binary is "installed"
    pub installed: AtomicBool,
    /// Whether to simulate PVM support
    pub pvm_support: AtomicBool,
    /// Whether to simulate KVM support
    pub kvm_support: AtomicBool,
    /// Simulated socket creation delay in ms
    pub socket_delay_ms: AtomicU64,
}

impl MockFirecrackerState {
    /// Create a new mock state
    pub fn new() -> Self {
        Self {
            vms_started: AtomicU64::new(0),
            vms_stopped: AtomicU64::new(0),
            installed: AtomicBool::new(false),
            pvm_support: AtomicBool::new(false),
            kvm_support: AtomicBool::new(false),
            socket_delay_ms: AtomicU64::new(0),
        }
    }

    /// Set the mock binary as installed
    pub fn set_installed(&self, installed: bool) {
        self.installed.store(installed, Ordering::SeqCst);
    }

    /// Set PVM support
    pub fn set_pvm_support(&self, support: bool) {
        self.pvm_support.store(support, Ordering::SeqCst);
    }

    /// Set KVM support (simulates /dev/kvm availability)
    pub fn set_kvm_support(&self, support: bool) {
        self.kvm_support.store(support, Ordering::SeqCst);
    }

    /// Increment VM started counter
    pub fn vm_started(&self) {
        self.vms_started.fetch_add(1, Ordering::SeqCst);
    }

    /// Increment VM stopped counter
    pub fn vm_stopped(&self) {
        self.vms_stopped.fetch_add(1, Ordering::SeqCst);
    }

    /// Get VMs started count
    pub fn get_vms_started(&self) -> u64 {
        self.vms_started.load(Ordering::SeqCst)
    }

    /// Get VMs stopped count
    pub fn get_vms_stopped(&self) -> u64 {
        self.vms_stopped.load(Ordering::SeqCst)
    }

    /// Reset all counters
    pub fn reset(&self) {
        self.vms_started.store(0, Ordering::SeqCst);
        self.vms_stopped.store(0, Ordering::SeqCst);
        self.installed.store(false, Ordering::SeqCst);
        self.pvm_support.store(false, Ordering::SeqCst);
        self.kvm_support.store(false, Ordering::SeqCst);
        self.socket_delay_ms.store(0, Ordering::SeqCst);
    }
}

/// Global mock state (singleton for testing)
static MOCK_STATE: std::sync::OnceLock<Arc<MockFirecrackerState>> = std::sync::OnceLock::new();

/// Get or initialize the global mock state
pub fn get_mock_state() -> Arc<MockFirecrackerState> {
    MOCK_STATE
        .get_or_init(|| Arc::new(MockFirecrackerState::new()))
        .clone()
}

/// Reset the mock state for a new test
pub fn reset_mock_state() {
    get_mock_state().reset();
}

/// Test fixture that creates temporary directories and cleans up on drop
pub struct TestFixture {
    pub temp_dir: PathBuf,
}

impl TestFixture {
    /// Create a new test fixture with a unique temp directory
    pub fn new(name: &str) -> Self {
        let temp_dir = std::env::temp_dir().join(format!(
            "shellwego-test-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
        Self { temp_dir }
    }

    /// Get the temp directory path
    pub fn path(&self) -> &std::path::Path {
        &self.temp_dir
    }

    /// Create a file in the temp directory
    pub fn create_file(&self, name: &str, content: &str) -> PathBuf {
        let path = self.temp_dir.join(name);
        std::fs::write(&path, content).expect("Failed to write file");
        path
    }

    /// Create an executable script in the temp directory
    pub fn create_executable(&self, name: &str, content: &str) -> PathBuf {
        let path = self.temp_dir.join(name);
        std::fs::write(&path, content).expect("Failed to write file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("Failed to set permissions");
        }
        path
    }

    /// Create a mock Firecracker binary
    pub fn create_mock_firecracker(&self, version: &str) -> PathBuf {
        let script = format!(
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
    echo "Firecracker {}"
    exit 0
fi
# Mock socket creation
touch /tmp/mock-firecracker-$$.sock
exec sleep 3600
"#,
            version
        );
        self.create_executable("firecracker", &script)
    }

    /// Create a mock PVM-enabled Firecracker binary
    pub fn create_mock_firecracker_pvm(&self) -> PathBuf {
        let script = r#"#!/bin/sh
if [ "$1" = "--version" ]; then
    echo "Firecracker v1.5.0-pvm"
    exit 0
fi
touch /tmp/mock-firecracker-pvm-$$.sock
exec sleep 3600
"#;
        self.create_executable("firecracker-pvm", script)
    }

    /// Check if a path exists
    pub fn exists(&self, name: &str) -> bool {
        self.temp_dir.join(name).exists()
    }
}

impl Drop for TestFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.temp_dir);
    }
}

/// Builder for creating test configurations
pub struct TestConfigBuilder {
    node_id: Option<uuid::Uuid>,
    region: String,
    zone: String,
    max_microvms: u32,
    force_mode: Option<crate::VirtualizationMode>,
    data_dir: PathBuf,
}

impl TestConfigBuilder {
    pub fn new() -> Self {
        Self {
            node_id: Some(uuid::Uuid::new_v4()),
            region: "test".to_string(),
            zone: "test".to_string(),
            max_microvms: 10,
            force_mode: None,
            data_dir: std::env::temp_dir().join("shellwego-test"),
        }
    }

    pub fn with_force_mode(mut self, mode: crate::VirtualizationMode) -> Self {
        self.force_mode = Some(mode);
        self
    }

    pub fn with_max_microvms(mut self, max: u32) -> Self {
        self.max_microvms = max;
        self
    }

    pub fn with_data_dir(mut self, dir: PathBuf) -> Self {
        self.data_dir = dir;
        self
    }

    pub fn build(self) -> crate::AgentConfig {
        crate::AgentConfig {
            node_id: self.node_id,
            control_plane_url: "http://localhost".into(),
            join_token: None,
            region: self.region,
            zone: self.zone,
            labels: Default::default(),
            firecracker_binary: PathBuf::from("/nonexistent/firecracker"),
            firecracker_pvm_binary: PathBuf::from("/nonexistent/firecracker-pvm"),
            kernel_image_path: PathBuf::from("/nonexistent/vmlinux"),
            data_dir: self.data_dir,
            max_microvms: self.max_microvms,
            reserved_memory_mb: 128,
            reserved_cpu_percent: 0.0,
            force_mode: self.force_mode,
        }
    }
}

impl Default for TestConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_state_operations() {
        let state = MockFirecrackerState::new();

        // Initial state
        assert_eq!(state.get_vms_started(), 0);
        assert_eq!(state.get_vms_stopped(), 0);
        assert!(!state.installed.load(Ordering::SeqCst));

        // Modify state
        state.vm_started();
        state.vm_started();
        state.vm_stopped();
        state.set_installed(true);
        state.set_pvm_support(true);

        // Check modified state
        assert_eq!(state.get_vms_started(), 2);
        assert_eq!(state.get_vms_stopped(), 1);
        assert!(state.installed.load(Ordering::SeqCst));
        assert!(state.pvm_support.load(Ordering::SeqCst));

        // Reset
        state.reset();
        assert_eq!(state.get_vms_started(), 0);
        assert_eq!(state.get_vms_stopped(), 0);
        assert!(!state.installed.load(Ordering::SeqCst));
    }

    #[test]
    fn test_global_mock_state() {
        reset_mock_state();
        let state = get_mock_state();

        state.vm_started();
        assert_eq!(state.get_vms_started(), 1);

        reset_mock_state();
        assert_eq!(state.get_vms_started(), 0);
    }

    #[test]
    fn test_fixture_creation() {
        let fixture = TestFixture::new("basic");
        assert!(fixture.path().exists());
        assert!(fixture.path().is_dir());
    }

    #[test]
    fn test_fixture_file_operations() {
        let fixture = TestFixture::new("files");

        // Create file
        let file = fixture.create_file("test.txt", "hello world");
        assert!(file.exists());

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "hello world");

        // Create multiple files
        fixture.create_file("file1.txt", "content1");
        fixture.create_file("file2.txt", "content2");
        assert!(fixture.exists("file1.txt"));
        assert!(fixture.exists("file2.txt"));
    }

    #[test]
    fn test_fixture_executable() {
        let fixture = TestFixture::new("exec");

        #[cfg(unix)]
        {
            let script = fixture.create_executable("test.sh", "#!/bin/sh\necho test");
            assert!(script.exists());

            // Verify it's executable
            let output = std::process::Command::new(&script).output();
            assert!(output.is_ok());
        }

        #[cfg(not(unix))]
        {
            let _ = fixture.create_executable("test.bat", "@echo test");
        }
    }

    #[test]
    fn test_fixture_mock_firecracker() {
        let fixture = TestFixture::new("fc");

        let fc = fixture.create_mock_firecracker("v1.5.0");
        assert!(fc.exists());

        #[cfg(unix)]
        {
            // Test version output
            let output = std::process::Command::new(&fc)
                .arg("--version")
                .output()
                .expect("Failed to run mock");

            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.contains("v1.5.0"));
        }
    }

    #[test]
    fn test_fixture_mock_firecracker_pvm() {
        let fixture = TestFixture::new("fc-pvm");

        let fc = fixture.create_mock_firecracker_pvm();
        assert!(fc.exists());

        #[cfg(unix)]
        {
            let output = std::process::Command::new(&fc)
                .arg("--version")
                .output()
                .expect("Failed to run mock");

            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.contains("pvm"));
        }
    }

    #[test]
    fn test_fixture_cleanup() {
        let path;
        {
            let fixture = TestFixture::new("cleanup");
            path = fixture.path().to_path_buf();
            fixture.create_file("test.txt", "data");
            assert!(path.exists());
        }
        // After drop, directory should be removed
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(!path.exists());
    }

    #[test]
    fn test_config_builder_default() {
        let config = TestConfigBuilder::new().build();

        assert!(config.node_id.is_some());
        assert_eq!(config.region, "test");
        assert_eq!(config.zone, "test");
        assert_eq!(config.max_microvms, 10);
        assert!(config.force_mode.is_none());
    }

    #[test]
    fn test_config_builder_with_force_mode() {
        let config = TestConfigBuilder::new()
            .with_force_mode(crate::VirtualizationMode::Wasm)
            .build();

        assert_eq!(config.force_mode, Some(crate::VirtualizationMode::Wasm));
    }

    #[test]
    fn test_config_builder_with_max_microvms() {
        let config = TestConfigBuilder::new()
            .with_max_microvms(100)
            .build();

        assert_eq!(config.max_microvms, 100);
    }

    #[test]
    fn test_config_builder_chained() {
        let temp_dir = std::env::temp_dir().join("test-chained-config");
        let config = TestConfigBuilder::new()
            .with_force_mode(crate::VirtualizationMode::Pvm)
            .with_max_microvms(50)
            .with_data_dir(temp_dir.clone())
            .build();

        assert_eq!(config.force_mode, Some(crate::VirtualizationMode::Pvm));
        assert_eq!(config.max_microvms, 50);
        assert_eq!(config.data_dir, temp_dir);
    }
}
