//! WebAssembly runtime for lightweight workloads
//!
//! Alternative to Firecracker for sub-10ms cold starts.

use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use wasi_common::pipe::WritePipe;
use wasmtime::{Linker, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

pub mod runtime;
use runtime::WasmtimeRuntime;

// Re-export types from schema
pub use shellwego_schema::{WasmExitStatus, WasmRuntimeConfig, WasmRuntimeStats};

#[derive(Error, Debug)]
pub enum WasmError {
    #[error("Module compilation failed: {0}")]
    CompileError(String),

    #[error("Instantiation failed: {0}")]
    InstantiateError(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Unknown error: {0}")]
    Other(String),
}

/// WASM runtime manager
#[derive(Clone)]
pub struct WasmRuntime {
    runtime: WasmtimeRuntime,
}

impl WasmRuntime {
    /// Initialize WASM runtime
    pub async fn new(config: &WasmRuntimeConfig) -> Result<Self, WasmError> {
        let runtime = WasmtimeRuntime::new(config)?;
        Ok(Self { runtime })
    }

    /// Compile WASM module from bytes
    pub async fn compile(&self, wasm_bytes: &[u8]) -> Result<CompiledModule, WasmError> {
        self.runtime.compile(wasm_bytes)
    }

    /// Load and compile a .wasm file from disk (with caching)
    pub async fn from_file(&self, path: &std::path::Path) -> Result<CompiledModule, WasmError> {
        self.runtime.from_file(path).await
    }

    /// Execute a WASM function with input/output
    pub async fn call(
        &self,
        module: &CompiledModule,
        func_name: &str,
        input: &[u8],
        fuel_limit: u64,
    ) -> Result<Vec<u8>, WasmError> {
        self.runtime.call(module, func_name, input, fuel_limit).await
    }

    /// Spawn new WASM instance (like a microVM)
    pub async fn spawn(
        &self,
        module: &CompiledModule,
        env_vars: &[(String, String)],
        args: &[String],
    ) -> Result<WasmInstance, WasmError> {
        let engine = self.runtime.engine();
        let mut linker = Linker::new(engine);

        // Enable WASI
        wasmtime_wasi::add_to_linker(&mut linker, |s: &mut WasmContext| &mut s.wasi)
            .map_err(|e| WasmError::InstantiateError(e.to_string()))?;

        // Setup Pipes
        let stdout = WritePipe::new_in_memory();
        let stderr = WritePipe::new_in_memory();

        // Setup WASI context
        let mut builder = WasiCtxBuilder::new();
        builder
            .stdout(Box::new(stdout.clone()))
            .stderr(Box::new(stderr.clone()))
            .args(args)
            .map_err(|e| WasmError::InstantiateError(e.to_string()))?
            .envs(env_vars)
            .map_err(|e| WasmError::InstantiateError(e.to_string()))?;

        let wasi = builder.build();
        let ctx = WasmContext { wasi };

        let mut store = Store::new(engine, ctx);

        // Set limits (e.g. 500ms CPU time approx)
        store
            .add_fuel(10_000_000)
            .map_err(|e| WasmError::ResourceLimit(e.to_string()))?;

        let instance = linker
            .instantiate(&mut store, &module.inner)
            .map_err(|e| WasmError::InstantiateError(e.to_string()))?;

        Ok(WasmInstance {
            store: Arc::new(Mutex::new(store)),
            instance,
            _stdout: stdout,
            _stderr: stderr,
        })
    }
}

struct WasmContext {
    wasi: WasiCtx,
}

/// Compiled WASM module handle
#[derive(Clone)]
pub struct CompiledModule {
    pub(crate) inner: wasmtime::Module,
}

/// Running WASM instance
pub struct WasmInstance {
    store: Arc<Mutex<Store<WasmContext>>>,
    instance: wasmtime::Instance,
    _stdout: WritePipe<std::io::Cursor<Vec<u8>>>,
    _stderr: WritePipe<std::io::Cursor<Vec<u8>>>,
}

impl WasmInstance {
    /// Wait for completion
    /// This runs the `_start` function of the WASI module with async timeout support.
    pub async fn wait(self, timeout: std::time::Duration) -> Result<WasmExitStatus, WasmError> {
        // Run the WASM function in a blocking thread to avoid stalling the async runtime
        let result = tokio::task::spawn_blocking(move || {
            let mut store = self.store.blocking_lock();

            let func = self
                .instance
                .get_typed_func::<(), ()>(&mut *store, "_start")
                .map_err(|_| {
                    WasmError::ExecutionError("Missing _start function".to_string())
                })?;

            func.call(&mut *store, ()).map_err(|e| {
                if let Some(i32_exit) = e.downcast_ref::<wasmtime_wasi::I32Exit>() {
                    WasmError::ExecutionError(format!("EXIT:{}", i32_exit.0))
                } else {
                    WasmError::ExecutionError(e.to_string())
                }
            })
        });

        match tokio::time::timeout(timeout, result).await {
            Ok(Ok(Ok(_))) => Ok(WasmExitStatus {
                success: true,
                code: 0,
            }),
            Ok(Ok(Err(WasmError::ExecutionError(msg)))) if msg.starts_with("EXIT:") => {
                let code: i32 = msg[5..].parse().unwrap_or(1);
                Ok(WasmExitStatus {
                    success: code == 0,
                    code,
                })
            }
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(e)) => Err(WasmError::ExecutionError(format!(
                "WASM task panicked or was cancelled: {}",
                e
            ))),
            Err(_) => Err(WasmError::ResourceLimit(format!(
                "WASM execution timed out after {:?}",
                timeout
            ))),
        }
    }

    /// Retrieve stdout content from the in-memory write pipe
    pub async fn get_stdout(&self) -> Vec<u8> {
        let stdout = self._stdout.clone();
        match stdout.try_into_inner() {
            Ok(cursor) => cursor.into_inner(),
            Err(_) => {
                // If we can't take the inner (shared references exist), return empty.
                // In practice, after wait() completes, we should be able to read.
                Vec::new()
            }
        }
    }

    /// Retrieve stderr content from the in-memory write pipe
    pub async fn get_stderr(&self) -> Vec<u8> {
        let stderr = self._stderr.clone();
        match stderr.try_into_inner() {
            Ok(cursor) => cursor.into_inner(),
            Err(_) => Vec::new(),
        }
    }
}

// ExitStatus, WasmConfig, and WasmStats types are now imported from shellwego_schema:
// - WasmExitStatus (re-exported above)
// - WasmRuntimeConfig (re-exported above)
// - WasmRuntimeStats (re-exported above)
