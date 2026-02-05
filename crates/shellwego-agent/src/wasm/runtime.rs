//! Wasmtime-based runtime implementation

use crate::wasm::{WasmError, WasmConfig, CompiledModule};
use wasmtime::{Engine, Config, Module};

/// Wasmtime runtime wrapper
#[derive(Clone)]
pub struct WasmtimeRuntime {
    engine: Engine,
}

impl WasmtimeRuntime {
    /// Create engine with custom config
    pub fn new(_config: &WasmConfig) -> Result<Self, WasmError> {
        let mut wasm_config = Config::new();
        
        // Security & Performance defaults
        wasm_config.consume_fuel(true); // Enable CPU limits
        wasm_config.epoch_interruption(true); // Enable timeouts
        wasm_config.cranelift_nan_canonicalization(true); // Determinism
        wasm_config.parallel_compilation(true);
        
        // Memory limits
        // static_memory_maximum_size = 4GB usually, but we limit at Linker/Store level
        
        let engine = Engine::new(&wasm_config)
            .map_err(|e| WasmError::InstantiateError(format!("Failed to create engine: {}", e)))?;
            
        Ok(Self { engine })
    }

    /// Compile module
    pub fn compile(&self, wasm: &[u8]) -> Result<CompiledModule, WasmError> {
        let module = Module::new(&self.engine, wasm)
            .map_err(|e| WasmError::CompileError(e.to_string()))?;
            
        Ok(CompiledModule { inner: module })
    }

    /// Load pre-compiled artifact
    pub fn load_precompiled(&self, data: &[u8]) -> Result<CompiledModule, WasmError> {
        // SAFETY: The artifact must have been compiled by the same Engine configuration.
        // In production, we would sign artifacts to verify origin.
        let module = unsafe { Module::deserialize(&self.engine, data) }
            .map_err(|e| WasmError::CompileError(format!("Deserialize failed: {}", e)))?;
            
        Ok(CompiledModule { inner: module })
    }
    
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}