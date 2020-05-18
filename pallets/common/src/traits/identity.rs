// This file is part of the Polymesh distribution (https://github.com/PolymathNetwork/Polymesh).
// Copyright (c) 2020 Polymath

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

// This program is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <http://www.gnu.org/licenses/>.

use crate::{
    traits::{
        balances, group::GroupTrait, multisig::AddSignerMultiSig, CommonTrait, NegativeImbalance,
    },
    ChargeProtocolFee, SystematicIssuers,
};
use polymesh_primitives::{
    AccountKey, AuthorizationData, IdentityClaim, IdentityId, LinkData, Permission, Signatory,
    SigningItem, Ticker,
};

use codec::{Decode, Encode};
use frame_support::{decl_event, weights::GetDispatchInfo, Parameter};
use pallet_transaction_payment::{CddAndFeeDetails, ChargeTxFee};
use sp_core::H512;
use sp_runtime::traits::{Dispatchable, IdentifyAccount, Member, Verify};
#[cfg(feature = "std")]
use sp_runtime::{Deserialize, Serialize};
use sp_std::vec::Vec;

/// Keys could be linked to several identities (`IdentityId`) as master key or signing key.
/// Master key or external type signing key are restricted to be linked to just one identity.
/// Other types of signing key could be associated with more than one identity.
/// # TODO
/// * Use of `Master` and `Signer` (instead of `Unique`) will optimize the access.
#[derive(codec::Encode, codec::Decode, Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum LinkedKeyInfo {
    Unique(IdentityId),
    Group(Vec<IdentityId>),
}

pub type AuthorizationNonce = u64;

/// It represents an authorization that any account could sign to allow operations related with a
/// target identity.
///
/// # Safety
///
/// Please note, that `nonce` has been added to avoid **replay attack** and it should be the current
/// value of nonce of master key of `target_id`. See `System::account_nonce`.
/// In this way, the authorization is delimited to an specific transaction (usually the next one)
/// of master key of target identity.
#[derive(codec::Encode, codec::Decode, Clone, PartialEq, Eq, Debug)]
pub struct TargetIdAuthorization<Moment> {
    /// Target identity which is authorized to make an operation.
    pub target_id: IdentityId,
    /// It HAS TO be `target_id` authorization nonce: See `Identity::offchain_authorization_nonce`
    pub nonce: AuthorizationNonce,
    pub expires_at: Moment,
}

/// It is a signing item with authorization of that signing key (off-chain operation) to be added
/// to an identity.
/// `auth_signature` is the signature, generated by signing item, of `TargetIdAuthorization`.
///
/// # TODO
///  - Replace `H512` type by a template type which represents explicitly the relation with
///  `TargetIdAuthorization`.
#[derive(codec::Encode, codec::Decode, Clone, PartialEq, Eq, Debug)]
pub struct SigningItemWithAuth {
    /// Signing item to be added.
    pub signing_item: SigningItem,
    /// Off-chain authorization signature.
    pub auth_signature: H512,
}

/// The module's configuration trait.
pub trait Trait: CommonTrait + pallet_timestamp::Trait + balances::Trait {
    /// The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
    /// An extrinsic call.
    type Proposal: Parameter
        + Dispatchable<Origin = <Self as frame_system::Trait>::Origin>
        + GetDispatchInfo;
    /// MultiSig module
    type AddSignerMultiSigTarget: AddSignerMultiSig;
    /// Group module
    type CddServiceProviders: GroupTrait<<Self as pallet_timestamp::Trait>::Moment>;

    type Balances: balances::BalancesTrait<
        <Self as frame_system::Trait>::AccountId,
        <Self as CommonTrait>::Balance,
        NegativeImbalance<Self>,
    >;
    /// Charges fee for forwarded call
    type ChargeTxFeeTarget: ChargeTxFee;
    /// Used to check and update CDD
    type CddHandler: CddAndFeeDetails<<Self as frame_system::Trait>::Call>;

    type Public: IdentifyAccount<AccountId = <Self as frame_system::Trait>::AccountId>;
    type OffChainSignature: Verify<Signer = Self::Public> + Member + Decode + Encode;
    type ProtocolFee: ChargeProtocolFee<<Self as frame_system::Trait>::AccountId>;
}

