use futures::StreamExt;
use libp2p::{
    SwarmBuilder, identify,
    identity::Keypair,
    noise, rendezvous,
    swarm::{NetworkBehaviour, Swarm, SwarmEvent},
    tcp, yamux,
};
use std::error::Error;

#[derive(NetworkBehaviour)]
struct Behaviour {
    rendezvous: rendezvous::server::Behaviour,
    identify: identify::Behaviour,
}

impl Behaviour {
    pub fn new(keys: &Keypair) -> Self {
        let identify_cfg =
            identify::Config::new_with_signed_peer_record("magic-test/1.0.0".to_string(), keys);
        let identify = identify::Behaviour::new(identify_cfg);

        let rendezvous_cfg = rendezvous::server::Config::default();
        let rendezvous = rendezvous::server::Behaviour::new(rendezvous_cfg);

        Self {
            rendezvous,
            identify,
        }
    }
}

fn network_handle(swarm: &mut Swarm<Behaviour>, event: BehaviourEvent) {
    match event {
        BehaviourEvent::Identify(e) => println!("{:?}", e),
        BehaviourEvent::Rendezvous(e) => println!("{:?}", e),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let keys = Keypair::generate_ed25519();
    println!("Local PeerID {}", keys.public().to_peer_id());

    let mut swarm = SwarmBuilder::with_existing_identity(keys.clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|keys| Behaviour::new(keys))?
        .build();
    swarm.listen_on("/ip6/::/tcp/8011".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/8011".parse()?)?;

    loop {
        let event = swarm.select_next_some().await;
        match event {
            SwarmEvent::NewListenAddr { address, .. } => println!("Listening on {}", address),
            SwarmEvent::Behaviour(netinfo) => network_handle(&mut swarm, netinfo),
            event => println!("Unhandled: {:?}", event),
        }
    }
}
