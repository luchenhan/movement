use std::str::FromStr;
use crate::eth_client::{
    MCR,
    MOVEToken,
    MovementStaking
};
use mcr_settlement_config::Config;
use alloy::providers::{ProviderBuilder, Provider};
use alloy_signer_wallet::LocalWallet;
use alloy_primitives::Address;
use alloy_primitives::U256;
use alloy_network::EthereumSigner;

use godfig::{
    Godfig,
    backend::config_file::ConfigFile
};
use tracing::info;
use anyhow::Context;
use alloy::rpc::types::trace::parity::TraceType;
use alloy_rpc_types::TransactionRequest;


async fn run_genesis_ceremony(
    config : &Config,
    governor: LocalWallet,
    rpc_url: &str,
    move_token_address: Address,
    staking_address: Address,
    mcr_address: Address,
) -> Result<(), anyhow::Error> {

    // Build alice client for MOVEToken, MCR, and staking
    info!("Creating alice client");
    let alice : LocalWallet = config.well_known_accounts.get(1).context("No well known account")?.parse()?;
    let alice_address : Address = config.well_known_addresses.get(1).context("No well known address")?.parse()?;
    let alice_rpc_provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .signer(EthereumSigner::from(alice.clone()))
        .on_http(rpc_url.parse()?);
    let alice_mcr = MCR::new(mcr_address, &alice_rpc_provider);
    let alice_staking = MovementStaking::new(staking_address, &alice_rpc_provider);
    let alice_move_token = MOVEToken::new(move_token_address, &alice_rpc_provider);

    // Build bob client for MOVEToken, MCR, and staking
    info!("Creating bob client");
    let bob: LocalWallet = config.well_known_accounts.get(2).context("No well known account")?.parse()?;
    let bob_rpc_provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .signer(EthereumSigner::from(bob.clone()))
        .on_http(rpc_url.parse()?);
    let bob_mcr = MCR::new(mcr_address, &bob_rpc_provider);
    let bob_staking = MovementStaking::new(staking_address, &bob_rpc_provider);
    let bob_move_token = MOVEToken::new(move_token_address, &bob_rpc_provider);

    // Build the MCR client for staking
    info!("Creating governor client");
    let governor_rpc_provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .signer(EthereumSigner::from(governor.clone()))
        .on_http(rpc_url.parse()?);
    let governor_token = MOVEToken::new(move_token_address, &governor_rpc_provider);
    let governor_staking = MovementStaking::new(staking_address, &governor_rpc_provider);

    // alice stakes for mcr
    info!("Alice stakes for MCR");
    let token_name = governor_token.
        name().call().await.context("Failed to get token name")?;
    info!("Token name: {}", token_name._0);
    let calldata = governor_token.approve(
        alice_address,
        U256::from(100)
    ).calldata().clone();

    let transaction = TransactionRequest::default()
        .from(governor.address())
        .to(move_token_address)
        .input(calldata.into());
    let trace_type = [TraceType::Trace];
    let result = governor_rpc_provider.trace_call(&transaction, &trace_type).await.context("Failed to approve alice")?;


    let calldata = governor_token
        .mint(alice_address, U256::from(100))
        .calldata().clone();

    let transaction = TransactionRequest::default()
        .from(governor.address())
        .to(move_token_address)
        .input(calldata.into());
    let trace_type = [TraceType::Trace];
    let result = governor_rpc_provider.trace_call(&transaction, &trace_type).await.context("Failed to fund alice")?;

    alice_move_token
        .approve(mcr_address, U256::from(100))
        .call()
        .await.context("Alice failed to approve MCR")?;
    alice_staking
        .stake(mcr_address, move_token_address, U256::from(100))
        .call()
        .await.context("Alice failed to stake for MCR")?;

    // bob stakes for mcr
    info!("Bob stakes for MCR");
    governor_token
        .mint(bob.address(), U256::from(100))
        .call()
        .await.context("Governor failed to mint for bob")?;
    bob_move_token
        .approve(mcr_address, U256::from(100))
        .call()
        .await.context("Bob failed to approve MCR")?;
    bob_staking
        .stake(mcr_address, move_token_address, U256::from(100))
        .call()
        .await.context("Bob failed to stake for MCR")?;

    // mcr accepts the genesis
    info!("MCR accepts the genesis");
    governor_staking
        .acceptGenesisCeremony()
        .call()
        .await.context("Governor failed to accept genesis ceremony")?;

    Ok(())
}

#[tokio::test]
pub async fn test_genesis_ceremony() -> Result<(), anyhow::Error> {

    use tracing_subscriber::EnvFilter;

	tracing_subscriber::fmt()
		.with_env_filter(
			EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
		)
		.init();
    
	let dot_movement = dot_movement::DotMovement::try_from_env()?;
	let mut config_file = dot_movement.try_get_or_create_config_file().await?;

	// get a matching godfig object
	let godfig : Godfig<Config, ConfigFile> = Godfig::new(ConfigFile::new(config_file), vec![
        "mcr_settlement".to_string(),
    ]);
    let config : Config = godfig.try_wait_for_ready().await?;

    run_genesis_ceremony(
        &config,
        LocalWallet::from_str(&config.governor_private_key)?,
        &config.eth_rpc_connection_url(),
        Address::from_str(&config.move_token_contract_address)?,
        Address::from_str(&config.movement_staking_contract_address)?,
        Address::from_str(&config.mcr_contract_address)?
    ).await?;

    Ok(())
}

