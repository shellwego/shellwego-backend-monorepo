# ShellWeGo Agent
**The microVM janitor.** Runs on every worker node to keep workloads isolated and fast.

- **Isolation:** Orchestrates AWS Firecracker for hardware-level isolation.
- **WASM:** Alternative `wasmtime` runtime for <10ms cold starts on serverless-style functions.
- **Reconciler:** A K8s-style control loop that converges local state with CP orders.
- **Live Migration:** Snapshot-based VM migration (work in progress).
