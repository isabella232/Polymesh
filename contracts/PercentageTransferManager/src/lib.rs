#![cfg_attr(not(feature = "std"), no_std)]

use ink_lang as ink;

mod custom_types {

    use ink_core::storage::Flush;
    use scale::Encode;

    #[derive(
        scale::Decode,
        scale::Encode,
        PartialEq,
        Ord,
        Eq,
        PartialOrd,
        Copy,
        Hash,
        Clone,
        Debug,
        Default,
    )]
    #[cfg_attr(feature = "ink-generate-abi", derive(type_metadata::Metadata))]
    pub struct IdentityId([u8; 32]);

    impl Flush for IdentityId {}

    impl From<u128> for IdentityId {
        fn from(id: u128) -> Self {
            let mut encoded_id = id.encode();
            encoded_id.resize(32, 0);
            let mut did = [0; 32];
            did.copy_from_slice(&encoded_id);
            IdentityId(did)
        }
    }

    /// Custom type
    #[derive(scale::Decode, scale::Encode, Debug, PartialEq, Ord, Eq, PartialOrd)]
    #[cfg_attr(feature = "ink-generate-abi", derive(type_metadata::Metadata))]
    pub enum RestrictionResult {
        Valid,
        Invalid,
        ForceValid,
    }
}

#[ink::contract(version = "0.1.0")]
mod percentage_transfer_manager {
    use crate::custom_types::{IdentityId, RestrictionResult};
    use ink_core::storage;
    use ink_prelude::vec::Vec;

    /// Defines the storage of your contract.
    /// Add new fields to the below struct in order
    /// to add new static storage fields to your contract.
    #[ink(storage)]
    struct PercentageTransferManagerStorage {
        /// Owner of the smart extension
        pub owner: storage::Value<AccountId>,
        /// Maximum allowed percentage of the tokens hold by an investor
        /// %age is based on the total supply of the asset. Multiplier of 10^6
        pub max_allowed_percentage: storage::Value<u128>,
        /// By toggling the primary issuance variable it will bypass
        /// all the restrictions imposed by this smart extension
        pub allow_primary_issuance: storage::Value<bool>,
        /// Exemption list that contains the list of investor's identities
        /// which are not affected by this module restrictions
        pub exemption_list: storage::HashMap<IdentityId, bool>,
    }

    #[ink(event)]
    struct ChangeAllowedPercentage {
        #[ink(topic)]
        old_percentage: u128,
        #[ink(topic)]
        new_percentage: u128,
    }

    #[ink(event)]
    struct ChangePrimaryIssuance {
        #[ink(topic)]
        allow_primary_issuance: bool,
    }

    #[ink(event)]
    struct ModifyExemptionList {
        #[ink(topic)]
        identity: IdentityId,
        #[ink(topic)]
        exempted: bool,
    }

    #[ink(event)]
    struct TransferOwnership {
        #[ink(topic)]
        new_owner: AccountId,
        #[ink(topic)]
        old_owner: AccountId,
    }

    impl PercentageTransferManagerStorage {
        /// Constructor that initializes the `u128` value to the given `max_allowed_percentage`,
        /// boolean value for the `allow_primary_issuance` & `owner` of the SE.
        #[ink(constructor)]
        fn new(&mut self, max_percentage: u128, primary_issuance: bool) {
            self.owner.set(self.env().caller());
            self.max_allowed_percentage.set(max_percentage);
            self.allow_primary_issuance.set(primary_issuance);
        }

        /// This function is used to verify transfers initiated by the
        /// runtime assets
        ///
        /// # Arguments
        /// * `from` - Identity Id of the sender.
        /// * `to` - Identity Id of the receiver.
        /// * `value` - Asset amount need to transfer to the receiver.
        /// * `balance_from` - Balance of sender at the time of transaction.
        /// * `balance_to` - Balance of receiver at the time of transaction.
        /// * `total_supply` - Total supply of the asset
        #[ink(message)]
        fn verify_transfer(
            &self,
            from: Option<IdentityId>,
            to: Option<IdentityId>,
            value: Balance,
            balance_from: Balance,
            balance_to: Balance,
            total_supply: Balance,
        ) -> RestrictionResult {
            if from == None && *self.allow_primary_issuance.get()
                || self._is_exempted_or_not(&(to.unwrap_or_default()))
                || ((balance_to + value) * 10u128.pow(6)) / total_supply
                    <= *self.max_allowed_percentage.get()
            {
                return RestrictionResult::Valid;
            }
            return RestrictionResult::Invalid;
        }

