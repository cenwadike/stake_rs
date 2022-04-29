use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Serialize};
use near_sdk::collections::LookupMap;
use near_sdk::utils::assert_one_yocto;
use near_sdk::json_types::{U128, ValidAccountId};
use near_sdk::{
    env, ext_contract, log, near_bindgen, AccountId, Balance, PanicOnDefault, Timestamp, PromiseOrValue,
};
use uint::construct_uint;

const NO_DEPOSIT: Balance = 0;
const BASE_GAS: u64 = 5_000_000_000_000;
const PROMISE_CALL: u64 = 5_000_000_000_000;
const GAS_FOR_ACCOUNT_REGISTRATION: u64 = BASE_GAS;
const GAS_FOR_ON_TRANSFER: u64 = BASE_GAS + PROMISE_CALL;

construct_uint! {
	pub struct U256(8);
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct Account {
    pub obs_balance: Balance,
    pub reward_balance: Balance,
    pub reward_claimed: Balance,
    pub last_obs_per_reward_rate: Balance,
    pub deposit_time: Timestamp,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct FarmerAccount {
    pub obs_balance: U128,
    pub reward_balance: U128,
    pub reward_claimed: U128,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct FarmStats {
    pub total_obs_balance: U128,
    pub total_reward_claimed: U128,
    pub total_reward_received: U128,
}

// using 10**18 for precision
pub const OBS_PER_REWARD_DENOM: Balance = 1_000_000_000_000_000_000;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Farm {
    pub obs_token_account_id: AccountId,

    pub reward_token_account_id: AccountId,

    pub accounts: LookupMap<ShortAccountHash, Account>,

    pub reward_rate: Balance,

    pub obs_per_reward_rate: Balance,

    pub staking_fee_rate: Balance,

    pub cliff_time: Timestamp,

    pub reward_interval: Timestamp,

    pub total_obs_balance: Balance,

    pub total_reward_farmed: Balance,

    pub total_reward_claimed: Balance,
}

trait FungibleTokenReceiver {
    fn ft_on_transfer(
        &mut self, 
        sender_id: AccountId, 
        amount: U128, 
        msg: String
    ) -> PromiseOrValue<U128>;
}

#[near_bindgen]
impl FungibleTokenReceiver for Farm {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        // Verifying that we were called by fungible token contract that we expect.
        assert_eq!(
            &env::predecessor_account_id(),
            &self.obs_token_account_id,
            "Only supports the one fungible token contract"
        );
        log!("in {} tokens from @{} ft_on_transfer, msg = {}", amount.0, sender_id, msg);
        match msg.as_str() {
            "Stake" => PromiseOrValue::Value(U128::from(0)),
            _ => {
                ext_self::on_transfer(
                    self.obs_token_account_id.clone(),
                    env::predecessor_account_id(),
                    amount.into(),
                    &env::current_account_id(),
                    NO_DEPOSIT,
                    GAS_FOR_ON_TRANSFER,
                )
                .into()
            }
        }
    }
}

// Defining cross-contract interface. This allows to create a new promise.
#[ext_contract(ext_self)]
pub trait ExtFarm {
    fn on_transfer(&mut self, sender: AccountId, receiver: AccountId, amount: Balance) -> PromiseOrValue<()>;
    fn register_account(&mut self, account_id: AccountId);
}

// interface for external call 
#[ext_contract(ext_fungible_token)]
pub trait FungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}


#[derive(BorshDeserialize, BorshSerialize, Clone, PartialEq)]
pub struct ShortAccountHash(pub [u8; 20]);

impl From<&AccountId> for ShortAccountHash {
    fn from(account_id: &AccountId) -> Self {
        let mut buf = [0u8; 20];
        buf.copy_from_slice(&env::sha256(account_id.as_bytes())[..20]);
        Self(buf)
    }
}

#[near_bindgen]
impl Farm {
    #[init]
    pub fn new(
            obs_token_account_id: ValidAccountId,
            reward_token_account_id: ValidAccountId) -> Self {
                // to allow access to obs and reward token contract
                ext_self::register_account(
                    env::current_account_id(),
                    obs_token_account_id.as_ref(),
                    NO_DEPOSIT,
                    GAS_FOR_ACCOUNT_REGISTRATION,
                );
                ext_self::register_account(
                    env::current_account_id(),
                    reward_token_account_id.as_ref(),
                    NO_DEPOSIT,
                    GAS_FOR_ACCOUNT_REGISTRATION,
                );
        assert!(!env::state_exists(), "Already initialized");
        Self { 
            obs_token_account_id: obs_token_account_id.into(),
            reward_token_account_id: reward_token_account_id.into(),
            accounts: LookupMap::new(b"a".to_vec()),
            reward_rate: 1800,
            obs_per_reward_rate: 0,
            staking_fee_rate: 25,
            cliff_time: 60 * 60 * 24 * 10,
            reward_interval: 60 * 60 * 24 * 365,
            total_obs_balance: 0,
            total_reward_farmed: 0,
            total_reward_claimed: 0,
        }
    }

    #[payable]
    pub fn stake_my_obs(&mut self, amount: Balance) {
        assert_one_yocto();
        assert!(
           amount > 0,
           "Amount must be greater than 0",
        );
        
        let fee = amount * self.staking_fee_rate * OBS_PER_REWARD_DENOM;
        let attached_deposit = amount + fee;
        let account_id = env::predecessor_account_id();
        let (_account_id_hash, mut account) = self.get_mut_account(&account_id);

        account.obs_balance = attached_deposit;
        account.reward_balance = 0;
        account.reward_claimed = 0;
        account.last_obs_per_reward_rate = self.touch(&mut account);
        account.deposit_time = env::block_timestamp();

        let time_diff = env::block_timestamp() - self.cliff_time;
        let obs_per_reward =(
            ((U256::from(attached_deposit)
            * U256::from(time_diff) 
            * U256::from(self.reward_rate)) 
            / U256::from(self.reward_interval))
        * U256::from(OBS_PER_REWARD_DENOM))
        .as_u128();

        self.obs_per_reward_rate += obs_per_reward;
        self.total_obs_balance += attached_deposit;

        ext_fungible_token::ft_transfer(
            env::current_account_id(),
            attached_deposit.into(),
            None,
            &self.obs_token_account_id.clone(),
            1,
            GAS_FOR_ON_TRANSFER,
        )
        .then(ext_self::on_transfer (
                self.obs_token_account_id.clone(),
                env::predecessor_account_id(),
                attached_deposit,
                &env::current_account_id(),
                NO_DEPOSIT,
                GAS_FOR_ON_TRANSFER,
            )
        );    
    }

    #[payable]
    pub fn unstake_my_obs(&mut self, amount: Balance) {
        assert_one_yocto();
        let (_account_id_hash, mut account) = self.get_mut_account(&env::predecessor_account_id());
        assert!(
            account.obs_balance >= amount,
        );
        assert!(
            env::block_timestamp() - account.deposit_time >= self.cliff_time,
            "You can unstake only after the 10 days of deposit"
        );

        self.touch(&mut account);

        account.obs_balance -= amount;
        account.reward_claimed = account.reward_balance;
        account.reward_balance = 0;

        self.total_obs_balance -= amount;
        self.total_reward_claimed += amount;
        self.total_reward_claimed += account.reward_claimed;

        let fee = amount * self.staking_fee_rate * OBS_PER_REWARD_DENOM;
        let attached_deposit = amount + fee;
        ext_fungible_token::ft_transfer(
            env::predecessor_account_id(),
            attached_deposit.into(),
            None,
            &self.obs_token_account_id.clone(),
            1,
            GAS_FOR_ON_TRANSFER,
        ).then(
            ext_fungible_token::ft_transfer(
            env::predecessor_account_id(),
            attached_deposit.into(),
            None,
            &self.reward_token_account_id.clone(),
            1,
            GAS_FOR_ON_TRANSFER,
            )
        );
    }

    pub fn on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    )  {
        // Verifying that we were called by fungible token contract that we expect.
        assert_eq!(
            &env::predecessor_account_id(),
            &self.obs_token_account_id,
            "Only supports the one fungible token contract"
        );
        log!("{} tokens from @{} on_transfer, msg = {}", amount.0, sender_id, msg);
    }
    
    pub fn register_account(&mut self) {
        let (account_id_hash, account) = self.get_mut_account(&env::predecessor_account_id());
        self.save_account(&account_id_hash, &account);
    }

    pub fn account_exists(&self, account_id: ValidAccountId) -> bool {
        self.get_internal_account(account_id.as_ref()).1.is_some()
    }

    pub fn get_reward_balance(&mut self, account_id: ValidAccountId) -> U128 {
        self.get_internal_account(account_id.as_ref())
            .1
            .map(|mut account| {
                self.touch(&mut account);
                account.reward_balance
            })
            .unwrap_or(0)
            .into()
    }

    pub fn get_stats(&self) -> FarmStats {
        FarmStats {
            total_obs_balance: self.total_obs_balance.into(),
            total_reward_claimed: self.total_reward_claimed.into(),
            total_reward_received: self.total_reward_farmed.into(),
        }
    }
}

