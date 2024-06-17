use {
    solana_program_runtime::loaded_programs::ProgramCacheMatchCriteria,
    solana_sdk::{
        account::AccountSharedData, feature_set::FeatureSet, hash::Hash, pubkey::Pubkey,
        rent_collector::RentCollector,
    },
    std::sync::Arc,
};

/// Runtime callbacks for transaction processing.
pub trait TransactionProcessingCallback {
    fn account_matches_owners(&mut self, account: &Pubkey, owners: &[Pubkey]) -> Option<usize>;

    fn get_account_shared_data(&mut self, pubkey: &Pubkey) -> Option<AccountSharedData>;

    fn get_last_blockhash_and_lamports_per_signature(&mut self) -> (Hash, u64);

    fn get_rent_collector(&mut self) -> &RentCollector;

    fn get_feature_set(&mut self) -> Arc<FeatureSet>;

    fn get_program_match_criteria(&mut self, _program: &Pubkey) -> ProgramCacheMatchCriteria {
        ProgramCacheMatchCriteria::NoCriteria
    }

    fn add_builtin_account(&mut self, _name: &str, _program_id: &Pubkey) {}
}