        /// Change the value of allowed percentage
        ///
        /// # Arguments
        /// * `new_percentage` - New value of Max percentage of assets hold by an investor
        #[ink(message)]
        fn change_allowed_percentage(&mut self, new_percentage: u128) {
            assert!(self.env().caller() == *self.owner.get(), "Not Authorized");
            assert!(
                *self.max_allowed_percentage.get() != new_percentage,
                "Must change setting"
            );
            self.env().emit_event(ChangeAllowedPercentage {
                old_percentage: *self.max_allowed_percentage.get(),
                new_percentage: new_percentage,
            });
            self.max_allowed_percentage.set(new_percentage);
        }

        /// Sets whether or not to consider primary issuance transfers
        ///
        /// # Arguments
        /// * `primary_issuance` - whether to allow all primary issuance transfers
        #[ink(message)]
        fn change_primary_issuance(&mut self, primary_issuance: bool) {
            assert!(self.env().caller() == *self.owner.get(), "Not Authorized");
            assert!(
                *self.allow_primary_issuance.get() != primary_issuance,
                "Must change setting"
            );
            self.allow_primary_issuance.set(primary_issuance);
            self.env().emit_event(ChangePrimaryIssuance {
                allow_primary_issuance: primary_issuance,
            });
        }

        /// To exempt the given Identity from the restriction
        ///
        /// # Arguments
        /// * `identity` - Identity of the token holder whose exemption status needs to change
        /// * `is_exempted` - New exemption status of the identity
        #[ink(message)]
        fn modify_exemption_list(&mut self, identity: IdentityId, is_exempted: bool) {
            assert!(self.env().caller() == *self.owner.get(), "Not Authorized");
            assert!(
                self._is_exempted_or_not(&identity) != is_exempted,
                "Must change setting"
            );
            self._modify_exemption_list(identity, is_exempted);
        }

        /// To exempt the given Identities from the restriction
        ///
        /// # Arguments
        /// * `exemptions` - Identities & exemption status of the identities
        #[ink(message)]
        fn modify_exemption_list_batch(&mut self, exemptions: Vec<(IdentityId, bool)>) {
            for (identity, status) in exemptions.into_iter() {
                self.modify_exemption_list(identity, status);
            }
        }

        /// Transfer ownership of the smart extension
        ///
        /// # Arguments
        /// * `new_owner` - AccountId of the new owner
        #[ink(message)]
        fn transfer_ownership(&mut self, new_owner: AccountId) {
            assert!(self.env().caller() == *self.owner.get(), "Not Authorized");
            self.env().emit_event(TransferOwnership {
                old_owner: self.env().caller(),
                new_owner: new_owner,
            });
            self.owner.set(new_owner);
        }

        /// Simply returns the current value of `max_allowed_percentage`.
        #[ink(message)]
        fn get_max_allowed_percentage(&self) -> u128 {
            *self.max_allowed_percentage.get()
        }

        /// Simply returns the current value of `allow_primary_issuance`.
        #[ink(message)]
        fn is_primary_issuance_allowed(&self) -> bool {
            *self.allow_primary_issuance.get()
        }

        /// Simply returns the current value of `owner`.
        #[ink(message)]
        fn owner(&self) -> AccountId {
            *self.owner.get()
        }

        /// Function to know whether given Identity is exempted or not
        #[ink(message)]
        fn is_exempted_or_not(&self, of: IdentityId) -> bool {
            *self.exemption_list.get(&of).unwrap_or(&false)
        }

        fn _is_exempted_or_not(&self, of: &IdentityId) -> bool {
            *self.exemption_list.get(of).unwrap_or(&false)
        }

        fn _modify_exemption_list(&mut self, identity: IdentityId, is_exempted: bool) {
            self.exemption_list.insert(identity, is_exempted);
            self.env().emit_event(ModifyExemptionList {
                identity: identity,
                exempted: is_exempted,
            });
        }
    }

    /// Unit tests in Rust are normally defined within such a `#[cfg(test)]`
    /// module and test functions are marked with a `#[test]` attribute.
    /// The below code is technically just normal Rust code.
    #[cfg(test)]
    mod tests {
        /// Imports all the definitions from the outer scope so we can use them here.
        use super::*;
        use ink_core::env::test::*;
        type EnvTypes = ink_core::env::DefaultEnvTypes;

        /// We test if the default constructor does its job.
        #[test]
        fn constructor_initialization_check() {
            let default_accounts = default_accounts::<EnvTypes>().unwrap();
            let percentage_transfer_manager =
                PercentageTransferManagerStorage::new(200000u128, false);
            assert_eq!(
                percentage_transfer_manager.get_max_allowed_percentage(),
                200000u128
            );
            assert_eq!(
                percentage_transfer_manager.is_primary_issuance_allowed(),
                false
            );
            assert_eq!(percentage_transfer_manager.owner(), default_accounts.alice);
        }

