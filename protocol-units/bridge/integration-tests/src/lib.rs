#![allow(dead_code)]
use alloy::node_bindings::AnvilInstance;
use alloy::primitives::Address;
use alloy::primitives::{keccak256, U256};
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use alloy_network::EthereumWallet;
use aptos_sdk::rest_client::aptos_api_types::Transaction as AptosTransaction;
use aptos_sdk::rest_client::{Client, FaucetClient};
use aptos_sdk::types::account_address::AccountAddress;
use aptos_sdk::types::LocalAccount;
use bridge_config::Config;
use bridge_service::chains::bridge_contracts::BridgeContractError;
use bridge_service::chains::ethereum::types::AlloyProvider;
use bridge_service::chains::ethereum::types::EthAddress;
use bridge_service::chains::ethereum::{client::EthClient, types::EthHash};
use bridge_service::chains::movement::utils::MovementAddress;
use bridge_service::chains::movement::{client::MovementClient, client_framework::MovementClientFramework, utils::MovementHash};
use bridge_service::types::Amount;
use bridge_service::types::BridgeTransferId;
use bridge_service::types::HashLockPreImage;
use bridge_service::{chains::bridge_contracts::BridgeContractResult, types::BridgeAddress};
use godfig::{backend::config_file::ConfigFile, Godfig};
use rand::SeedableRng;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use url::Url;

pub mod utils;

#[derive(Clone)]
pub struct EthToMovementCallArgs {
	pub initiator: Vec<u8>,
	pub recipient: MovementAddress,
	pub bridge_transfer_id: MovementHash,
	pub hash_lock: MovementHash,
	pub time_lock: u64,
	pub amount: u64,
}

#[derive(Clone)]
pub struct MovementToEthCallArgs {
	pub initiator: MovementAddress,
	pub recipient: Vec<u8>,
	pub bridge_transfer_id: EthHash,
	pub hash_lock: EthHash,
	pub time_lock: u64,
	pub amount: u64,
}

impl Default for EthToMovementCallArgs {
	fn default() -> Self {
		Self {
			initiator: b"32Be343B94f860124dC4fEe278FDCBD38C102D88".to_vec(),
			recipient: MovementAddress(AccountAddress::new(*b"0x00000000000000000000000000face")),
			bridge_transfer_id: MovementHash(*b"00000000000000000000000transfer9"),
			hash_lock: MovementHash(*keccak256(b"secret")),
			time_lock: 3600,
			amount: 100,
		}
	}
}

impl Default for MovementToEthCallArgs {
	fn default() -> Self {
		Self {
			initiator: MovementAddress(AccountAddress::new(*b"0x000000000000000000000000A55018")),
			recipient: b"32Be343B94f860124dC4fEe278FDCBD38C102D88".to_vec(),
			bridge_transfer_id: EthHash(*b"00000000000000000000000transfer1"),
			hash_lock: EthHash(*keccak256(b"secret")),
			time_lock: 3600,
			amount: 100,
		}
	}
}

pub struct HarnessEthClient {
	pub eth_rpc_url: String,
	pub signer_private_key: PrivateKeySigner,
	pub eth_client: EthClient,
}

impl HarnessEthClient {
	pub async fn build(config: &Config) -> Self {
		let eth_rpc_url = config.eth.eth_rpc_connection_url().clone();

		let signer_private_key = config
			.eth
			.signer_private_key
			.parse::<PrivateKeySigner>()
			.expect("Error during parsing signer private key?");

		let eth_client = EthClient::new(&config.eth).await.expect("Failed to create EthClient");
		HarnessEthClient { eth_client, eth_rpc_url, signer_private_key }
	}

	pub async fn rpc_provider(&self) -> AlloyProvider {
		let rpc_provider = ProviderBuilder::new()
			.with_recommended_fillers()
			.wallet(EthereumWallet::from(self.signer_private_key.clone()))
			.on_builtin(&self.eth_rpc_url)
			.await
			.expect("Error during provider creation");
		rpc_provider
	}

	pub fn signer_address(&self) -> Address {
		self.signer_private_key.address()
	}

	pub fn get_initiator_private_key(config: &Config) -> PrivateKeySigner {
		let signer_private_key = config.testing.eth_well_known_account_private_keys[2]
			.clone()
			.parse::<PrivateKeySigner>()
			.unwrap();
		signer_private_key
	}

