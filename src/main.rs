use clap::Parser;
use futures::prelude::*;
use libp2p::swarm::{self, ConnectionId, Swarm, SwarmEvent, dial_opts::DialOpts};
use libp2p::{Multiaddr, SwarmBuilder, gossipsub, identity::Keypair, noise, tcp, yamux};
use magicp2p::{
    self,
    behaviour::{MainBehaviour, MainBehaviourEvent, SwarmOpts},
    events::{ConnectionMonitor, Status},
};
use std::error::Error;
use tokio::{
    self, io,
    io::AsyncBufReadExt,
    select,
    time::{self, Duration},
};
use tracing::{debug, error, info, warn};
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

fn network_handle(monitor: &mut ConnectionMonitor, event: SwarmEvent<MainBehaviourEvent>) {
    match event {
        SwarmEvent::Dialing { peer_id, .. } => {
            info!("Dialing {:?}", peer_id);
        }
        SwarmEvent::NewListenAddr { address, .. } => {
            info!("Listening on {}", address);
        }
        SwarmEvent::ExternalAddrConfirmed { address } => {
            info!("External address confirmed: {address}");
        }
        SwarmEvent::ConnectionEstablished { established_in, .. } => {
            info!("Connected to server in {}ms", established_in.as_millis());
        }
        SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
            if let Some(id) = peer_id {
                error!("Connection error on {}: {}", id, error);
            } else {
                error!("Connection error on Unknown: {}", error);
            }
        }
        SwarmEvent::Behaviour(event) => {
            let opt = match magicp2p::behaviour::behaviour_handle(event) {
                Some(x) => x,
                None => return,
            };

            match opt {
                SwarmOpts::Connect(list) => {
                    for peer in list {
                        let record = peer.record;

                        let dial_request = DialOpts::peer_id(record.peer_id())
                            .addresses(record.addresses().to_vec())
                            .build();
                        monitor.dial(dial_request);
                    }
                }
                SwarmOpts::Identify(info) => {
                    monitor.regester(&info);
                }
                SwarmOpts::Message(message) => {
                    println!(
                        "#{} <{:?}> {}",
                        message.topic,
                        message.source,
                        String::from_utf8_lossy(&message.data)
                    );
                }
                SwarmOpts::Mdns(list) => {
                    for dail in list {
                        /*if swarm.is_connected(&dail.get_peer_id().unwrap()) {
                            continue;
                        }

                        if let Err(err) = swarm.dial(dail) {
                            error!("Problem dialing peer: {}", err);
                        }*/
                    }
                }
            }
        }
        event => debug!("{:?}", event),
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
        .with(fmt::layer().with_line_number(true).with_file(true))
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
    let mut monitor = ConnectionMonitor::new(swarm);

    if let Some(bootnode) = args.bootnode {
        let bootnode: Multiaddr = bootnode.parse()?;
        let request = DialOpts::unknown_peer_id().address(bootnode).build();
        // NOTE: We haven't confiremd the addr yet, we will do that later
        monitor.dial(request);
    }

    let mut stdin = io::BufReader::new(io::stdin()).lines();

    let topic = gossipsub::IdentTopic::new("hi-dave");
    // swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    let mut discover = time::interval(Duration::from_secs(5));
    print!("{}", magicp2p::BANNER);

    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                if let Err(e) = swarm
                    .behaviour_mut().gossipsub
                    .publish(topic.clone(), line.as_bytes()) {
                    warn!("Publish error: {e:?}");
                }
            }
            event = swarm.select_next_some() => network_handle(&mut monitor, event),
            _ = discover.tick() => {
                for &server in monitor.get_rendezvous() {
                    info!(?server);
                    info!("Scanning: {}", server);
                    swarm.behaviour_mut().rendezvous.discover(None, None, None, server);
                }
            }
        }
    }
}