// rustfmt adds a comma after Option<Moment> in NewAuthorization and it breaks compilation
#[rustfmt::skip]
decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as frame_system::Trait>::AccountId,
        Moment = <T as pallet_timestamp::Trait>::Moment,
    {
        /// DID, master key account ID, signing keys
        DidCreated(IdentityId, AccountId, Vec<SigningItem>),

        /// DID, new keys
        SigningItemsAdded(IdentityId, Vec<SigningItem>),

        /// DID, the keys that got removed
        SigningItemsRemoved(IdentityId, Vec<Signatory>),

        /// DID, updated signing key, previous permissions
        SigningPermissionsUpdated(IdentityId, SigningItem, Vec<Permission>),


        /// DID, old master key account ID, new key
        MasterKeyUpdated(IdentityId, AccountKey, AccountKey),

        /// DID, claims
        ClaimAdded(IdentityId, IdentityClaim),

        /// DID, ClaimType, Claim Issuer
        ClaimRevoked(IdentityId, IdentityClaim),

        /// DID queried
        DidStatus(IdentityId, AccountKey),

        /// CDD queried
        CddStatus(Option<IdentityId>, AccountKey, bool),

        /// Asset DID
        AssetDidRegistered(IdentityId, Ticker),

        /// New authorization added.
        /// (from, to, auth_id, authorization_data, expiry)
        AuthorizationAddedByIdentity(
            IdentityId,
            Option<IdentityId>,
            Option<AccountKey>,
            u64,
            AuthorizationData,
            Option<Moment>
        ),

        AuthorizationAddedByKey(
            AccountKey,
            Option<IdentityId>,
            Option<AccountKey>,
            u64,
            AuthorizationData,
            Option<Moment>
        ),

        /// Authorization revoked by the authorizer.
        /// (authorized_identity, authorized_key, auth_id)
        AuthorizationRevoked(Option<IdentityId>, Option<AccountKey>, u64),

        /// Authorization rejected by the user who was authorized.
        /// (authorized_identity, authorized_key, auth_id)
        AuthorizationRejected(Option<IdentityId>, Option<AccountKey>, u64),

        /// Authorization consumed.
        /// (authorized_identity, authorized_key, auth_id)
        AuthorizationConsumed(Option<IdentityId>, Option<AccountKey>, u64),

        /// Off-chain Authorization has been revoked.
        /// (Target Identity, Signatory)
        OffChainAuthorizationRevoked(IdentityId, Signatory),

        /// CDD requirement for updating master key changed. (new_requirement)
        CddRequirementForMasterKeyUpdated(bool),

        /// New link added
        /// (associated identity or key, link_id, link_data, expiry)
        LinkAdded(
            Option<IdentityId>,
            Option<AccountKey>,
            u64,
            LinkData,
            Option<Moment>
        ),

        /// Link removed.
        /// (associated identity or key, link_id)
        LinkRemoved(Option<IdentityId>, Option<AccountKey>, u64),

        /// Link contents updated.
        /// (associated identity or key, link_id)
        LinkUpdated(Option<IdentityId>, Option<AccountKey>, u64),


        /// CDD claims generated by `IdentityId` (a CDD Provider) have been invalidated from
        /// `Moment`.
        CddClaimsInvalidated(IdentityId, Moment),

        /// All Signing keys of the identity ID are frozen.
        SigningKeysFrozen(IdentityId),

        /// All Signing keys of the identity ID are unfrozen.
        SigningKeysUnfrozen(IdentityId),
    }
);

pub trait IdentityTrait {
    fn get_identity(key: &AccountKey) -> Option<IdentityId>;
    fn current_identity() -> Option<IdentityId>;
    fn set_current_identity(id: Option<IdentityId>);
    fn current_payer() -> Option<Signatory>;
    fn set_current_payer(payer: Option<Signatory>);

    fn is_signer_authorized(did: IdentityId, signer: &Signatory) -> bool;
    fn is_signer_authorized_with_permissions(
        did: IdentityId,
        signer: &Signatory,
        permissions: Vec<Permission>,
    ) -> bool;
    fn is_master_key(did: IdentityId, key: &AccountKey) -> bool;

    /// It adds a systematic CDD claim for each `target` identity.
    ///
    /// It is used when we add a new member to CDD providers or Governance Committee.
    fn unsafe_add_systematic_cdd_claims(targets: &[IdentityId], issuer: SystematicIssuers);

    /// It removes the systematic CDD claim for each `target` identity.
    ///
    /// It is used when we remove a member from CDD providers or Governance Committee.
    fn unsafe_revoke_systematic_cdd_claims(targets: &[IdentityId], issuer: SystematicIssuers);

    // Provides the DID status for the given DID
    fn has_valid_cdd(target_did: IdentityId) -> bool;
}
