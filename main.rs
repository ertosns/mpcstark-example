use mpc_stark::{
    algebra::scalar::Scalar, beaver::SharedValueSource, network::QuicTwoPartyNet, MpcFabric,
    PARTY0, PARTY1,
};
use rand::thread_rng;

#[tokio::main]
async fn main() {
    // Beaver source should be defined outside of the crate and rely on separate infrastructure
    let beaver = BeaverSource::new();

    let local_addr = "127.0.0.1:8000".parse().unwrap();
    let peer_addr = "127.0.0.1:9000".parse().unwrap();
    let network = QuicTwoPartyNet::new(PARTY0, local_addr, peer_addr);

    // MPC circuit
    let mut rng = thread_rng();
    let my_val = Scalar::random(&mut rng);
    println!("val: {my_val}");
    let fabric = MpcFabric::new(network, beaver);

    let a = fabric.share_scalar(my_val, PARTY0 /* sender */); // party0 value
    let b = fabric.share_scalar(my_val, PARTY1 /* sender */); // party1 value
    let c = a * b;

    let res = c.open_authenticated().await.expect("authentication error");
    println!("a * b = {res}");
}
