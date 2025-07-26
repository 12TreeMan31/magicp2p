use crate::dht;
use libp2p::{
    Multiaddr, PeerId,
    gossipsub::{self, Hasher, MessageAuthenticity, Topic},
    identify,
    identity::Keypair,
    kad,
    swarm::NetworkBehaviour,
};
use std::{collections::HashSet, error::Error};

// Might be better to make a struct with a tag
pub enum RegisterType {
    /// Peer to be regestered to the DHT and discovered via Identify
    Identify(PeerId, Vec<Multiaddr>),
    Provider(HashSet<PeerId>),
    /// Peer that we need to dial for pubsub
    Pubsub(PeerId, Multiaddr),
}

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
                "/ipfs/id/1.0.0".to_string(),
                pub_key.clone(),
            )),
            kademlia: dht::kad_behaviour(&pub_key.to_peer_id()).expect("Could not make kad"),
            gossipsub: gossipsub::Behaviour::new(
                MessageAuthenticity::Signed(keys.clone()),
                gossipsub::ConfigBuilder::default()
                    .mesh_outbound_min(0)
                    .mesh_n(1)
                    .mesh_n_low(0)
                    .build()
                    .unwrap(),
            )
            .unwrap(),
        }
    }
    pub fn subscribe<H: Hasher>(&mut self, topic: &Topic<H>) -> Result<(), Box<dyn Error>> {
        self.gossipsub.subscribe(topic)?;

        let topic_key = topic.hash().into_string().into_bytes();
        self.kademlia.start_providing(topic_key.clone().into())?;
        self.kademlia.get_providers(topic_key.into());
        Ok(())
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
