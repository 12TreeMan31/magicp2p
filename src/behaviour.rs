use libp2p::{
    autonat,
    gossipsub::{self, MessageAuthenticity},
    identify,
    identity::Keypair,
    mdns, relay, rendezvous,
    swarm::{NetworkBehaviour, behaviour::toggle::Toggle},
};

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
}
