A high-performance Ethereum vanity transaction hash generator designed to brute-force transaction hashes for contract deployments on any EVM-compatible blockchain. It operates by incrementally adjusting the transaction's gas price by 1 wei per attempt, parallelised across multiple threads with a 0.1 gwei offset per thread—providing each worker thread with a total search space of 100,000,000 transaction hashes. Once your desired transaction hash is identified, the contract deployment transaction can be executed, leveraging the deterministic nature of EVM blockchains. Although it has no real-world application, it was an enjoyable project to develop.

Example transaction: https://basescan.org/tx/0xba5ed2a73cd2123feeb6c6aa2599007c6d6164847453056e3670c52f14e8f6c2

Note: The example provided above also includes a vanity contract address and a vanity EOA, which are beyond the scope of this project. For generating the vanity contract address, I used [1inch's updated fork of profanity2](https://github.com/1inch/profanity2). I created a separate script for the EOA vanity address. However, in general, vanity EOAs are less secure than random addresses, so I wouldn't recommend using them.

## Features

- Generate vanity transaction hashes for contract deployments
- Parallel processing for faster hash generation
- Compatible with any EVM that uses EIP-1559 for transaction fees
- Gas price monitoring utility based on most recent block (gas_checker.rs)

## Prerequisites

- Rust and Cargo installed
- Your EVM contract bytecode
- EVM RPC endpoint
- Private key for transaction signing

## Setup

1. Clone the repository
2. Create a `.env` file with the following variables:

```env
PRIVATE_KEY=your_private_key_here
RPC=your_rpc_endpoint_url
CHAIN_ID=your_chain_id
HASH_PREFIX=desired_transaction_hash_prefix
CALLDATA=your_contract_bytecode
GAS_LIMIT=set_your_max_spend
```

**Note:** Starting gas price for base and priority fee are set in `main.rs` (line 44 + 45), and each thread is offset by 0.1 gwei (see `THREAD_OFFSET_SPACING` line 21). These values worked well for me during testing on Base Sepolia - adjust as needed for the target EVM. 

## Usage

### Gas Price Checker

Check current network gas prices and adjust main.rs if needed:

```bash
cargo run --bin gas_checker
```
### Vanity Contract Deployment

Run the main program to deploy your contract with a custom transaction hash prefix:

```bash
cargo run
```

Once a match is found you'll see the transaction hash, contract address and estimated gas cost in your console and need to confirm for the transaction to be broadcast. 
