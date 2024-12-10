use ethers::prelude::*;
use ethers::types::{
    transaction::eip1559::Eip1559TransactionRequest,
    transaction::eip2718::TypedTransaction,
    Address, Bytes, U256,
};
use std::convert::TryFrom;
use std::env;
use std::io::{self, Write};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::mpsc;
use parking_lot::RwLock;
use rlp::RlpStream; 
// Constants for optimization
const BUFFER_SIZE: usize = 1024;
const BATCH_SIZE: usize = 1000;
const DEFAULT_THREAD_COUNT: usize = 8;
const THREAD_OFFSET_SPACING: u64 = 100_000_000;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    dotenv::dotenv().ok();

    // Load environment variables
    let private_key = env::var("PRIVATE_KEY")?;
    let rpc_url = env::var("RPC")?;
    let chain_id: u64 = env::var("CHAIN_ID")?.parse()?;
    let hash_prefix = env::var("HASH_PREFIX")?.to_lowercase();
    let calldata = env::var("CALLDATA")?;
    let gas_limit: U256 = env::var("GAS_LIMIT")?.parse::<u64>()?.into();

    let wallet: LocalWallet = private_key.parse::<LocalWallet>()?.with_chain_id(chain_id);
    let provider = Provider::<Http>::try_from(rpc_url.clone())?;
    let client = Arc::new(SignerMiddleware::new(provider, wallet.clone()));

    let from = client.default_sender().expect("no sender address found");
    let nonce = client.get_transaction_count(from, None).await?;
    let contract_address = get_contract_address(from, nonce);

    // Base fee and priority fee configuration
    let base_fee_start = U256::from(18_000_000u64);
    let priority_fee = U256::from(1_250_000u64);

    // Prepare transaction template
    let mut eip1559_tx = Eip1559TransactionRequest::new();
    eip1559_tx.to = None;
    eip1559_tx.data = Some(calldata.parse::<Bytes>()?);
    eip1559_tx.nonce = Some(nonce);
    eip1559_tx.gas = Some(gas_limit);
    eip1559_tx.chain_id = Some(chain_id.into());

    println!("Starting parallel search for transaction hash with prefix: {}", hash_prefix);

    let thread_count = num_cpus::get().min(DEFAULT_THREAD_COUNT);
    let (tx_result, mut rx_result) = mpsc::channel::<(Bytes, [u8; 32], U256)>(BUFFER_SIZE);
    let found = Arc::new(AtomicBool::new(false));
    let tx_template = Arc::new(RwLock::new(eip1559_tx.clone()));

    let tasks: Vec<_> = (0..thread_count)
        .map(|i| {
            let wallet_clone = wallet.clone();
            let hash_prefix = hash_prefix.clone();
            let tx_result = tx_result.clone();
            let found = found.clone();
            let tx_template = tx_template.clone();
            let base_fee_start = base_fee_start;
            let priority_fee = priority_fee;
            let gas_limit = gas_limit;
            
            tokio::spawn(async move {
                let base_fee_offset = U256::from(i as u64 * THREAD_OFFSET_SPACING);
                let mut base_fee = base_fee_start + base_fee_offset;
                let mut batch = Vec::with_capacity(BATCH_SIZE);

                while !found.load(Ordering::Relaxed) {
                    batch.clear();
                    
                    for _ in 0..BATCH_SIZE {
                        let mut tx = tx_template.read().clone();
                        tx.max_fee_per_gas = Some(base_fee + priority_fee);
                        tx.max_priority_fee_per_gas = Some(priority_fee);
                        batch.push(tx);
                        base_fee = base_fee.saturating_add(U256::one());
                    }

                    if let Some((signed_rlp, tx_hash, total_fee_wei)) = process_batch(
                        &batch,
                        &wallet_clone,
                        &hash_prefix,
                        gas_limit,
                        &found,
                    ).await? {
                        let _ = tx_result.send((signed_rlp, tx_hash, total_fee_wei)).await;
                        break;
                    }
                }
                Ok::<_, eyre::Report>(())
            })
        })
        .collect();

    for task in tasks {
        if let Ok(result) = task.await {
            if result.is_ok() {
                break;
            }
        }
    }

    if let Some((signed_rlp, tx_hash_bytes, total_fee_wei)) = rx_result.recv().await {
        let tx_hash_hex = format!("0x{}", hex::encode(tx_hash_bytes));
        let total_fee_eth = wei_to_eth(total_fee_wei);

        println!("Match found!");
        println!("Transaction Hash: {}", tx_hash_hex);
        println!("Contract Address: {:?}", contract_address);
        println!("Estimated Gas Cost: {} ETH", total_fee_eth);

        print!("Send this transaction? (y/n): ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        if input.trim().to_lowercase() == "y" {
            let provider = Provider::<Http>::try_from(rpc_url)?;
            let pending_tx = provider.send_raw_transaction(signed_rlp).await?;
            let receipt = pending_tx.await?;
            println!("Transaction sent! Receipt: {:?}", receipt);
        } else {
            println!("Aborted by user.");
        }
    } else {
        println!("No solution found (interrupted?)");
    }

    Ok(())
}

async fn process_batch(
    batch: &[Eip1559TransactionRequest],
    wallet: &LocalWallet,
    hash_prefix: &str,
    gas_limit: U256,
    found: &AtomicBool,
) -> eyre::Result<Option<(Bytes, [u8; 32], U256)>> {
    for tx in batch {
        if found.load(Ordering::Relaxed) {
            return Ok(None);
        }

        if let Ok((signed_rlp, tx_hash)) = encode_and_sign_eip1559(wallet, tx).await {
            let tx_hash_hex = format!("0x{}", hex::encode(tx_hash));
            if tx_hash_hex.starts_with(hash_prefix) {
                if !found.swap(true, Ordering::Relaxed) {
                    let total_fee_wei = gas_limit * tx.max_fee_per_gas.unwrap_or_default();
                    return Ok(Some((signed_rlp, tx_hash, total_fee_wei)));
                }
                break;
            }
        }
    }
    Ok(None)
}

async fn encode_and_sign_eip1559(
    wallet: &LocalWallet,
    eip1559_tx: &Eip1559TransactionRequest,
) -> eyre::Result<(Bytes, [u8; 32])> {
    // Convert to TypedTransaction
    let typed_tx = TypedTransaction::Eip1559(eip1559_tx.clone());
    
    // Sign the transaction
    let signature = wallet.sign_transaction(&typed_tx).await?;
    
    // Get the signed transaction bytes and hash
    let signed_tx = typed_tx.rlp_signed(&signature);
    let tx_hash: [u8; 32] = typed_tx.hash(&signature).into();

    Ok((signed_tx, tx_hash))
}

fn get_contract_address(sender: Address, nonce: U256) -> Address {
    use tiny_keccak::{Hasher, Keccak};
    let mut stream = RlpStream::new_list(2);
    stream.append(&sender);
    stream.append(&nonce);
    let out = stream.out();

    let mut hasher = Keccak::v256();
    hasher.update(&out);
    let mut hash = [0u8; 32];
    hasher.finalize(&mut hash);

    Address::from_slice(&hash[12..])
}

fn wei_to_eth(value: U256) -> f64 {
    const WEI_IN_ETH: f64 = 1e18;
    let wei_str = value.to_string();
    let wei_f64 = wei_str.parse::<f64>().unwrap_or(f64::MAX);
    wei_f64 / WEI_IN_ETH
}