	pub fn get_initiator_address(config: &Config) -> Address {
		HarnessEthClient::get_initiator_private_key(config).address()
	}

	pub fn get_recipient_private_key(config: &Config) -> PrivateKeySigner {
		let signer_private_key = config.testing.eth_well_known_account_private_keys[3]
			.clone()
			.parse::<PrivateKeySigner>()
			.unwrap();
		signer_private_key
	}

	pub fn get_recipeint_address(config: &Config) -> Address {
		HarnessEthClient::get_recipient_private_key(config).address()
	}

	pub async fn deposit_weth_and_approve(
		&mut self,
		initiator_address: BridgeAddress<EthAddress>,
		amount: Amount,
	) -> BridgeContractResult<()> {
		self.eth_client
			.deposit_weth_and_approve(initiator_address.0 .0, U256::from(amount.weth_value()))
			.await
			.expect("Failed to deposit WETH");
		Ok(())
	}
}

pub struct HarnessMvtClient {
	pub movement_client: MovementClient,
	///The Apotos Rest Client
	pub rest_client: Client,
	///The Apotos Rest Client
	pub faucet_client: Arc<RwLock<FaucetClient>>,
}

impl HarnessMvtClient {
	pub fn gen_aptos_account() -> Vec<u8> {
		let mut rng = ::rand::rngs::StdRng::from_seed([3u8; 32]);
		let movement_recipient = LocalAccount::generate(&mut rng);
		movement_recipient.public_key().to_bytes().to_vec()
	}

	pub async fn build(config: &Config) -> Self {
		let movement_client = MovementClient::new(&config.movement)
			.await
			.expect("Failed to create MovementClient");

		let node_connection_url = Url::from_str(&config.movement.mvt_rpc_connection_url())
			.expect("Bad movement rpc url in config");
		let rest_client = Client::new(node_connection_url.clone());

		let faucet_url = Url::from_str(&config.movement.mvt_faucet_connection_url())
			.expect("Bad movement faucet url in config");
		let faucet_client = Arc::new(RwLock::new(FaucetClient::new(
			faucet_url.clone(),
			node_connection_url.clone(),
		)));

		HarnessMvtClient { movement_client, rest_client, faucet_client }
	}

	pub async fn fund_account(&self) -> LocalAccount {
		let account = LocalAccount::generate(&mut rand::rngs::OsRng);
		self.faucet_client
			.write()
			.unwrap()
			.fund(account.address(), 100_000_000)
			.await
			.expect("Failed to fund account");
		account
	}

	pub async fn counterparty_complete_bridge_transfer(
		&mut self,
		recipient_privatekey: LocalAccount,
		bridge_transfer_id: BridgeTransferId,
		preimage: HashLockPreImage,
	) -> BridgeContractResult<AptosTransaction> {
		let unpadded_preimage = {
			let mut end = preimage.0.len();
			while end > 0 && preimage.0[end - 1] == 0 {
				end -= 1;
			}
			&preimage.0[..end]
		};
		let args2 = vec![
			bridge_service::chains::movement::utils::serialize_vec(&bridge_transfer_id.0[..])?,
			bridge_service::chains::movement::utils::serialize_vec(&unpadded_preimage)?,
		];

		let payload = bridge_service::chains::movement::utils::make_aptos_payload(
			self.movement_client.native_address,
			bridge_service::chains::movement::client::COUNTERPARTY_MODULE_NAME,
			"complete_bridge_transfer",
			Vec::new(),
			args2,
		);

		bridge_service::chains::movement::utils::send_and_confirm_aptos_transaction(
			&self.rest_client,
			&recipient_privatekey,
			payload,
		)
		.await
		.map_err(|_| BridgeContractError::CompleteTransferError)
	}
}

pub struct TestHarness;
impl TestHarness {
	pub async fn read_bridge_config() -> Result<Config, anyhow::Error> {
		let mut dot_movement = dot_movement::DotMovement::try_from_env()?;
		let pathbuff = bridge_config::get_config_path(&dot_movement);
		dot_movement.set_path(pathbuff);
		let config_file = dot_movement.try_get_or_create_config_file().await?;

		// get a matching godfig object
		let godfig: Godfig<Config, ConfigFile> = Godfig::new(ConfigFile::new(config_file), vec![]);
		let bridge_config: Config = godfig.try_wait_for_ready().await?;
		Ok(bridge_config)
	}