        #[test]
        fn test_verify_transfer_successfully() {
            let mut percentage_transfer_manager =
                PercentageTransferManagerStorage::new(200000, false);
            let from = IdentityId::from(1);
            let to = IdentityId::from(2);
            let multiplier: u128 = 1000000;
            // test verify transfer return value

            // Should pass when transfer value is under restriction
            assert_eq!(
                percentage_transfer_manager.verify_transfer(
                    Some(from),
                    Some(to),
                    (100u128 * multiplier).into(),
                    (2000u128 * multiplier).into(),
                    0u128.into(),
                    (2000u128 * multiplier).into()
                ),
                RestrictionResult::Valid
            );

            // Should fail if the transfer value is more than the restriction
            assert_eq!(
                percentage_transfer_manager.verify_transfer(
                    Some(from),
                    Some(to),
                    (410u128 * multiplier).into(),
                    (2000u128 * multiplier).into(),
                    0u128.into(),
                    (2000u128 * multiplier).into(),
                ),
                RestrictionResult::Invalid
            );

            // Should fail when the balance of will be more the restriction
            assert_eq!(
                percentage_transfer_manager.verify_transfer(
                    Some(from),
                    Some(to),
                    (301u128 * multiplier).into(),
                    (2000u128 * multiplier).into(),
                    (100u128 * multiplier).into(),
                    (2000u128 * multiplier).into()
                ),
                RestrictionResult::Invalid
            );

            // Should fail when primary issuance is on because from is not None
            percentage_transfer_manager.change_primary_issuance(true);
            // check for the primary issuance value
            assert_eq!(
                percentage_transfer_manager.is_primary_issuance_allowed(),
                true
            );
            assert_eq!(
                percentage_transfer_manager.verify_transfer(
                    Some(from),
                    Some(to),
                    (700u128 * multiplier).into(),
                    (2000u128 * multiplier).into(),
                    0u128.into(),
                    (2000u128 * multiplier).into()
                ),
                RestrictionResult::Invalid
            );

            // Should pass when primary issuance is on & from is None
            assert_eq!(
                percentage_transfer_manager.verify_transfer(
                    None,
                    Some(to),
                    (700u128 * multiplier).into(),
                    (2000u128 * multiplier).into(),
                    0u128.into(),
                    (2000u128 * multiplier).into()
                ),
                RestrictionResult::Valid
            );

            // Should pass when the Identity in the exemption list
            percentage_transfer_manager.change_primary_issuance(false);
            assert_eq!(
                percentage_transfer_manager.is_primary_issuance_allowed(),
                false
            );
            percentage_transfer_manager.modify_exemption_list(to, true);
            assert_eq!(percentage_transfer_manager.is_exempted_or_not(to), true);

            assert_eq!(
                percentage_transfer_manager.verify_transfer(
                    None,
                    Some(to),
                    (700u128 * multiplier).into(),
                    (2000u128 * multiplier).into(),
                    0u128.into(),
                    (2000u128 * multiplier).into()
                ),
                RestrictionResult::Valid
            );
        }

        #[test]
        fn test_verify_transfer_with_decimal_percentage() {
            let percentage_transfer_manager = PercentageTransferManagerStorage::new(278940, false); // it is 27.894% of the totalSupply
            let from = IdentityId::from(1);
            let to = IdentityId::from(2);
            let multiplier: u128 = 1000000;

            // Should pass when transfer value is under restriction
            assert_eq!(
                percentage_transfer_manager.verify_transfer(
                    Some(from),
                    Some(to),
                    (55788u128 * 10000).into(), // exact 27.894% of 2000 tokens
                    (2000u128 * multiplier).into(),
                    0u128.into(),
                    (2000u128 * multiplier).into()
                ),
                RestrictionResult::Valid
            );

            // Should fail when passing more than 27.894%
            assert_eq!(
                percentage_transfer_manager.verify_transfer(
                    Some(from),
                    Some(to),
                    (558u128 * multiplier).into(),
                    (2000u128 * multiplier).into(),
                    0u128.into(),
                    (2000u128 * multiplier).into()
                ),
                RestrictionResult::Invalid
            );
        }

        #[test]
        fn should_successfully_change_allowed_percentage() {
            let mut percentage_transfer_manager =
                PercentageTransferManagerStorage::new(200000, false);
            let from = IdentityId::from(1);
            let to = IdentityId::from(2);
            let multiplier: u128 = 1000000;
            let default_accounts = default_accounts::<EnvTypes>().unwrap();

            // Should fail if the transfer value is more than the restriction
            assert_eq!(
                percentage_transfer_manager.verify_transfer(
                    Some(from),
                    Some(to),
                    (410u128 * multiplier).into(),
                    (2000u128 * multiplier).into(),
                    0u128.into(),
                    (2000u128 * multiplier).into(),
                ),
                RestrictionResult::Invalid
            );

            percentage_transfer_manager.change_allowed_percentage(300000u128);

            // Should pass with the same values because allowed percentage get increased
            assert_eq!(
                percentage_transfer_manager.verify_transfer(
                    Some(from),
                    Some(to),
                    (410u128 * multiplier).into(),
                    (2000u128 * multiplier).into(),
                    0u128.into(),
                    (2000u128 * multiplier).into(),
                ),
                RestrictionResult::Valid
            );
        }

