use crate::behaviour::MainBehaviour;
use libp2p::rendezvous::{Namespace, Registration};
use libp2p::swarm::{ConnectionId, Swarm, dial_opts::DialOpts};
use libp2p::{Multiaddr, PeerId, StreamProtocol, gossipsub::Message, identify::Info};
use std::collections::{HashMap, HashSet};
use std::mem;
use std::time::SystemTime;
use tracing::{error, info, warn};

const RENDEZVOUS_PROTOCOL: StreamProtocol = StreamProtocol::new("/rendezvous/1.0.0");

enum SwarmOpts {
    Mdns(Vec<DialOpts>),
    Rendezvous(Vec<Registration>),
    Identify(Info),
    Gossipsub(Message),
}

#[derive(Debug)]
pub enum Status {
    Dialing,
    Connected,
    Confirmed(Multiaddr),
    Closed,
}

// Fight me
impl PartialEq for Status {
    fn eq(&self, other: &Self) -> bool {
        mem::discriminant(self) == mem::discriminant(other)
    }
    fn ne(&self, other: &Self) -> bool {
        mem::discriminant(self) != mem::discriminant(other)
    }
}

struct Connection {
    peer_id: PeerId,
    epoc: SystemTime,
    status: Status,
}

pub struct ConnectionMonitor {
    local_peer_id: PeerId,
    swarm: Swarm<MainBehaviour>,
    connections: HashMap<ConnectionId, Status>,
    rendezvous: HashSet<PeerId>,
}

impl ConnectionMonitor {
    pub fn new(swarm: Swarm<MainBehaviour>) -> Self {
        ConnectionMonitor {
            local_peer_id: *swarm.local_peer_id(),
            swarm,
            connections: HashMap::new(),
            rendezvous: HashSet::new(),
        }
    }
    /// Helper function that manages connections. It manages dial requests because
    /// libp2p will throw errors if we dail a peer multiple times
    pub fn dial(&mut self, request: DialOpts) {
        let id = request.connection_id();
        if let Some(s) = self.connections.get(&id) {
            if *s == Status::Dialing {
                return;
            }
        }

        if let Err(e) = self.swarm.dial(request) {
            error!(target: "monitor", "Could not dial peer: {}", e);
        }

        self.connections.insert(id, Status::Dialing);
    }
    /// Update the status of a connection
    pub fn update_status(&mut self, id: ConnectionId, status: Status) {
        if !self.connections.contains_key(&id) {
            error!(target: "monitor", "Tried to change status of a unknown ConnectionId: {} to {:?}", id, status)
        }

        let current = self.connections.get(&id).expect("Wont fail");

        match status {
            Status::Closed => {
                if *current != Status::Confirmed(Multiaddr::empty())
                    || *current != Status::Connected
                {
                    panic!("Invalid state for status");
                }
                self.connections.remove(&id);
            }
            Status::Confirmed(addr) => {
                if *current != Status::Connected {
                    panic!("Invalid state for status");
                }
                self.swarm.add_external_address(addr);
            }
            Status::Connected => {
                if *current != Status::Dialing {
                    panic!("Invalid state for status");
                }
                self.connections.insert(id, status);
            }
            Status::Dialing => {}
        }
    }

    pub fn regester(&mut self, info: &Info) {
        let remote_id = info.public_key.to_peer_id();
        if remote_id == self.local_peer_id {
            warn!(target: "monitor", "got local id from identify {:#?}", info);
            return;
        }

        for proto in &info.protocols {
            if *proto == RENDEZVOUS_PROTOCOL {
                self.rendezvous.insert(remote_id);
                if let Err(err) = self.behaviour_mut().rendezvous.register(
                    Namespace::from_static("magic-test"),
                    remote_id,
                    None,
                ) {
                    error!(target: "monitor", "Could not regester: {}", err);
                }
                info!(target: "monitor", "Found rendezvous server: {}/{}", info.observed_addr, remote_id);
            }
        }
    }

    pub fn behaviour_mut(&mut self) -> &mut MainBehaviour {
        self.swarm.behaviour_mut()
    }

    pub fn get_rendezvous(&self) -> impl Iterator<Item = &PeerId> {
        self.rendezvous.iter()
    }
}
