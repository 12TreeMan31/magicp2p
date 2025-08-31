# magicp2p

## Steps in order to connect to Amino
Make sure you have support to RSA and ED25519 in libp2p-identity
RSA is the old standard that is being [fazed out](link).
Once you have that create a ED25519 keypair to give to the swarm.

#### Intialize the network behavour with these protocals:
- kademlia      (the dht)
- identify      (for sharing supported protocals)
- autonat-v2*    (for what ip addrs are reachable and can be shared on animo)
- relay         (for ipv4)
- dcutr     (unkown if needed)

*Autonat-v1 is being deprecated by [kubo](https://github.com/ipfs/kubo/releases#kubo-now-uses-autonatv2-as-a-client); despite this, rust-libp2ps [default behavour](https://docs.rs/libp2p/latest/libp2p/autonat/struct.Behaviour.html) is still using v1 and so you must explicitly use v2.

- Create the swarm with the ED25519 key and wanted transports (which should just be dns, tcp, and quic-v1*)
- Listen on ipv6 and ipv4

*Do not support quic [draft 29](https://github.com/libp2p/specs/blob/master/quic/README.md#quic-versions)

#### Next we should be ready to bootstrap to the ipfs bootnodes:

```mermaid
graph LR;
    A[TBD]
```

```
/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN
/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa
/dnsaddr/bootstrap.libp2p.io/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb
/dnsaddr/bootstrap.libp2p.io/p2p/QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt
```

We add these to the DHT using kademlias `add_address()` and its important to know that you need to have the [peerID in the multiaddr](rust-libp2p github issue).

Now you can call `bootstrap()` and you *should* be connected to Amino as a [client](link that explains what a client can do)

While you might be connected to Animo, being a client isn't that useful and limits you a lot in what you can do. In order to use the network to its fullest we need to become a [server](https://github.com/libp2p/specs/blob/master/kad-dht/README.md#client-and-server-mode).

In order to become a server there are a couple [requirments](link) we must meet.

See
https://blog.ipfs.tech/2020-07-20-dht-deep-dive/