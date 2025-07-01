//! Plugin trait for rent collection within the Solana SVM.

use {
    crate::rent_state::RentState,
    serde_derive::{Deserialize, Serialize},
    solana_account::{AccountSharedData, ReadableAccount, WritableAccount},
    solana_clock::{Epoch, DEFAULT_TICKS_PER_SLOT},
    solana_epoch_schedule::EpochSchedule,
    solana_poh_config::PohConfig,
    solana_pubkey::Pubkey,
    solana_rent::{Rent, RentDue},
    solana_sdk_ids::incinerator,
    solana_time_utils::years_as_slots,
    solana_transaction_context::{IndexOfAccount, TransactionContext},
    solana_transaction_error::{TransactionError, TransactionResult},
};

#[cfg(feature = "rent-collector")]
mod rent_collector;

/// Information computed during rent collection
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
pub struct CollectedInfo {
    /// Amount of rent collected from account
    pub rent_amount: u64,
    /// Size of data reclaimed from account (happens when account's lamports go to zero)
    pub account_data_len_reclaimed: u64,
}

impl std::ops::Add for CollectedInfo {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self {
            rent_amount: self.rent_amount.saturating_add(other.rent_amount),
            account_data_len_reclaimed: self
                .account_data_len_reclaimed
                .saturating_add(other.account_data_len_reclaimed),
        }
    }
}

impl std::ops::AddAssign for CollectedInfo {
    #![allow(clippy::arithmetic_side_effects)]
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

/// NOTE: Mimics the behavior of `solana_rent_collector::RentCollector` without depending on that
/// crate (which has since been deprecated) or `solana_runtime` as it cannot be used inside ZK environments.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RentCollector {
    pub epoch: Epoch,
    pub epoch_schedule: EpochSchedule,
    pub slots_per_year: f64,
    pub rent: Rent,
}

impl Default for RentCollector {
    fn default() -> Self {
        Self {
            epoch: Epoch::default(),
            epoch_schedule: EpochSchedule::default(),
            // derive default value like in GenesisConfig::default()
            slots_per_year: years_as_slots(
                1.0,
                &PohConfig::default().target_tick_duration,
                DEFAULT_TICKS_PER_SLOT,
            ),
            rent: Rent::default(),
        }
    }
}

/// When rent is collected from an exempt account, rent_epoch is set to this
/// value. The idea is to have a fixed, consistent value for rent_epoch for all accounts that do not collect rent.
/// This enables us to get rid of the field completely.
pub const RENT_EXEMPT_RENT_EPOCH: Epoch = Epoch::MAX;

/// when rent is collected for this account, this is the action to apply to the account
#[derive(Debug)]
enum RentResult {
    /// this account will never have rent collected from it
    Exempt,
    /// maybe we collect rent later, but not now
    NoRentCollectionNow,
    /// collect rent
    CollectRent {
        new_rent_epoch: Epoch,
        rent_due: u64, // lamports, could be 0
    },
}

impl RentCollector {
    pub fn new(
        epoch: Epoch,
        epoch_schedule: EpochSchedule,
        slots_per_year: f64,
        rent: Rent,
    ) -> Self {
        Self {
            epoch,
            epoch_schedule,
            slots_per_year,
            rent,
        }
    }

    pub fn clone_with_epoch(&self, epoch: Epoch) -> Self {
        Self {
            epoch,
            ..self.clone()
        }
    }

    /// true if it is easy to determine this account should consider having rent collected from it
    pub fn should_collect_rent(&self, address: &Pubkey, executable: bool) -> bool {
        !(executable // executable accounts must be rent-exempt balance
            || *address == incinerator::id())
    }

