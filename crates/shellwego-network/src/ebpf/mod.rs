#[cfg(feature = "ebpf")]
use aya::{
    programs::{Xdp, XdpFlags, SchedClassifier, TcAttachType},
    Ebpf,
};
#[cfg(feature = "ebpf")]
use aya_log::EbpfLogger;
use thiserror::Error;
#[cfg(feature = "ebpf")]
use std::sync::Arc;
#[cfg(feature = "ebpf")]
use tokio::sync::Mutex;

pub mod firewall;
pub mod qos;

#[derive(Error, Debug)]
pub enum EbpfError {
    #[error("aya load failed: {0}")]
    LoadFailed(String),
    #[cfg(feature = "ebpf")]
    #[error("Bpf error: {0}")]
    Bpf(#[from] aya::EbpfError),
    #[cfg(feature = "ebpf")]
    #[error("Program error: {0}")]
    Program(#[from] aya::programs::ProgramError),
    #[cfg(feature = "ebpf")]
    #[error("Map error: {0}")]
    Map(#[from] aya::maps::MapError),
    #[error("eBPF not supported in this build")]
    NotSupported,
}

/// The heart of the data plane. Replaces legacy iptables logic.
#[derive(Clone)]
pub struct EbpfManager {
    #[cfg(feature = "ebpf")]
    bpf: Arc<Mutex<Ebpf>>,
}

impl EbpfManager {
    pub async fn new() -> Result<Self, EbpfError> {
        #[cfg(feature = "ebpf")]
        {
            let bytes = include_bytes!("bin/shellwego.bin");
            let mut bpf = Ebpf::load(bytes)?;

            if let Err(e) = EbpfLogger::init(&mut bpf) {
                tracing::warn!("failed to initialize eBPF logger: {}", e);
            }

            Ok(Self {
                bpf: Arc::new(Mutex::new(bpf)),
            })
        }
        #[cfg(not(feature = "ebpf"))]
        {
            Ok(Self {})
        }
    }

    pub async fn attach_firewall(&self, _iface: &str) -> Result<(), EbpfError> {
        #[cfg(feature = "ebpf")]
        {
            let mut bpf = self.bpf.lock().await;
            let program: &mut Xdp = bpf.program_mut("ingress_filter").ok_or_else(|| EbpfError::LoadFailed("ingress_filter not found".to_string()))?.try_into()?;

            program.load()?;
            program.attach(_iface, XdpFlags::SKB_MODE)?;

            tracing::info!("XDP firewall attached to {}", _iface);
            Ok(())
        }
        #[cfg(not(feature = "ebpf"))]
        {
            Ok(())
        }
    }

    pub async fn apply_qos(&self, _iface: &str, _limit_mbps: u32) -> Result<(), EbpfError> {
        #[cfg(feature = "ebpf")]
        {
            let mut bpf = self.bpf.lock().await;

            let mut rates = aya::maps::HashMap::try_from(bpf.map_mut("RATE_LIMITS").ok_or_else(|| EbpfError::LoadFailed("RATE_LIMITS not found".to_string()))?)?;
            rates.insert(iface_to_u32(_iface), _limit_mbps, 0)?;

            let prog: &mut SchedClassifier = bpf.program_mut("tc_egress_limiter").ok_or_else(|| EbpfError::LoadFailed("tc_egress_limiter not found".to_string()))?.try_into()?;
            prog.load()?;
            prog.attach(_iface, TcAttachType::Egress)?;

            tracing::info!("eBPF QoS applied to {} ({} Mbps)", _iface, _limit_mbps);
            Ok(())
        }
        #[cfg(not(feature = "ebpf"))]
        {
            Ok(())
        }
    }
}

#[cfg(feature = "ebpf")]
fn iface_to_u32(iface: &str) -> u32 {
    nix::net::if_::if_nametoindex(iface).unwrap_or(0)
}
