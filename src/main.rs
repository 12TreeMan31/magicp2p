use clap::Parser;
use futures::prelude::*;
use libp2p::{
    Multiaddr, SwarmBuilder, gossipsub,
    identity::Keypair,
    noise, rendezvous,
    swarm::{self, ConnectionId, Swarm, SwarmEvent, dial_opts::DialOpts},
    tcp, yamux,
};
use magicp2p::{
    self,
    behaviour::{MainBehaviour, MainBehaviourEvent, NetworkInfo, SwarmOpts},
};
use std::error::Error;
use tokio::{
    self, io,
    io::AsyncBufReadExt,
    select,
    time::{self, Duration},
};
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

fn dial_remote(
    swarm: &mut Swarm<MainBehaviour>,
    addr: &Multiaddr,
) -> Result<ConnectionId, swarm::DialError> {
    let dialer = DialOpts::unknown_peer_id().address(addr.clone()).build();
    let id = dialer.connection_id();
    swarm.dial(dialer)?;

    swarm.add_external_address(addr.clone());

    Ok(id)
}

fn input_processing(net: &NetworkInfo, behaviour: &mut MainBehaviour, input: String) {
    if input.starts_with(':') {
        if input.contains("scan") {
            behaviour.get_peers(&net.get_rendezvous()[0]);
        }
    }
}

fn network(
    swarm: &mut Swarm<MainBehaviour>,
    event: SwarmEvent<MainBehaviourEvent>,
    id: &ConnectionId,
) {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => info!("Listening on {}", address),
        SwarmEvent::Behaviour(netinfo) => {
            let opts = magicp2p::behaviour::behaviour_handle(netinfo);
            if let Some(x) = opts {
                match x {
                    SwarmOpts::Mdns(v) => {
                        for dial in v {
                            swarm.dial(dial).unwrap();
                        }
                    }
                    SwarmOpts::Rendezvous(reg, _) => {
                        for peer in reg {
                            let dial = DialOpts::peer_id(peer.record.peer_id())
                                .addresses(peer.record.addresses().to_vec())
                                .build();
                            swarm.dial(dial).unwrap();
                        }
                    }
                    SwarmOpts::Disconnect(peer_id) => {}
                }
            }
        }
        SwarmEvent::ExternalAddrConfirmed { address } => {
            info!("External address confirmed: {address}");
        }
        SwarmEvent::ConnectionEstablished {
            peer_id,
            connection_id,
            established_in,
            ..
        } => {
            if connection_id == *id {
                swarm
                    .behaviour_mut()
                    .rendezvous
                    .register(
                        rendezvous::Namespace::from_static("magic-test"),
                        peer_id,
                        None,
                    )
                    .unwrap();
                info!("Connected to server in {}ms", established_in.as_millis());
            }
        }
        event => warn!("Unhandled: {:?}", event),
    }
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Opt {
    /// Connects client to the server at the provided multiaddr. Starts in server mode if none is given.
    #[arg(short, long, value_name = "multiaddr")]
    bootnode: Option<String>,
    /// multiaddr that the client will be listening on.
    #[arg(short, long, value_name = "multiaddr", default_value = "/ip6/::1")]
    address: Vec<String>,
    /// Disables mDNS
    #[arg(short, long)]
    mdns: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let args: Opt = Opt::parse();

    let keys = Keypair::generate_ed25519();
    let mut swarm = SwarmBuilder::with_existing_identity(keys.clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_dns()?
        .with_behaviour(|keys| MainBehaviour::new(keys, !args.mdns))?
        .build();

    for addr in args.address {
        swarm.listen_on(format!("{}/tcp/0", addr).parse()?)?;
        // swarm.listen_on(format!("{}/udp/0/quic-v1", addr).parse()?)?;
    }

    let mut stdin = io::BufReader::new(io::stdin()).lines();

    let mut watched_connection = ConnectionId::new_unchecked(0);
    if let Some(bootnode) = args.bootnode {
        let bootnode: Multiaddr = bootnode.parse()?;
        watched_connection = dial_remote(&mut swarm, &bootnode)?;
    } else {
        // Does nothing for now but sound spooky
        info!("Starting in server mode!");
    }

    let mut netin = NetworkInfo::new();

    let topic = gossipsub::IdentTopic::new("hi-dave");
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    print!("{}", magicp2p::BANNER);

    let discover = time::interval(Duration::from_secs(10));

    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                input_processing(&mut netin, swarm.behaviour_mut(), line.clone());
                if let Err(e) = swarm
                    .behaviour_mut().gossipsub
                    .publish(topic.clone(), line.as_bytes()) {
                    warn!("Publish error: {e:?}");
                }
            }
            event = swarm.select_next_some() => network(&mut swarm, event, &watched_connection),

        }
    }
}
