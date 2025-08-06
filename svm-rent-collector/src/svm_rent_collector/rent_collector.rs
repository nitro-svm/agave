//! Implementation of `SVMRentCollector` for `RentCollector` from the Solana
//! SDK.

use solana_epoch_schedule::EpochSchedule;
use {
    crate::svm_rent_collector::SVMRentCollector,
    solana_account::{AccountSharedData, ReadableAccount, WritableAccount},
    solana_clock::Epoch,
    solana_pubkey::Pubkey,
    solana_rent::{Rent, RentDue},
};

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
pub struct CollectedInfo {
    /// Amount of rent collected from account
    pub rent_amount: u64,
    /// Size of data reclaimed from account (happens when account's lamports go to zero)
    pub account_data_len_reclaimed: u64,
}

pub const RENT_EXEMPT_RENT_EPOCH: Epoch = Epoch::MAX;

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
    }}

#[derive(Clone, Debug, PartialEq)]
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
            // derive default value using GenesisConfig::default()
            slots_per_year: 78_890_000.0,
            rent: Rent::default(),
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

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::rent_state::RentState,
        solana_account::ReadableAccount,
        solana_clock::Epoch,
        solana_epoch_schedule::EpochSchedule,
        solana_pubkey::Pubkey,
        solana_transaction_context::{IndexOfAccount, TransactionContext},
        solana_transaction_error::TransactionError,
    };

    #[test]
    fn test_get_account_rent_state() {
        let program_id = Pubkey::new_unique();
        let uninitialized_account = AccountSharedData::new(0, 0, &Pubkey::default());

        let account_data_size = 100;

        let rent_collector = RentCollector::new(
            Epoch::default(),
            EpochSchedule::default(),
            0.0,
            Rent::free(),
        );

        let rent_exempt_account = AccountSharedData::new(1, account_data_size, &program_id); // if rent is free, all accounts with non-zero lamports and non-empty data are rent-exempt

        assert_eq!(
            rent_collector.get_account_rent_state(&uninitialized_account),
            RentState::Uninitialized
        );
        assert_eq!(
            rent_collector.get_account_rent_state(&rent_exempt_account),
            RentState::RentExempt
        );

        let rent = Rent::default();
        let rent_minimum_balance = rent.minimum_balance(account_data_size);
        let rent_paying_account = AccountSharedData::new(
            rent_minimum_balance.saturating_sub(1),
            account_data_size,
            &program_id,
        );
        let rent_exempt_account = AccountSharedData::new(
            rent.minimum_balance(account_data_size),
            account_data_size,
            &program_id,
        );
        let rent_collector =
            RentCollector::new(Epoch::default(), EpochSchedule::default(), 0.0, rent);

        assert_eq!(
            rent_collector.get_account_rent_state(&uninitialized_account),
            RentState::Uninitialized
        );
        assert_eq!(
            rent_collector.get_account_rent_state(&rent_paying_account),
            RentState::RentPaying {
                data_size: account_data_size,
                lamports: rent_paying_account.lamports(),
            }
        );
        assert_eq!(
            rent_collector.get_account_rent_state(&rent_exempt_account),
            RentState::RentExempt
        );
    }

    #[test]
    fn test_transition_allowed() {
        let rent_collector = RentCollector::default();

        let post_rent_state = RentState::Uninitialized;
        assert!(rent_collector.transition_allowed(&RentState::Uninitialized, &post_rent_state));
        assert!(rent_collector.transition_allowed(&RentState::RentExempt, &post_rent_state));
        assert!(rent_collector.transition_allowed(
            &RentState::RentPaying {
                data_size: 0,
                lamports: 1,
            },
            &post_rent_state
        ));

        let post_rent_state = RentState::RentExempt;
        assert!(rent_collector.transition_allowed(&RentState::Uninitialized, &post_rent_state));
        assert!(rent_collector.transition_allowed(&RentState::RentExempt, &post_rent_state));
        assert!(rent_collector.transition_allowed(
            &RentState::RentPaying {
                data_size: 0,
                lamports: 1,
            },
            &post_rent_state
        ));

        let post_rent_state = RentState::RentPaying {
            data_size: 2,
            lamports: 5,
        };

        // These transitions are not allowed.
        assert!(!rent_collector.transition_allowed(&RentState::Uninitialized, &post_rent_state));
        assert!(!rent_collector.transition_allowed(&RentState::RentExempt, &post_rent_state));

        // Transition is not allowed if data size changes.
        assert!(!rent_collector.transition_allowed(
            &RentState::RentPaying {
                data_size: 3,
                lamports: 5,
            },
            &post_rent_state
        ));
        assert!(!rent_collector.transition_allowed(
            &RentState::RentPaying {
                data_size: 1,
                lamports: 5,
            },
            &post_rent_state
        ));

        // Transition is always allowed if there is no account data resize or
        // change in account's lamports.
        assert!(rent_collector.transition_allowed(
            &RentState::RentPaying {
                data_size: 2,
                lamports: 5,
            },
            &post_rent_state
        ));
        // Transition is always allowed if there is no account data resize and
        // account's lamports is reduced.
        assert!(rent_collector.transition_allowed(
            &RentState::RentPaying {
                data_size: 2,
                lamports: 7,
            },
            &post_rent_state
        ));
        // Transition is not allowed if the account is credited with more
        // lamports and remains rent-paying.
        assert!(!rent_collector.transition_allowed(
            &RentState::RentPaying {
                data_size: 2,
                lamports: 3,
            },
            &post_rent_state
        ));
    }

    #[test]
    fn test_check_rent_state_with_account() {
        let rent_collector = RentCollector::default();

        let pre_rent_state = RentState::RentPaying {
            data_size: 2,
            lamports: 3,
        };

        let post_rent_state = RentState::RentPaying {
            data_size: 2,
            lamports: 5,
        };
        let account_index = 2 as IndexOfAccount;
        let key = Pubkey::new_unique();
        let result = rent_collector.check_rent_state_with_account(
            &pre_rent_state,
            &post_rent_state,
            &key,
            &AccountSharedData::default(),
            account_index,
        );
        assert_eq!(
            result.err(),
            Some(TransactionError::InsufficientFundsForRent {
                account_index: account_index as u8
            })
        );

        let result = rent_collector.check_rent_state_with_account(
            &pre_rent_state,
            &post_rent_state,
            &solana_sdk_ids::incinerator::id(),
            &AccountSharedData::default(),
            account_index,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_rent_state() {
        let rent_collector = RentCollector::default();

        let context = TransactionContext::new(
            vec![(Pubkey::new_unique(), AccountSharedData::default())],
            Rent::default(),
            20,
            20,
        );

        let pre_rent_state = RentState::RentPaying {
            data_size: 2,
            lamports: 3,
        };

        let post_rent_state = RentState::RentPaying {
            data_size: 2,
            lamports: 5,
        };

        let result = rent_collector.check_rent_state(
            Some(&pre_rent_state),
            Some(&post_rent_state),
            &context,
            0,
        );
        assert_eq!(
            result.err(),
            Some(TransactionError::InsufficientFundsForRent { account_index: 0 })
        );

        let result = rent_collector.check_rent_state(None, Some(&post_rent_state), &context, 0);
        assert!(result.is_ok());
    }
}
