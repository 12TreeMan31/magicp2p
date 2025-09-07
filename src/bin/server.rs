use futures::StreamExt;
use libp2p::{
    SwarmBuilder,
    autonat::v2 as autonat,
    identify,
    identity::Keypair,
    noise, relay, rendezvous,
    swarm::{NetworkBehaviour, Swarm, SwarmEvent},
    tcp, yamux,
};
use std::error::Error;

#[derive(NetworkBehaviour)]
struct Behaviour {
    rendezvous: rendezvous::server::Behaviour,
    identify: identify::Behaviour,
    autonat: autonat::server::Behaviour,
    relay: relay::Behaviour,
}

impl Behaviour {
    pub fn new(keys: &Keypair) -> Self {
        let identify_cfg =
            identify::Config::new_with_signed_peer_record("magic-test/1.0.0".to_string(), keys);
        let identify = identify::Behaviour::new(identify_cfg);

        let rendezvous_cfg = rendezvous::server::Config::default();
        let rendezvous = rendezvous::server::Behaviour::new(rendezvous_cfg);

        let autonat = autonat::server::Behaviour::default();

        let relay_cfg = relay::Config::default();
        let relay = relay::Behaviour::new(keys.public().to_peer_id(), relay_cfg);

        Self {
            rendezvous,
            identify,
            autonat,
            relay,
        }
    }
}

fn network_handle(swarm: &mut Swarm<Behaviour>, event: BehaviourEvent) {
    match event {
        BehaviourEvent::Identify(e) => match e {
            identify::Event::Received { peer_id, info, .. } => {
                println!("Found peer: {}", peer_id);
            }
            identify::Event::Error { peer_id, error, .. } => {
                println!("Error with {} getting peer info: {}", peer_id, error);
            }
            _ => {}
        },
        BehaviourEvent::Rendezvous(e) => match e {
            rendezvous::server::Event::PeerRegistered { peer, registration } => println!(
                "rendezvous: Regestered: {} to {}: TTL {}",
                peer, registration.namespace, registration.ttl
            ),
            rendezvous::server::Event::PeerUnregistered { peer, namespace } => {
                println!("rendezvous: Deregestered {} from {}", peer, namespace)
            }
            rendezvous::server::Event::PeerNotRegistered {
                peer,
                namespace,
                error,
            } => println!(
                "rendezvous: Failed to regester: {} to {}: {:?}",
                peer, namespace, error
            ),
            _ => {}
        },
        BehaviourEvent::Autonat(e) => {
            if e.result.is_ok() {
                println!("Autonat: External address confirmed: {}", e.tested_addr);
                return;
            }
            println!(
                "Autonat: Sent {} bytes; test failed for {} on {}",
                e.data_amount, e.client, e.tested_addr
            );
        }
        _ => {}
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
            SwarmEvent::ConnectionEstablished {
                peer_id,
                established_in: inter,
                ..
            } => println!("Connected to {} in {}ms", peer_id, inter.as_millis()),
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                println!("Connection closed for {}: {:?}", peer_id, cause)
            }
            _ => {}
        }
    }
}
