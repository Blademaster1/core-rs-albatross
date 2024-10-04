#[macro_use]
extern crate log;

mod autonat;
mod behaviour;
mod config;
mod connection_pool;
pub mod discovery;
pub mod dispatch;
mod error;
mod network;
#[cfg(feature = "metrics")]
mod network_metrics;
mod network_types;
mod only_secure_ws_transport;
mod rate_limiting;
mod swarm;
mod utils;

pub const DISCOVERY_PROTOCOL: &str = "/nimiq/discovery/0.0.1";
pub const DHT_PROTOCOL: &str = "/nimiq/kad/0.0.1";
pub const AUTONAT_DIAL_REQUEST_PROTOCOL: &str = "/libp2p/autonat/2/dial-request";
pub const AUTONAT_DIAL_BACK_PROTOCOL: &str = "/libp2p/autonat/2/dial-back";

pub use config::{Config, TlsConfig};
pub use error::NetworkError;
pub use libp2p::{
    self,
    identity::{ed25519::Keypair as Ed25519KeyPair, Keypair},
    swarm::NetworkInfo,
    PeerId,
};
pub use network::Network;
use serde::{
    de::Error, ser::Error as SerializationError, Deserialize, Deserializer, Serialize, Serializer,
};

/// Wrapper to libp2p Keypair identity that implements SerDe Serialize/Deserialize
#[derive(Clone, Debug)]
pub struct Libp2pKeyPair(pub Keypair);

impl Serialize for Libp2pKeyPair {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Ok(pk) = self.0.clone().try_into_ed25519() {
            nimiq_serde::FixedSizeByteArray::from(pk.to_bytes()).serialize(serializer)
        } else {
            Err(S::Error::custom("Unsupported key type"))
        }
    }
}

impl<'de> Deserialize<'de> for Libp2pKeyPair {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut hex_encoded: [u8; 64] =
            nimiq_serde::FixedSizeByteArray::deserialize(deserializer)?.into_inner();

        let keypair = libp2p::identity::ed25519::Keypair::try_from_bytes(&mut hex_encoded)
            .map_err(|_| D::Error::custom("Invalid value"))?;

        Ok(Libp2pKeyPair(Keypair::from(keypair)))
    }
}
