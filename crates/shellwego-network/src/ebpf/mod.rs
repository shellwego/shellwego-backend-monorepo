use aya::{
    programs::{Xdp, XdpFlags, SchedClassifier, TcAttachType},
    Bpf,
};
use aya_log::BpfLogger;
use thiserror::Error;
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod firewall;
pub mod qos;

#[derive(Error, Debug)]
pub enum EbpfError {
    #[error("aya load failed: {0}")]
    LoadFailed(String),
    #[error("Bpf error: {0}")]
    Bpf(#[from] aya::BpfError),
    #[error("Program error: {0}")]
    Program(#[from] aya::programs::ProgramError),
}

/// The heart of the data plane. Replaces legacy iptables logic.
#[derive(Clone)]
pub struct EbpfManager {
    bpf: Arc<Mutex<Bpf>>,
}

impl EbpfManager {
    pub async fn new() -> Result<Self, EbpfError> {
        let bytes = include_bytes!("../../../../target/bpf/shellwego.bin");
        let mut bpf = Bpf::load(bytes)?;

        if let Err(e) = BpfLogger::init(&mut bpf) {
            tracing::warn!("failed to initialize eBPF logger: {}", e);
        }

        Ok(Self {
            bpf: Arc::new(Mutex::new(bpf)),
        })
    }

    pub async fn attach_firewall(&self, iface: &str) -> Result<(), EbpfError> {
        let mut bpf = self.bpf.lock().await;
        let program: &mut Xdp = bpf.program_mut("ingress_filter").unwrap().try_into()?;

        program.load()?;
        program.attach(iface, XdpFlags::SKB_MODE)?;

        tracing::info!("XDP firewall attached to {}", iface);
        Ok(())
    }

    pub async fn apply_qos(&self, iface: &str, limit_mbps: u32) -> Result<(), EbpfError> {
        let mut bpf = self.bpf.lock().await;

        let mut rates = aya::maps::HashMap::try_from(bpf.map_mut("RATE_LIMITS").unwrap())?;
        rates.insert(iface_to_u32(iface), limit_mbps, 0)?;

        let prog: &mut SchedClassifier = bpf.program_mut("tc_egress_limiter").unwrap().try_into()?;
        prog.load()?;
        prog.attach(iface, TcAttachType::Egress)?;

        tracing::info!("eBPF QoS applied to {} ({} Mbps)", iface, limit_mbps);
        Ok(())
    }
}

fn iface_to_u32(iface: &str) -> u32 {
    nix::net::if_::if_nametoindex(iface).unwrap_or(0)
}