        #[test]
        #[should_panic(expected = "Must change setting")]
        fn should_panic_when_same_value_submitted_as_param() {
            let mut percentage_transfer_manager =
                PercentageTransferManagerStorage::new(200000, false);
            let from = IdentityId::from(1);
            let to = IdentityId::from(2);
            let multiplier: u128 = 1000000;
            //Should fail to change the allowed percentage because no change in the allowed percentage value
            percentage_transfer_manager.change_allowed_percentage(200000u128);
        }

        #[test]
        #[should_panic(expected = "Not Authorized")]
        fn should_panic_when_wrong_owner_call() {
            let default_accounts = default_accounts::<EnvTypes>().unwrap();
            let mut percentage_transfer_manager =
                PercentageTransferManagerStorage::new(200000, false);
            let from = IdentityId::from(1);
            let to = IdentityId::from(2);
            let multiplier: u128 = 1000000;

            // Should fail to call change_allowed_percentage when ownership changes
            percentage_transfer_manager.transfer_ownership(default_accounts.bob);
            assert_eq!(percentage_transfer_manager.owner(), default_accounts.bob);
            percentage_transfer_manager.change_allowed_percentage(200000u128);
        }

        #[test]
        #[should_panic(expected = "Not Authorized")]
        fn should_panic_when_calling_modify_exemption_list_by_wrong_owner() {
            let default_accounts = default_accounts::<EnvTypes>().unwrap();
            let mut percentage_transfer_manager =
                PercentageTransferManagerStorage::new(200000, false);
            let from = IdentityId::from(1);
            let to = IdentityId::from(2);
            let multiplier: u128 = 1000000;

            percentage_transfer_manager.transfer_ownership(default_accounts.bob);
            assert_eq!(percentage_transfer_manager.owner(), default_accounts.bob);
            percentage_transfer_manager.modify_exemption_list(to, true);
        }

        #[test]
        #[should_panic(expected = "Must change setting")]
        fn should_panic_when_calling_modify_exemption_list_when_same_value_passed() {
            let default_accounts = default_accounts::<EnvTypes>().unwrap();
            let mut percentage_transfer_manager =
                PercentageTransferManagerStorage::new(200000, false);
            let from = IdentityId::from(1);
            let to = IdentityId::from(2);
            let multiplier: u128 = 1000000;

            percentage_transfer_manager.modify_exemption_list(to, true);
            // Should fail to call modify_exemption_list with same exemption state
            percentage_transfer_manager.modify_exemption_list(to, true);
        }

        #[test]
        #[should_panic(expected = "Not Authorized")]
        fn should_panic_when_calling_change_primary_issuance_by_wrong_owner() {
            let default_accounts = default_accounts::<EnvTypes>().unwrap();
            let mut percentage_transfer_manager =
                PercentageTransferManagerStorage::new(200000, false);
            let from = IdentityId::from(1);
            let to = IdentityId::from(2);
            let multiplier: u128 = 1000000;

            percentage_transfer_manager.transfer_ownership(default_accounts.bob);
            assert_eq!(percentage_transfer_manager.owner(), default_accounts.bob);
            percentage_transfer_manager.change_primary_issuance(true);
        }

        #[test]
        #[should_panic(expected = "Must change setting")]
        fn should_panic_when_calling_change_primary_issuance_when_same_value_passed() {
            let default_accounts = default_accounts::<EnvTypes>().unwrap();
            let mut percentage_transfer_manager =
                PercentageTransferManagerStorage::new(200000, false);
            let from = IdentityId::from(1);
            let to = IdentityId::from(2);
            let multiplier: u128 = 1000000;

            // Should fail to call change_primary_issuance with same issuance state
            percentage_transfer_manager.change_primary_issuance(false);
        }

        #[test]
        fn should_exempt_multiple_identities() {
            let mut percentage_transfer_manager =
                PercentageTransferManagerStorage::new(200000, false);
            let exempted_identities = vec![
                (IdentityId::from(1), true),
                (IdentityId::from(2), true),
                (IdentityId::from(3), true),
            ];
            percentage_transfer_manager.modify_exemption_list_batch(exempted_identities.clone());

            assert!(percentage_transfer_manager.is_exempted_or_not(IdentityId::from(1)));
            assert!(percentage_transfer_manager.is_exempted_or_not(IdentityId::from(2)));
            assert!(percentage_transfer_manager.is_exempted_or_not(IdentityId::from(3)));
        }
    }
}
