# ShellWeGo Storage
**Data Persistence.** ZFS-first storage management.

- **ZFS Driver:** Wrapper for `zfs` CLI with structured output parsing.
- **Snapshots:** Automated point-in-time recovery for persistent volumes.
- **Encryption:** LUKS-based encryption at rest (KMS integration).
- **Backups:** S3-compatible offsite replication logic.