    /// given an account that 'should_collect_rent'
    /// returns (amount rent due, is_exempt_from_rent)
    pub fn get_rent_due(
        &self,
        lamports: u64,
        data_len: usize,
        account_rent_epoch: Epoch,
    ) -> RentDue {
        if self.rent.is_exempt(lamports, data_len) {
            RentDue::Exempt
        } else {
            let slots_elapsed: u64 = (account_rent_epoch..=self.epoch)
                .map(|epoch| {
                    self.epoch_schedule
                        .get_slots_in_epoch(epoch.saturating_add(1))
                })
                .sum();

            // avoid infinite rent in rust 1.45
            let years_elapsed = if self.slots_per_year != 0.0 {
                slots_elapsed as f64 / self.slots_per_year
            } else {
                0.0
            };

            // we know this account is not exempt
            let due = self.rent.due_amount(data_len, years_elapsed);
            RentDue::Paying(due)
        }
    }

    // Updates the account's lamports and status, and returns the amount of rent collected, if any.
    // This is NOT thread safe at some level. If we try to collect from the same account in
    // parallel, we may collect twice.
    #[must_use = "add to Bank::collected_rent"]
    pub fn collect_from_existing_account(
        &self,
        address: &Pubkey,
        account: &mut AccountSharedData,
    ) -> CollectedInfo {
        match self.calculate_rent_result(address, account) {
            RentResult::Exempt => {
                account.set_rent_epoch(RENT_EXEMPT_RENT_EPOCH);
                CollectedInfo::default()
            }
            RentResult::NoRentCollectionNow => CollectedInfo::default(),
            RentResult::CollectRent {
                new_rent_epoch,
                rent_due,
            } => match account.lamports().checked_sub(rent_due) {
                None | Some(0) => {
                    let account = std::mem::take(account);
                    CollectedInfo {
                        rent_amount: account.lamports(),
                        account_data_len_reclaimed: account.data().len() as u64,
                    }
                }
                Some(lamports) => {
                    account.set_lamports(lamports);
                    account.set_rent_epoch(new_rent_epoch);
                    CollectedInfo {
                        rent_amount: rent_due,
                        account_data_len_reclaimed: 0u64,
                    }
                }
            },
        }
    }

    /// determine what should happen to collect rent from this account
    #[must_use]
    fn calculate_rent_result(
        &self,
        address: &Pubkey,
        account: &impl ReadableAccount,
    ) -> RentResult {
        if account.rent_epoch() == RENT_EXEMPT_RENT_EPOCH || account.rent_epoch() > self.epoch {
            // potentially rent paying account (or known and already marked exempt)
            // Maybe collect rent later, leave account alone for now.
            return RentResult::NoRentCollectionNow;
        }
        if !self.should_collect_rent(address, account.executable()) {
            // easy to determine this account should not consider having rent collected from it
            return RentResult::Exempt;
        }
        match self.get_rent_due(
            account.lamports(),
            account.data().len(),
            account.rent_epoch(),
        ) {
            // account will not have rent collected ever
            RentDue::Exempt => RentResult::Exempt,
            // potentially rent paying account
            // Maybe collect rent later, leave account alone for now.
            RentDue::Paying(0) => RentResult::NoRentCollectionNow,
            // Rent is collected for next epoch.
            RentDue::Paying(rent_due) => RentResult::CollectRent {
                new_rent_epoch: self.epoch.saturating_add(1),
                rent_due,
            },
        }
    }
}

impl SVMRentCollector for RentCollector {
    fn collect_rent(&self, address: &Pubkey, account: &mut AccountSharedData) -> CollectedInfo {
        self.collect_from_existing_account(address, account)
    }

    fn get_rent(&self) -> &Rent {
        &self.rent
    }

    fn get_rent_due(&self, lamports: u64, data_len: usize, account_rent_epoch: Epoch) -> RentDue {
        self.get_rent_due(lamports, data_len, account_rent_epoch)
    }
}

