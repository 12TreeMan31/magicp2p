/// Everything needed to create the swarms
use libp2p::{Swarm, SwarmBuilder, identify, identity::Keypair, ping, swarm::NetworkBehaviour};
use std::time::Duration;

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "Event")]
pub struct NetworkState {
    identify: identify::Behaviour,
    ping: ping::Behaviour,
}

impl NetworkState {
    pub fn new(id_keys: &Keypair) -> Self {
        Self {
            identify: identify::Behaviour::new(identify::Config::new(
                "0.0.1".to_string(),
                id_keys.public(),
            )),
            ping: ping::Behaviour::new(ping::Config::new().with_interval(Duration::from_secs(15))),
        }
    }
}

#[derive(Debug)]
pub enum Event {
    Identify(identify::Event),
    Ping(ping::Event),
}

impl From<identify::Event> for Event {
    fn from(event: identify::Event) -> Self {
        Self::Identify(event)
    }
}

impl From<ping::Event> for Event {
    fn from(event: ping::Event) -> Self {
        Self::Ping(event)
    }
}

// Wrapper around libp2p::SwarmBuilder
//fn build_swarm() -> Swarm<NetworkState> {}
