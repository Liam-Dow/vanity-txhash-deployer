use ethers::prelude::*;
use std::convert::TryFrom;
use std::env;
use eyre::Result;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    // Get RPC URL from .env
    let rpc_url = env::var("RPC")?;
    let provider = Provider::<Http>::try_from(rpc_url)?;

    // Get latest block to get base fee
    let block = provider.get_block(BlockNumber::Latest).await?.unwrap();
    let base_fee = block.base_fee_per_gas.unwrap_or_default();
    
    // Get fee history for priority fee estimation
    let fee_history = provider
        .fee_history(10, BlockNumber::Latest, &[10.0])
        .await?;
    
    // Calculate average priority fee from recent blocks
    let priority_fees: Vec<U256> = fee_history.reward.iter()
        .flat_map(|reward| reward.first().cloned())
        .collect();
    
    let avg_priority_fee = if !priority_fees.is_empty() {
        let sum = priority_fees.iter().fold(U256::zero(), |acc, &x| acc + x);
        sum / U256::from(priority_fees.len())
    } else {
        U256::zero()
    };

    // Convert to Gwei (1 Gwei = 10^9 wei)
    let base_fee_gwei = base_fee.as_u128() as f64 / 1_000_000_000.0;
    let priority_fee_gwei = avg_priority_fee.as_u128() as f64 / 1_000_000_000.0;

    println!("Current Base Network Gas Prices:");
    println!("Base Fee: {:.5} Gwei", base_fee_gwei);
    println!("Priority Fee: {:.5} Gwei", priority_fee_gwei);
    println!("Total: {:.5} Gwei", base_fee_gwei + priority_fee_gwei);

    Ok(())
}