/// Rent collector trait. Represents an entity that can evaluate the rent state
/// of an account, determine rent due, and collect rent.
///
/// Implementors are responsible for evaluating rent due and collecting rent
/// from accounts, if required. Methods for evaluating account rent state have
/// default implementations, which can be overridden for customized rent
/// management.
pub trait SVMRentCollector {
    /// Check rent state transition for an account in a transaction.
    ///
    /// This method has a default implementation that calls into
    /// `check_rent_state_with_account`.
    fn check_rent_state(
        &self,
        pre_rent_state: Option<&RentState>,
        post_rent_state: Option<&RentState>,
        transaction_context: &TransactionContext,
        index: IndexOfAccount,
    ) -> TransactionResult<()> {
        if let Some((pre_rent_state, post_rent_state)) = pre_rent_state.zip(post_rent_state) {
            let expect_msg =
                "account must exist at TransactionContext index if rent-states are Some";
            self.check_rent_state_with_account(
                pre_rent_state,
                post_rent_state,
                transaction_context
                    .get_key_of_account_at_index(index)
                    .expect(expect_msg),
                &transaction_context
                    .accounts()
                    .try_borrow(index)
                    .expect(expect_msg),
                index,
            )?;
        }
        Ok(())
    }

    /// Check rent state transition for an account directly.
    ///
    /// This method has a default implementation that checks whether the
    /// transition is allowed and returns an error if it is not. It also
    /// verifies that the account is not the incinerator.
    fn check_rent_state_with_account(
        &self,
        pre_rent_state: &RentState,
        post_rent_state: &RentState,
        address: &Pubkey,
        _account_state: &AccountSharedData,
        account_index: IndexOfAccount,
    ) -> TransactionResult<()> {
        if !solana_sdk_ids::incinerator::check_id(address)
            && !self.transition_allowed(pre_rent_state, post_rent_state)
        {
            let account_index = account_index as u8;
            Err(TransactionError::InsufficientFundsForRent { account_index })
        } else {
            Ok(())
        }
    }

    /// Collect rent from an account.
    fn collect_rent(&self, address: &Pubkey, account: &mut AccountSharedData) -> CollectedInfo;

    /// Determine the rent state of an account.
    ///
    /// This method has a default implementation that treats accounts with zero
    /// lamports as uninitialized and uses the implemented `get_rent` to
    /// determine whether an account is rent-exempt.
    fn get_account_rent_state(&self, account: &AccountSharedData) -> RentState {
        if account.lamports() == 0 {
            RentState::Uninitialized
        } else if self
            .get_rent()
            .is_exempt(account.lamports(), account.data().len())
        {
            RentState::RentExempt
        } else {
            RentState::RentPaying {
                data_size: account.data().len(),
                lamports: account.lamports(),
            }
        }
    }

    /// Get the rent collector's rent instance.
    fn get_rent(&self) -> &Rent;

    /// Get the rent due for an account.
    fn get_rent_due(&self, lamports: u64, data_len: usize, account_rent_epoch: Epoch) -> RentDue;

    /// Check whether a transition from the pre_rent_state to the
    /// post_rent_state is valid.
    ///
    /// This method has a default implementation that allows transitions from
    /// any state to `RentState::Uninitialized` or `RentState::RentExempt`.
    /// Pre-state `RentState::RentPaying` can only transition to
    /// `RentState::RentPaying` if the data size remains the same and the
    /// account is not credited.
    fn transition_allowed(&self, pre_rent_state: &RentState, post_rent_state: &RentState) -> bool {
        match post_rent_state {
            RentState::Uninitialized | RentState::RentExempt => true,
            RentState::RentPaying {
                data_size: post_data_size,
                lamports: post_lamports,
            } => {
                match pre_rent_state {
                    RentState::Uninitialized | RentState::RentExempt => false,
                    RentState::RentPaying {
                        data_size: pre_data_size,
                        lamports: pre_lamports,
                    } => {
                        // Cannot remain RentPaying if resized or credited.
                        post_data_size == pre_data_size && post_lamports <= pre_lamports
                    }
                }
            }
        }
    }
}
