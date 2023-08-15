use mpc_stark::{
    algebra::scalar::Scalar, beaver::SharedValueSource, network::{MpcNetwork}, MpcFabric, PARTY0, PARTY1,
};
use mpc_stark::error::MpcNetworkError;
use rand::thread_rng;
use std::{
    pin::Pin,
    task::{Context, Poll},
};


use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use mpc_stark::network::NetworkOutbound;

use mpc_stark::network::PartyId;
use futures::{Future, Sink, Stream};
use async_trait::async_trait;

/// An implementation of a beaver value source that returns
/// beaver triples (0, 0, 0) for party 0 and (1, 1, 1) for party 1
#[derive(Clone, Debug, Default)]
pub struct PartyIDBeaverSource {
    /// The ID of the local party
    party_id: u64,
}

impl PartyIDBeaverSource {
    /// Create a new beaver source given the local party_id
    pub fn new(party_id: u64) -> Self {
        Self { party_id }
    }
}

/// The PartyIDBeaverSource returns beaver triplets split statically between the
/// parties. We assume a = 2, b = 3 ==> c = 6. [a] = (1, 1); [b] = (3, 0) [c] = (2, 4)
impl SharedValueSource for PartyIDBeaverSource {
    fn next_shared_bit(&mut self) -> Scalar {
        // Simply output partyID, assume partyID \in {0, 1}
        assert!(self.party_id == 0 || self.party_id == 1);
        Scalar::from(self.party_id)
    }

    fn next_triplet(&mut self) -> (Scalar, Scalar, Scalar) {
        if self.party_id == 0 {
            (Scalar::from(1u64), Scalar::from(3u64), Scalar::from(2u64))
        } else {
            (Scalar::from(1u64), Scalar::from(0u64), Scalar::from(4u64))
        }
    }

    fn next_shared_inverse_pair(&mut self) -> (Scalar, Scalar) {
        (Scalar::from(self.party_id), Scalar::from(self.party_id))
    }

    fn next_shared_value(&mut self) -> Scalar {
        Scalar::from(self.party_id)
    }
}

/// A dummy MPC network that operates over a duplex channel instead of a network connection/// An unbounded duplex channel used to mock a network connection
pub struct UnboundedDuplexStream {
    /// The send side of the stream
    send: UnboundedSender<NetworkOutbound>,
    /// The receive side of the stream
    recv: UnboundedReceiver<NetworkOutbound>,
}

impl UnboundedDuplexStream {
    /// Create a new pair of duplex streams
    pub fn new_duplex_pair() -> (Self, Self) {
        let (send1, recv1) = unbounded_channel();
        let (send2, recv2) = unbounded_channel();

        (
            Self {
                send: send1,
                recv: recv2,
            },
            Self {
                send: send2,
                recv: recv1,
            },
        )
    }

    /// Send a message on the stream
    pub fn send(&mut self, msg: NetworkOutbound) {
        self.send.send(msg).unwrap();
    }

    /// Recv a message from the stream
    pub async fn recv(&mut self) -> NetworkOutbound {
        self.recv.recv().await.unwrap()
    }
}


/// A dummy network implementation used for unit testing
pub struct MockNetwork {
    /// The ID of the local party
    party_id: PartyId,
    /// The underlying mock network connection
    mock_conn: UnboundedDuplexStream,
}

impl MockNetwork {
    /// Create a new mock network from one half of a duplex stream
    pub fn new(party_id: PartyId, stream: UnboundedDuplexStream) -> Self {
        Self {
            party_id,
            mock_conn: stream,
        }
    }
}

#[async_trait]
impl MpcNetwork for MockNetwork {
    fn party_id(&self) -> PartyId {
        self.party_id
    }

    async fn close(&mut self) -> Result<(), MpcNetworkError> {
        Ok(())
    }
}

impl Stream for MockNetwork {
    type Item = Result<NetworkOutbound, MpcNetworkError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Box::pin(self.mock_conn.recv())
            .as_mut()
            .poll(cx)
            .map(|value| Some(Ok(value)))
    }
}

impl Sink<NetworkOutbound> for MockNetwork {
    type Error = MpcNetworkError;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: NetworkOutbound) -> Result<(), Self::Error> {
        self.mock_conn.send(item);
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

pub async fn execute_mock_mpc<T, S, F>(mut f: F) -> (T, T)
where
    T: Send + 'static,
S: Future<Output = T> + Send + 'static,
F: FnMut(MpcFabric) -> S,
{
    // Build a duplex stream to broker communication between the two parties
    let (party0_stream, party1_stream) = UnboundedDuplexStream::new_duplex_pair();
    let party0_fabric = MpcFabric::new(
        MockNetwork::new(PARTY0, party0_stream),
        PartyIDBeaverSource::new(PARTY0),
    );
    let party1_fabric = MpcFabric::new(
        MockNetwork::new(PARTY1, party1_stream),
        PartyIDBeaverSource::new(PARTY1),
    );

    // Spawn two tasks to execute the MPC
    let fabric0 = party0_fabric.clone();
    let fabric1 = party1_fabric.clone();
    let party0_task = tokio::spawn(f(fabric0));
    let party1_task = tokio::spawn(f(fabric1));

    let party0_output = party0_task.await.unwrap();
    let party1_output = party1_task.await.unwrap();

    // Shutdown the fabrics
    party0_fabric.shutdown();
    party1_fabric.shutdown();

    (party0_output, party1_output)
}

#[tokio::main]
async fn main() {

    let mut rng = thread_rng();
    let party0_val = Scalar::random(&mut rng);
    let party1_val = Scalar::random(&mut rng);
    let (_, _) = execute_mock_mpc(|fabric| async move {
        //
        let a = fabric.share_scalar(party0_val, PARTY0 /* sender */);
        let b = fabric.share_scalar(party1_val, PARTY1 /* receiver */);
        let c = a * b;
        let output_c = c.open_authenticated().await.expect("");
        assert!(party0_val*party1_val == output_c);
    })
        .await;

}
