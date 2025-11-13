//! This module contains everything that is needed in order for the network
//! to function. This includes network event handling, keeping track of network
//! state, and the actual NetworkBehaviour. This is also where most of the libp2p
//! code will live.
use libp2p::{
    PeerId, autonat,
    gossipsub::{self, Message, MessageAuthenticity},
    identify::{self, Info},
    identity::Keypair,
    mdns, relay,
    rendezvous::{self, Registration},
    swarm::{NetworkBehaviour, behaviour::toggle::Toggle, dial_opts::DialOpts},
};
use tracing::{debug, error, info, warn};

pub const PROGRAM_PROTOCOL: &str = "magic-test/0.0.1";

#[derive(NetworkBehaviour)]
pub struct MainBehaviour {
    pub identify: identify::Behaviour,
    pub gossipsub: gossipsub::Behaviour,
    pub rendezvous: rendezvous::client::Behaviour,
    pub autonat: autonat::v2::client::Behaviour,
    pub relay: relay::Behaviour,

    pub mdns: Toggle<mdns::tokio::Behaviour>,
}

impl MainBehaviour {
    pub fn new(keys: &Keypair, has_mdns: bool) -> Self {
        let peer_id = keys.public().to_peer_id();

        let identify_cfg =
            identify::Config::new_with_signed_peer_record(PROGRAM_PROTOCOL.to_string(), keys);
        let identify = identify::Behaviour::new(identify_cfg);

        let rendezvous = rendezvous::client::Behaviour::new(keys.clone());

        let gossipsub_cfg = gossipsub::ConfigBuilder::default().build().unwrap();
        let gossipsub =
            gossipsub::Behaviour::new(MessageAuthenticity::Signed(keys.clone()), gossipsub_cfg)
                .unwrap();

        let autonat = autonat::v2::client::Behaviour::default();

        let relay_cfg = relay::Config::default();
        let relay = relay::Behaviour::new(peer_id, relay_cfg);

        let mdns = if has_mdns {
            let mdns_cfg = mdns::Config::default();
            let mdns = mdns::tokio::Behaviour::new(mdns_cfg, peer_id).unwrap();
            Toggle::from(Some(mdns))
        } else {
            Toggle::from(None)
        };

        Self {
            identify,
            gossipsub,
            rendezvous,
            autonat,
            relay,
            mdns,
        }
    }
    pub fn get_peers(&mut self, node: &PeerId) {
        self.rendezvous.discover(None, None, None, *node);
    }
}

pub enum SwarmOpts {
    Message(Message),
    Connect(Vec<Registration>),
    Mdns(Vec<DialOpts>),
    Identify(Info),
}

pub fn behaviour_handle(event: MainBehaviourEvent) -> Option<SwarmOpts> {
    match event {
        MainBehaviourEvent::Autonat(_) => None,
        MainBehaviourEvent::Gossipsub(e) => match e {
            gossipsub::Event::Message { message, .. } => Some(SwarmOpts::Message(message)),
            gossipsub::Event::Subscribed { peer_id, topic } => {
                println!("#{} <{}> Connected", topic, peer_id);
                None
            }
            gossipsub::Event::Unsubscribed { peer_id, topic } => {
                println!("#{} <{}> Disconnected", topic, peer_id);
                None
            }
            _ => None,
        },
        MainBehaviourEvent::Identify(e) => match e {
            identify::Event::Received {
                connection_id,
                peer_id,
                info,
            } => {
                info!(target: "identify", ?connection_id, "Found peer {}: supports {:#?}", peer_id, info.protocols);
                debug!(target: "identify", ?connection_id, "supports addrs {:#?}", info.listen_addrs);

                Some(SwarmOpts::Identify(info))
            }
            identify::Event::Error { peer_id, error, .. } => {
                error!(target: "identify", "Error with {} getting peer info: {}", peer_id, error);
                None
            }
            _ => None,
        },
        MainBehaviourEvent::Mdns(e) => match e {
            mdns::Event::Discovered(list) => {
                let mut dials = Vec::new();
                for (peer_id, addr) in list {
                    info!(target: "mDNS", "Adding {}", peer_id);

                    let dial = DialOpts::peer_id(peer_id).addresses(vec![addr]).build();
                    dials.push(dial);
                }
                Some(SwarmOpts::Mdns(dials))
            }
            mdns::Event::Expired(list) => {
                // This needs more testing to see if we need explicitly remove the addr
                for (peer_id, _) in list {
                    info!(target: "mDNS", "Removing {}", peer_id);
                }
                None
            }
        },
        MainBehaviourEvent::Relay(_) => None,
        MainBehaviourEvent::Rendezvous(e) => match e {
            rendezvous::client::Event::Discovered { registrations, .. } => {
                Some(SwarmOpts::Connect(registrations))
            }
            rendezvous::client::Event::Registered {
                rendezvous_node,
                ttl,
                namespace,
            } => {
                info!(target: "rendezvous", ttl, "Regestered on {} to <{}>", namespace, rendezvous_node );
                None
            }
            rendezvous::client::Event::DiscoverFailed {
                rendezvous_node,
                namespace,
                error,
            } => {
                error!(target: "rendezvous", "Discover failed on <{}>:{:?} {:?}", rendezvous_node, namespace, error);
                None
            }
            rendezvous::client::Event::RegisterFailed {
                rendezvous_node,
                namespace,
                error,
            } => {
                error!(target: "rendezvous", "Register failed on <{}>:{} {:?}", rendezvous_node, namespace, error);
                None
            }
            _ => None,
        },
    }
}
