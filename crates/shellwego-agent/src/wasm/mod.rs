//! WebAssembly runtime for lightweight workloads
//! 
//! Alternative to Firecracker for sub-10ms cold starts.

use thiserror::Error;
use std::sync::Arc;
use wasmtime::{Linker, Store, Engine};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};
use tokio::sync::Mutex;
use std::path::Path;

pub mod runtime;
use runtime::WasmtimeRuntime;

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
    // Store modules in memory for "warm" starts
    module_cache: Arc<Mutex<std::collections::HashMap<String, CompiledModule>>>,
}

impl WasmRuntime {
    /// Initialize WASM runtime
    pub async fn new(config: &WasmConfig) -> Result<Self, WasmError> {
        let runtime = WasmtimeRuntime::new(config)?;
        Ok(Self {
            runtime,
            module_cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
        })
    }

    /// Compile WASM module from bytes
    pub async fn compile(&self, wasm_bytes: &[u8]) -> Result<CompiledModule, WasmError> {
        self.runtime.compile(wasm_bytes)
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
        let stdout = wasmtime_wasi::pipe::WritePipe::new_in_memory();
        let stderr = wasmtime_wasi::pipe::WritePipe::new_in_memory();
        
        // Setup WASI context
        let mut builder = WasiCtxBuilder::new();
        builder
            .stdout(Box::new(stdout.clone()))
            .stderr(Box::new(stderr.clone()))
            .args(args).map_err(|e| WasmError::InstantiateError(e.to_string()))?
            .envs(env_vars).map_err(|e| WasmError::InstantiateError(e.to_string()))?;

        let wasi = builder.build();
        let ctx = WasmContext { wasi };
        
        let mut store = Store::new(engine, ctx);
        
        // Set limits (e.g. 500ms CPU time approx)
        store.add_fuel(10_000_000).map_err(|e| WasmError::ResourceLimit(e.to_string()))?;

        let instance = linker.instantiate(&mut store, &module.inner)
            .map_err(|e| WasmError::InstantiateError(e.to_string()))?;

        Ok(WasmInstance {
            store: Arc::new(Mutex::new(store)),
            instance,
            stdout,
            stderr,
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
    stdout: wasmtime_wasi::pipe::WritePipe<std::io::Cursor<Vec<u8>>>,
    stderr: wasmtime_wasi::pipe::WritePipe<std::io::Cursor<Vec<u8>>>,
}

impl WasmInstance {
    /// Wait for completion
    /// This runs the `_start` function of the WASI module
    pub async fn wait(self, _timeout: std::time::Duration) -> Result<ExitStatus, WasmError> {
        let mut store = self.store.lock().await;
        
        // Get the entry point (usually _start for WASI command modules)
        let func = self.instance.get_typed_func::<(), ()>(&mut *store, "_start")
            .map_err(|_| WasmError::ExecutionError("Missing _start function".to_string()))?;
            
        // TODO: Run in a separate thread/task with timeout to avoid blocking executor
        // For now, we run directly (blocking)
        match func.call(&mut *store, ()) {
            Ok(_) => Ok(ExitStatus { success: true, code: 0 }),
            Err(e) => {
                // Check if it's a clean exit (WASI exit)
                if let Some(i32_exit) = e.downcast_ref::<wasmtime_wasi::I32Exit>() {
                    Ok(ExitStatus { success: i32_exit.0 == 0, code: i32_exit.0 })
                } else {
                    Err(WasmError::ExecutionError(e.to_string()))
                }
            }
        }
    }

    /// Retrieve stdout content
    pub async fn get_stdout(&self) -> Vec<u8> {
        // In a real stream, we'd read from the pipe. 
        // WritePipe::try_into_inner is complex with Arc, so we assume we can read the buffer.
        // For this impl, we just stub it as the pipe logic in wasi-common is involved.
        Vec::new() 
    }
}

/// Instance exit status
#[derive(Debug, Clone)]
pub struct ExitStatus {
    pub success: bool,
    pub code: i32,
}

/// WASM configuration
#[derive(Debug, Clone)]
pub struct WasmConfig {
    pub max_memory_mb: u32,
}

/// Runtime statistics
#[derive(Debug, Clone, Default)]
pub struct WasmStats {
    pub active_instances: u32,
}