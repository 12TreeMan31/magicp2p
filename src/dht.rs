use libp2p::{
    Multiaddr, PeerId, StreamProtocol,
    identity::PublicKey,
    kad::{
        self, Behaviour,
        Event::{self, OutboundQueryProgressed},
        PeerInfo, QueryResult, Record,
        store::MemoryStore,
    },
};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

const IPFS_BOOTNODES: [&str; 4] = [
    "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    "QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    "QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
];
const IPFS_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/kad/1.0.0");

/// Handles all events related to Kademlia
pub fn event_handler(e: Event, tx: &mpsc::UnboundedSender<PeerInfo>) {
    match e {
        OutboundQueryProgressed { result, .. } => match result {
            QueryResult::PutRecord(res) => {
                if res.is_err() {
                    // println!("Error: {:?}", res)
                } else {
                    println!("Connected to Amino!")
                }
            }
            QueryResult::Bootstrap(res) => {
                if res.is_err() {
                    println!("Error: {:?}", res)
                }
            }
            QueryResult::GetClosestPeers(result) => {
                if let Ok(peers) = result {
                    for id in peers.peers {
                        tx.send(id).expect("Error: RX is closed");
                    }
                }
            }
            _ => {}
        },
        _ => {}
    }
}

/// Makes a pk (public key) record to be stored in the DHT.
pub fn new_pk_record(key: &PublicKey) -> Record {
    let id: PeerId = key.to_peer_id();

    let mut pk_record_key: Vec<u8> = b"/pk/".into();
    pk_record_key.extend_from_slice(id.to_bytes().as_slice());

    let mut pk_record = Record::new(pk_record_key, key.encode_protobuf());
    pk_record.publisher = Some(id);
    pk_record.expires = Some(Instant::now() + Duration::from_secs(15));

    pk_record
}

/// Creates behaviour and adds the IPFS bootnodes to the table
pub fn kad_behaviour(id: &PeerId) -> Result<Behaviour<MemoryStore>, Box<dyn std::error::Error>> {
    let mut kademlia_cfg = kad::Config::new(IPFS_PROTO_NAME);
    kademlia_cfg.set_query_timeout(Duration::from_secs(60));

    let mut kademlia = Behaviour::with_config(*id, MemoryStore::new(*id), kademlia_cfg);
    let bootaddr: Multiaddr = "/dnsaddr/bootstrap.libp2p.io".parse()?;

    for peer in IPFS_BOOTNODES {
        let peer: PeerId = peer.parse()?;
        kademlia.add_address(&peer, bootaddr.clone());
    }

    Ok(kademlia)
}
