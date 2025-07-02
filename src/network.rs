use libp2p::{
    PeerId, identify,
    identity::PublicKey,
    kad,
    multiaddr::Multiaddr,
    ping, rendezvous,
    swarm::{NetworkBehaviour, StreamProtocol},
};
/// Everything needed to create the swarms
use std::str::FromStr;
use std::time::Duration;

const IPFS_BOOTNODES: [&str; 4] = [
    "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    "QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    "QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
];

const IPFS_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/kad/1.0.0");
const IPFS_BOOT_URL: &str = "/dnsaddr/bootstrap.libp2p.io";

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "Event")]
pub struct NetworkState {
    identify: identify::Behaviour,
    // rendezvous: rendezvous::client::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    // ping: ping::Behaviour,
}

impl NetworkState {
    pub fn new(pub_key: &PublicKey) -> Self {
        let kademlia_cfg = kad::Config::new(IPFS_PROTO_NAME);
        let mut kademlia = kad::Behaviour::with_config(
            pub_key.to_peer_id(),
            kad::store::MemoryStore::new(pub_key.to_peer_id()),
            kademlia_cfg,
        );

        let bootaddr: Multiaddr = IPFS_BOOT_URL.parse().unwrap();
        for peer in IPFS_BOOTNODES {
            let peer = PeerId::from_str(peer).unwrap();
            kademlia.add_address(&peer, bootaddr.clone());
        }

        Self {
            identify: identify::Behaviour::new(identify::Config::new(
                "0.0.1".to_string(),
                pub_key.clone(),
            )),
            // rendezvous: rendezvous::client::Behaviour::new(id_keys.clone()),
            kademlia,
            // ping: ping::Behaviour::new(ping::Config::new().with_interval(Duration::from_secs(1))),
        }
    }
}

#[derive(Debug)]
pub enum Event {
    Identify(identify::Event),
    Rendezvous(rendezvous::client::Event),
    Kademlia(kad::Event),
    Ping(ping::Event),
}

impl From<identify::Event> for Event {
    fn from(event: identify::Event) -> Self {
        Self::Identify(event)
    }
}

impl From<rendezvous::client::Event> for Event {
    fn from(event: rendezvous::client::Event) -> Self {
        Self::Rendezvous(event)
    }
}

impl From<kad::Event> for Event {
    fn from(event: kad::Event) -> Self {
        Self::Kademlia(event)
    }
}

impl From<ping::Event> for Event {
    fn from(event: ping::Event) -> Self {
        Self::Ping(event)
    }
}