	pub async fn new_with_eth_and_movement(
	) -> Result<(HarnessEthClient, HarnessMvtClient, Config), anyhow::Error> {
		let config = TestHarness::read_bridge_config().await?;

		let test_mvt_harness = HarnessMvtClient::build(&config).await;
		let test_eth_harness = HarnessEthClient::build(&config).await;

		Ok((test_eth_harness, test_mvt_harness, config))
	}

	pub async fn new_with_movement(
		config: Config,
	) -> (HarnessMvtClient, Config, tokio::process::Child) {
		let (config, movement_process) = bridge_setup::test_mvt_setup(config)
			.await
			.expect("Failed to setup Movement config");

		let test_harness = HarnessMvtClient::build(&config).await;

		(test_harness, config, movement_process)
	}

	pub async fn new_only_eth(config: Config) -> (HarnessEthClient, Config, AnvilInstance) {
		let (config, anvil) = bridge_setup::test_eth_setup(config)
			.await
			.expect("Test eth config setup failed.");
		let test_hadness = HarnessEthClient::build(&config).await;
		(test_hadness, config, anvil)
	}
}

pub struct HarnessMvtClientFramework {
	pub movement_client: MovementClientFramework,
	///The Apotos Rest Client
	pub rest_client: Client,
	///The Apotos Rest Client
	pub faucet_client: Arc<RwLock<FaucetClient>>,
}

impl HarnessMvtClientFramework {
	pub fn gen_aptos_account() -> Vec<u8> {
		let mut rng = ::rand::rngs::StdRng::from_seed([3u8; 32]);
		let movement_recipient = LocalAccount::generate(&mut rng);
		movement_recipient.public_key().to_bytes().to_vec()
	}

	pub async fn build(config: &Config) -> Self {
		let movement_client = MovementClientFramework::new(&config.movement)
			.await
			.expect("Failed to create MovementClient");

		let node_connection_url = Url::from_str(&config.movement.mvt_rpc_connection_url())
			.expect("Bad movement rpc url in config");
		let rest_client = Client::new(node_connection_url.clone());

		let faucet_url = Url::from_str(&config.movement.mvt_faucet_connection_url())
			.expect("Bad movement faucet url in config");
		let faucet_client = Arc::new(RwLock::new(FaucetClient::new(
			faucet_url.clone(),
			node_connection_url.clone(),
		)));

		HarnessMvtClientFramework { movement_client, rest_client, faucet_client }
	}
}

pub struct TestHarnessFramework;
impl TestHarnessFramework {
	pub async fn read_bridge_config() -> Result<Config, anyhow::Error> {
		let mut dot_movement = dot_movement::DotMovement::try_from_env()?;
		let pathbuff = bridge_config::get_config_path(&dot_movement);
		dot_movement.set_path(pathbuff);
		let config_file = dot_movement.try_get_or_create_config_file().await?;

		// get a matching godfig object
		let godfig: Godfig<Config, ConfigFile> = Godfig::new(ConfigFile::new(config_file), vec![]);
		let bridge_config: Config = godfig.try_wait_for_ready().await?;
		Ok(bridge_config)
	}

	pub async fn new_with_eth_and_movement(
	) -> Result<(HarnessEthClient, HarnessMvtClientFramework, Config), anyhow::Error> {
		let config = TestHarnessFramework::read_bridge_config().await?;

		let test_mvt_harness = HarnessMvtClientFramework::build(&config).await;
		let test_eth_harness = HarnessEthClient::build(&config).await;

		Ok((test_eth_harness, test_mvt_harness, config))
	}

	pub async fn new_with_movement(
		config: Config,
	) -> (HarnessMvtClientFramework, Config, tokio::process::Child) {
		let (config, movement_process) = bridge_setup::test_mvt_setup(config)
			.await
			.expect("Failed to setup Movement config");

		let test_harness = HarnessMvtClientFramework::build(&config).await;

		(test_harness, config, movement_process)
	}

	pub async fn new_with_suzuka(
		config: Config,
	) -> (HarnessMvtClientFramework, Config) {
		let test_harness = HarnessMvtClientFramework::build(&config).await;
		(test_harness, config)
	}

	pub async fn new_only_eth(config: Config) -> (HarnessEthClient, Config, AnvilInstance) {
		let (config, anvil) = bridge_setup::test_eth_setup(config)
			.await
			.expect("Test eth config setup failed.");
		let test_harness = HarnessEthClient::build(&config).await;
		(test_harness, config, anvil)
	}
}