impl Farm {
    fn get_internal_account(&self, account_id: &AccountId) -> (ShortAccountHash, Option<Account>) {
        let account_id_hash: ShortAccountHash = account_id.into();
        let account = self.accounts.get(&account_id_hash);
        (account_id_hash, account)
    }

    /// updating inner pool balances.
    fn touch(&mut self, account: &mut Account) -> Balance {
        let current_time = env::block_timestamp();
        let time_diff = current_time - account.deposit_time;
        let earned_balance = (
                ((U256::from(account.obs_balance)
                * U256::from(time_diff) 
                * U256::from(self.reward_rate)) 
                / U256::from(self.reward_interval))
            * U256::from(OBS_PER_REWARD_DENOM))
            .as_u128();
        if time_diff > self.cliff_time.into() {
            account.reward_balance += earned_balance;
            self.total_reward_farmed += earned_balance;
        };
        return account.last_obs_per_reward_rate;
    }

    fn get_mut_account(&mut self, account_id: &AccountId) -> (ShortAccountHash, Account) {
        let (account_id_hash, account) = self.get_internal_account(&account_id);
        let mut account = account.unwrap_or_else(|| Account {
            last_obs_per_reward_rate: self.obs_per_reward_rate,
            obs_balance: 0,
            reward_balance: 0,
            reward_claimed: 0,
            deposit_time: 0,
        });
        self.touch(&mut account);
        (account_id_hash, account)
    }

    fn save_account(&mut self, account_id_hash: &ShortAccountHash, account: &Account) {
        self.accounts.insert(account_id_hash, account);
    }
}
