export RUST_LOG=info


CLIENT_TERM="sleep 0.5; cd `pwd`; cargo run --bin message -- -r /ip4/127.0.0.1/tcp/8011"
kitty --detach sh -c "$CLIENT_TERM"

cargo run --bin server