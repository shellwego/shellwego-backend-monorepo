//! Wasmtime-based runtime implementation
//!
//! Provides WASM compilation, caching, and resource-limited execution.

use crate::wasm::{CompiledModule, WasmError, WasmRuntimeConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use wasmtime::{Config, Engine, Module};

/// Wasmtime runtime wrapper with module caching
#[derive(Clone)]
pub struct WasmtimeRuntime {
    engine: Engine,
    /// Module cache for faster cold starts - stores compiled modules keyed by hash
    module_cache: Arc<RwLock<HashMap<String, CompiledModule>>>,
    /// Precompiled artifact cache directory
    cache_dir: Option<PathBuf>,
}

impl WasmtimeRuntime {
    /// Create engine with custom config and optional cache directory
    pub fn new(config: &WasmRuntimeConfig) -> Result<Self, WasmError> {
        let mut wasm_config = Config::new();

        // Security & Performance defaults
        wasm_config.consume_fuel(true); // Enable CPU time limits
        wasm_config.epoch_interruption(true); // Enable async timeouts
        wasm_config.cranelift_nan_canonicalization(true); // Deterministic NaN handling
        wasm_config.parallel_compilation(true); // Faster compilation

        // Memory limits
        wasm_config.max_wasm_memory(2u64 * 1024 * 1024 * 1024); // 2GB max linear memory
        wasm_config.static_memory_maximum_size(512u64 * 1024 * 1024); // 512MB max static memory

        // Wasmtime features
        wasm_config.wasm_simd(true); // Enable SIMD for better performance
        wasm_config.wasm_bulk_memory(true); // Enable bulk memory operations

        // Disable non-essential features for security
        wasm_config.wasm_threads(false); // No shared memory threading
        wasm_config.wasm_reference_types(false); // No reference types

        // Epoch configuration for async interruption
        wasm_config.epoch_deadline_boost(1_000_000_000); // 1 second between checks

        let engine = Engine::new(&wasm_config)
            .map_err(|e| WasmError::InstantiateError(format!("Failed to create engine: {}", e)))?;

        let cache_dir = config.cache_dir.clone().and_then(|d| {
            if d.is_empty() {
                None
            } else {
                Some(PathBuf::from(d))
            }
        });

        Ok(Self {
            engine,
            module_cache: Arc::new(RwLock::new(HashMap::new())),
            cache_dir,
        })
    }

    /// Compile a WASM module from raw bytes
    pub fn compile(&self, wasm: &[u8]) -> Result<CompiledModule, WasmError> {
        let module =
            Module::new(&self.engine, wasm).map_err(|e| WasmError::CompileError(e.to_string()))?;
        Ok(CompiledModule { inner: module })
    }

    /// Load and compile a .wasm file from disk
    pub async fn from_file(&self, path: &Path) -> Result<CompiledModule, WasmError> {
        // Check module cache first
        let cache_key = self.compute_cache_key(path).await?;

        {
            let cache = self.module_cache.read().await;
            if let Some(cached) = cache.get(&cache_key) {
                tracing::debug!("Module cache hit for {:?}", path);
                return Ok(cached.clone());
            }
        }

        // Read and compile the module
        let wasm_bytes = tokio::fs::read(path)
            .await
            .map_err(|e| WasmError::Io(e))?;

        let module = self.compile(&wasm_bytes)?;

        // Also check for pre-compiled artifact on disk
        if let Some(ref cache_dir) = self.cache_dir {
            let artifact_path = cache_dir.join(format!("{}.cwasm", cache_key));
            if artifact_path.exists() {
                // Try loading pre-compiled artifact first
                if let Ok(artifact_bytes) = tokio::fs::read(&artifact_path).await {
                    if let Ok(precompiled) = self.load_precompiled(&artifact_bytes) {
                        // Store in cache
                        let mut cache = self.module_cache.write().await;
                        cache.insert(cache_key.clone(), precompiled.clone());
                        return Ok(precompiled);
                    }
                }
            }

            // Save compiled artifact for future use
            if let Ok(serialized) = module.inner.serialize() {
                let _ = tokio::fs::create_dir_all(cache_dir).await;
                let _ = tokio::fs::write(&artifact_path, serialized).await;
            }
        }

        // Store in in-memory cache
        {
            let mut cache = self.module_cache.write().await;
            cache.insert(cache_key, module.clone());
        }

        Ok(module)
    }

    /// Load a pre-compiled artifact from bytes
    ///
    /// # Safety
    /// The artifact must have been compiled by the same Engine configuration.
    /// In production, artifacts should be signed to verify origin.
    pub fn load_precompiled(&self, data: &[u8]) -> Result<CompiledModule, WasmError> {
        // SAFETY: The artifact must have been compiled by the same Engine configuration.
        // In production, we would sign artifacts to verify origin.
        let module = unsafe { Module::deserialize(&self.engine, data) }
            .map_err(|e| WasmError::CompileError(format!("Deserialize failed: {}", e)))?;
        Ok(CompiledModule { inner: module })
    }

    /// Execute a WASM function with input and output bytes
    ///
    /// This is a convenience method that compiles a module (or loads from cache),
    /// instantiates it, and calls the specified export function with the given input.
    pub async fn call(
        &self,
        module: &CompiledModule,
        func_name: &str,
        input_bytes: &[u8],
        fuel_limit: u64,
    ) -> Result<Vec<u8>, WasmError> {
        use wasmtime::{Linker, Store};
        use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

        let engine = &self.engine;
        let mut linker = Linker::new(engine);

        // Enable WASI
        let stdout = wasi_common::pipe::WritePipe::new_in_memory();
        let stderr = wasi_common::pipe::WritePipe::new_in_memory();
        let stdin_pipe = wasi_common::pipe::ReadPipe::from(input_bytes.to_vec());

        let mut wasi_builder = WasiCtxBuilder::new();
        wasi_builder
            .stdin(Box::new(stdin_pipe))
            .stdout(Box::new(stdout.clone()))
            .stderr(Box::new(stderr.clone()))
            .args(&["wasm-function".to_string()])
            .map_err(|e| WasmError::InstantiateError(e.to_string()))?;

        let wasi = wasi_builder.build();

        struct WasmCtx {
            wasi: WasiCtx,
        }
        let ctx = WasmCtx { wasi };

        wasmtime_wasi::add_to_linker(&mut linker, |s: &mut WasmCtx| &mut s.wasi)
            .map_err(|e| WasmError::InstantiateError(e.to_string()))?;

        let mut store = Store::new(engine, ctx);
        store
            .add_fuel(fuel_limit)
            .map_err(|e| WasmError::ResourceLimit(e.to_string()))?;

        let instance = linker
            .instantiate(&mut store, &module.inner)
            .map_err(|e| WasmError::InstantiateError(e.to_string()))?;

        // Try to call the named function
        let func = instance
            .get_typed_func::<(u32, u32), ()>(&mut store, func_name)
            .map_err(|_| {
                WasmError::ExecutionError(format!(
                    "Function '{}' not found, trying _start",
                    func_name
                ))
            });

        match func {
            Ok(typed_func) => {
                // Allocate memory for input
                let memory = instance
                    .get_memory(&mut store, "memory")
                    .ok_or_else(|| {
                        WasmError::ExecutionError("No memory exported".to_string())
                    })?;

                let input_len = input_bytes.len() as u32;
                memory.data_mut(&mut store)[..input_len as usize].copy_from_slice(input_bytes);
                typed_func.call(&mut store, (0, input_len)).map_err(|e| {
                    if let Some(i32_exit) = e.downcast_ref::<wasmtime_wasi::I32Exit>() {
                        WasmError::ExecutionError(format!("WASI exit code: {}", i32_exit.0))
                    } else {
                        WasmError::ExecutionError(e.to_string())
                    }
                })?;

                // Read output from memory (full memory contents as output)
                let data = memory.data(&store);
                let output_len = input_len; // In a real impl, the function would set this
                Ok(data[..output_len as usize].to_vec())
            }
            Err(_) => {
                // Fallback: try _start
                let start_func = instance
                    .get_typed_func::<(), ()>(&mut store, "_start")
                    .map_err(|_| {
                        WasmError::ExecutionError("No _start function found".to_string())
                    })?;
                start_func.call(&mut store, ()).map_err(|e| {
                    if let Some(i32_exit) = e.downcast_ref::<wasmtime_wasi::I32Exit>() {
                        if i32_exit.0 == 0 {
                            WasmError::ExecutionError("".to_string()) // clean exit
                        } else {
                            WasmError::ExecutionError(format!("Exit code: {}", i32_exit.0))
                        }
                    } else {
                        WasmError::ExecutionError(e.to_string())
                    }
                })?;
                Ok(vec![])
            }
        }
    }

    /// Get the engine reference
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Clear the in-memory module cache
    pub async fn clear_cache(&self) {
        let mut cache = self.module_cache.write().await;
        cache.clear();
    }

    /// Get the number of cached modules
    pub async fn cache_size(&self) -> usize {
        let cache = self.module_cache.read().await;
        cache.len()
    }

    /// Compute a cache key from a file path and modification time
    async fn compute_cache_key(&self, path: &Path) -> Result<String, WasmError> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let metadata = tokio::fs::metadata(path)
            .await
            .map_err(|e| WasmError::Io(e))?;

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        metadata.len().hash(&mut hasher);
        metadata
            .modified()
            .map(|t| t.hash(&mut hasher))
            .ok();

        Ok(format!("{:016x}", hasher.finish()))
    }
}
