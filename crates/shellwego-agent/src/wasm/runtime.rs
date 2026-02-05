//! Wasmtime-based runtime implementation

use crate::wasm::{WasmError, WasmConfig, CompiledModule, ExitStatus};

pub struct WasmtimeRuntime {
}

impl WasmtimeRuntime {
    pub fn new(_config: &WasmConfig) -> Result<Self, WasmError> {
        Ok(Self {})
    }

    pub fn compile(&self, _wasm: &[u8]) -> Result<CompiledModule, WasmError> {
        Ok(CompiledModule {})
    }

    pub fn load_precompiled(&self, _data: &[u8]) -> Result<CompiledModule, WasmError> {
        Ok(CompiledModule {})
    }
}

pub struct ComponentRuntime {
}

impl ComponentRuntime {
    pub fn compile_component(&self, _wasm: &[u8]) -> Result<Component, WasmError> {
        Ok(Component {})
    }
}

pub struct Component {
}

pub struct CapabilityProvider {
}