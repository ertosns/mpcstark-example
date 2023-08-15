# MPC-STARK example

spdz mpc with wrapped stark using mpc_stark crate,

``` rust
async fn main() {

    let mut rng = thread_rng();
    // 2pc shares
    let party0_val = Scalar::random(&mut rng);
    let party1_val = Scalar::random(&mut rng);
    // mock fabric networking
    let (_, _) = execute_mock_mpc(|fabric| async move {
        let a = fabric.share_scalar(party0_val, PARTY0 /* sender */);
        let b = fabric.share_scalar(party1_val, PARTY1 /* receiver */);
        let c = a * b;
        // authenticate opend output
        let output_c = c.open_authenticated().await.expect("");
        assert!(party0_val*party1_val == output_c);
    })
        .await;

}
```
