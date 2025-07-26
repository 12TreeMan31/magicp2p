use libp2p::{
    Multiaddr, PeerId, StreamProtocol,
    identity::PublicKey,
    kad::{
        self, Behaviour,
        Event::{self, OutboundQueryProgressed},
        PeerInfo, QueryId, QueryResult, Record,
        store::MemoryStore,
    },
};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

//#[derive(Debug)]
/*pub enum RegisterType {
    Provider(Option<HashSet<PeerId>>),
    Discover(Vec<PeerInfo>),
}*/

const IPFS_BOOTNODES: [&str; 4] = [
    "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    "QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    "QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
];
const IPFS_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/kad/1.0.0");

/// Allows tasks to monitor for events
/// TODO: Maybe make this a hook for a Stream?
/// TODO: Generic?
/*pub struct Watcher {
    map: Arc<Mutex<HashMap<QueryId, oneshot::Sender<Event>>>>,
}

impl Watcher {
    fn get_sender(&mut self, id: &QueryId) -> Option<oneshot::Sender<Event>> {
        self.map
            .lock()
            .expect("Mutex failed to lock while getting sender")
            .remove(id)
    }
    pub fn new() -> Self {
        Self {
            map: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub fn is_registered(&self, id: &QueryId) -> bool {
        self.map
            .lock()
            .expect("Tried to lock mutex but got an error")
            .contains_key(id)
    }
    pub fn register(&mut self, id: QueryId) -> oneshot::Receiver<Event> {
        let (rx, tx) = oneshot::channel();

        let old_rx = self.map.lock().expect("Couldn't lock").insert(id, rx);
        if let Some(v) = old_rx {
            drop(v);
        }

        tx
    }
    pub fn deregister(&mut self, id: &QueryId) -> Option<()> {
        let rx = self.get_sender(id)?;
        drop(rx);

        Some(())
    }
    /// Sends event to consumer and removes key, returns event or error
    pub fn put_event(&mut self, id: QueryId, event: Event) -> Result<(), Event> {
        let rx = self.get_sender(&id).ok_or(event.clone())?;
        rx.send(event)?;

        Ok(())
    }
}*/

/*struct Tracker {}

impl Tracker {
    /// Returns true if `id`` is being watched
    fn is_watch(&self, id: QueryId) -> bool {
        true
    }
    async fn watch(&mut self, id: QueryId) {}
}*/

/// Handles all events related to Kademlia
/*pub fn event_handler(e: Event, tx: &mpsc::UnboundedSender<RegisterType>) {
    match e {
        OutboundQueryProgressed { result, .. } => match result {
            QueryResult::GetClosestPeers(x) => {
                // println!("{:?}", x);
                if let Ok(peers) = x {
                    let _ = tx.send(RegisterType::Discover(peers.peers));
                }
            }
            QueryResult::GetProviders(x) => {
                // println!("{:?}", x);
                if let Ok(kad::GetProvidersOk::FoundProviders { providers, .. }) = x {
                    let _ = tx.send(RegisterType::Provider(Some(providers)));
                }
            }
            QueryResult::GetRecord(..) => {
                unimplemented!("Connot get records, using providers instead")
            }
            QueryResult::PutRecord(..) => {
                unimplemented!("Cannot put records, using providers instead")
            }
            // Periodic status messesges that only matter if they fail (For now)
            QueryResult::StartProviding(Err(e)) => {
                println!("Kademlia: {:?}", e);
            }
            QueryResult::Bootstrap(Err(e)) => {
                println!("Kademlia: {:?}", e);
            }
            _ => (),
        },

        _ => {}
    }
}*/

/// Makes a pk (public key) record to be stored in the DHT.
pub fn new_pk_record(key: &PublicKey) -> Record {
    let id: PeerId = key.to_peer_id();

    let mut pk_record_key: Vec<u8> = b"/pk/".into();
    pk_record_key.extend_from_slice(id.to_bytes().as_slice());

    let mut pk_record = Record::new(pk_record_key, key.encode_protobuf());
    pk_record.publisher = Some(id);
    pk_record.expires = None;

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
