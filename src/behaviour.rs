use crate::dht;
use libp2p::{
    gossipsub::{self, MessageAuthenticity},
    identify,
    identity::Keypair,
    kad,
    swarm::NetworkBehaviour,
};
use std::time::Duration;

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "Event")]
pub struct Behaviour {
    pub identify: identify::Behaviour,
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    pub gossipsub: gossipsub::Behaviour,
}

impl Behaviour {
    pub fn new(keys: &Keypair) -> Self {
        let pub_key = keys.public();
        Self {
            identify: identify::Behaviour::new(identify::Config::new(
                "0.0.1".to_string(),
                pub_key.clone(),
            )),
            kademlia: dht::kad_behaviour(&pub_key.to_peer_id()).expect("Could not make kad"),
            gossipsub: gossipsub::Behaviour::new(
                MessageAuthenticity::Signed(keys.clone()),
                gossipsub::Config::default(),
            )
            .unwrap(),
        }
    }
}

#[derive(Debug)]
pub enum Event {
    Identify(identify::Event),
    Kademlia(kad::Event),
    Gossipsub(gossipsub::Event),
}

impl From<identify::Event> for Event {
    fn from(event: identify::Event) -> Self {
        Self::Identify(event)
    }
}

impl From<kad::Event> for Event {
    fn from(event: kad::Event) -> Self {
        Self::Kademlia(event)
    }
}

impl From<gossipsub::Event> for Event {
    fn from(event: gossipsub::Event) -> Self {
        Self::Gossipsub(event)
    }
}
