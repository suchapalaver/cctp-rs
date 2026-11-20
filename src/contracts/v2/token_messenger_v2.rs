// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0
//! `TokenMessengerV2` contract bindings and wrapper
//!
//! This module contains the Alloy-generated contract bindings for the CCTP v2
//! `TokenMessenger` contract, which manages USDC burn and mint operations with
//! Fast Transfer and hooks support.

#![allow(dead_code)] // Public API methods not used internally

use alloy_network::Ethereum;
use alloy_primitives::{Address, Bytes, FixedBytes, U256};
use alloy_provider::Provider;
use alloy_rpc_types::TransactionRequest;
use alloy_sol_types::sol;
use tracing::{debug, info};

use crate::protocol::DomainId;
use crate::spans;
use crate::RestrictedDestinationCaller;
use TokenMessengerV2::TokenMessengerV2Instance;

/// The CCTP v2 Token Messenger contract wrapper
///
/// Supports v2 features including Fast Transfer (with fees) and programmable hooks.
#[allow(dead_code)]
pub struct TokenMessengerV2Contract<P: Provider<Ethereum>> {
    instance: TokenMessengerV2Instance<P>,
}

impl<P: Provider<Ethereum>> TokenMessengerV2Contract<P> {
    /// Create a new `TokenMessengerV2Contract`
    #[allow(dead_code)]
    pub fn new(address: Address, provider: P) -> Self {
        debug!(
            contract_address = %address,
            event = "token_messenger_v2_contract_initialized"
        );
        Self {
            instance: TokenMessengerV2Instance::new(address, provider),
        }
    }

    /// Create the transaction request for the `depositForBurn` function (v2 standard transfer)
    ///
    /// For standard transfers without fast transfer or hooks.
    ///
    /// # Arguments
    ///
    /// * `from_address` - Address initiating the burn
    /// * `recipient` - Recipient address on destination chain
    /// * `destination_domain` - CCTP domain ID for destination
    /// * `token_address` - USDC token contract address
    /// * `amount` - Amount to burn
    /// * `max_fee` - Maximum fee for fast transfer (0 for standard)
    /// * `min_finality_threshold` - 1000 (fast) or 2000 (standard)
    /// * `destination_caller` - Authorized caller on destination (0x0 = anyone)
    #[allow(dead_code)]
    #[allow(clippy::too_many_arguments)]
    fn deposit_for_burn_internal(
        &self,
        from_address: Address,
        recipient: Address,
        destination_domain: DomainId,
        token_address: Address,
        amount: U256,
        max_fee: U256,
        min_finality_threshold: u32,
        destination_caller: FixedBytes<32>,
    ) -> TransactionRequest {
        self.instance
            .depositForBurn(
                amount,
                destination_domain.as_u32(),
                recipient.into_word(),
                token_address,
                destination_caller,
                max_fee,
                min_finality_threshold,
            )
            .from(from_address)
            .into_transaction_request()
    }

    /// Create the transaction request for the `depositForBurn` function (v2 standard)
    ///
    /// For standard (no-hook, zero-fee) transfers. Pass
    /// `FinalityThreshold::Standard.as_u32()` (2000) for this path; the
    /// parameter is exposed rather than hardcoded so the same value can be
    /// derived from a single caller-side source of truth.
    #[allow(dead_code)]
    pub fn deposit_for_burn_transaction(
        &self,
        from_address: Address,
        recipient: Address,
        destination_domain: DomainId,
        token_address: Address,
        amount: U256,
        min_finality_threshold: u32,
    ) -> TransactionRequest {
        self.deposit_for_burn_with_fee_transaction(
            from_address,
            recipient,
            destination_domain,
            token_address,
            amount,
            U256::ZERO,
            min_finality_threshold,
        )
    }

    /// Create a `depositForBurn` transaction with an explicit Standard
    /// Transfer fee cap.
    ///
    /// Callers must capability-gate this path because Circle's Standard
    /// Transfer fee switch is deployment-specific. The bridge-level
    /// [`CctpV2Bridge::burn`](crate::CctpV2Bridge::burn) path performs that
    /// validation automatically.
    #[allow(clippy::too_many_arguments)]
    pub fn deposit_for_burn_with_fee_transaction(
        &self,
        from_address: Address,
        recipient: Address,
        destination_domain: DomainId,
        token_address: Address,
        amount: U256,
        max_fee: U256,
        min_finality_threshold: u32,
    ) -> TransactionRequest {
        self.deposit_for_burn_configured_transaction(
            from_address,
            recipient,
            destination_domain,
            token_address,
            amount,
            max_fee,
            min_finality_threshold,
            FixedBytes::ZERO,
        )
    }

    /// Create a `depositForBurn` transaction restricted to one destination
    /// caller.
    ///
    /// This is an advanced alternative to the permissionless bridge helpers.
    /// The caller is a validated, nonzero CCTP `bytes32` word, so the protocol
    /// value is not coupled to an EVM-only representation. Only that caller
    /// may submit `receiveMessage` on the destination chain; a wrong value can
    /// permanently strand funds.
    ///
    /// Circle's ordinary Forwarding Service does not support wrapper-contract
    /// flows with `destinationCaller` set. This method still takes an EVM
    /// `Address` mint recipient and does not implement Stellar's separate
    /// `CctpForwarder` recipient and hook requirements.
    #[allow(clippy::too_many_arguments)]
    pub fn deposit_for_burn_restricted_transaction(
        &self,
        from_address: Address,
        recipient: Address,
        destination_domain: DomainId,
        token_address: Address,
        amount: U256,
        max_fee: U256,
        min_finality_threshold: u32,
        destination_caller: RestrictedDestinationCaller,
    ) -> TransactionRequest {
        self.deposit_for_burn_configured_transaction(
            from_address,
            recipient,
            destination_domain,
            token_address,
            amount,
            max_fee,
            min_finality_threshold,
            destination_caller.as_bytes32(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn deposit_for_burn_configured_transaction(
        &self,
        from_address: Address,
        recipient: Address,
        destination_domain: DomainId,
        token_address: Address,
        amount: U256,
        max_fee: U256,
        min_finality_threshold: u32,
        destination_caller: FixedBytes<32>,
    ) -> TransactionRequest {
        let span = spans::deposit_for_burn(
            &from_address,
            &recipient,
            destination_domain.as_u32(),
            &token_address,
            &amount,
        );
        let _guard = span.enter();

        info!(
            from_address = %from_address,
            recipient = %recipient,
            destination_domain = %destination_domain,
            token_address = %token_address,
            amount = %amount,
            max_fee = %max_fee,
            destination_caller = %destination_caller,
            contract_address = %self.instance.address(),
            version = "v2",
            finality_threshold = min_finality_threshold,
            event = "deposit_for_burn_v2_transaction_created"
        );

        self.deposit_for_burn_internal(
            from_address,
            recipient,
            destination_domain,
            token_address,
            amount,
            max_fee,
            min_finality_threshold,
            destination_caller,
        )
    }

    /// Returns the deployment's minimum Standard Transfer fee for `amount`.
    ///
    /// Callers must capability-gate this selector because older
    /// `TokenMessengerV2` deployments do not expose it.
    pub(crate) async fn get_min_fee_amount(
        &self,
        amount: U256,
    ) -> Result<U256, alloy_contract::Error> {
        let result = self.instance.getMinFeeAmount(amount).call().await?;
        Ok(result)
    }

    /// Create transaction for depositForBurn with Fast Transfer enabled
    ///
    /// # Arguments
    ///
    /// * `from_address` - Sender address
    /// * `recipient` - Recipient address on destination chain
    /// * `destination_domain` - CCTP domain ID for destination chain
    /// * `token_address` - USDC token contract address
    /// * `amount` - Amount to transfer
    /// * `max_fee` - Maximum fee willing to pay for fast transfer
    /// * `min_finality_threshold` - Pass `FinalityThreshold::Fast.as_u32()`
    ///   (1000) for this path. Exposed alongside `max_fee` so both wire values
    ///   can be derived from a single caller-side source of truth.
    ///
    /// # Fast Transfer
    ///
    /// When `max_fee` >= minimum fast transfer fee for the chain, the transfer
    /// will be attested at the "confirmed" finality level (~30 seconds) instead
    /// of "finalized" level (~15 minutes).
    #[allow(dead_code)]
    #[allow(clippy::too_many_arguments)]
    pub fn deposit_for_burn_fast_transaction(
        &self,
        from_address: Address,
        recipient: Address,
        destination_domain: DomainId,
        token_address: Address,
        amount: U256,
        max_fee: U256,
        min_finality_threshold: u32,
    ) -> TransactionRequest {
        info!(
            from_address = %from_address,
            recipient = %recipient,
            destination_domain = %destination_domain,
            token_address = %token_address,
            amount = %amount,
            max_fee = %max_fee,
            contract_address = %self.instance.address(),
            version = "v2",
            transfer_type = "fast",
            finality_threshold = min_finality_threshold,
            event = "deposit_for_burn_fast_transaction_created"
        );

        self.deposit_for_burn_internal(
            from_address,
            recipient,
            destination_domain,
            token_address,
            amount,
            max_fee,
            min_finality_threshold,
            FixedBytes::ZERO, // destination_caller: 0x0 = anyone
        )
    }

    /// Create transaction for `depositForBurnWithHook`
    ///
    /// # Arguments
    ///
    /// * `from_address` - Sender address
    /// * `recipient` - Recipient address on destination chain
    /// * `destination_domain` - CCTP domain ID for destination chain
    /// * `token_address` - USDC token contract address
    /// * `amount` - Amount to transfer
    /// * `max_fee` - Maximum fee willing to pay. Use `U256::ZERO` for standard
    ///   finality; supply a non-zero cap when pairing hooks with fast finality.
    /// * `min_finality_threshold` - 1000 (fast) or 2000 (standard). Hooks are
    ///   supported at both thresholds at the protocol level — pass whichever
    ///   matches the intended transfer mode.
    /// * `hook_data` - Arbitrary bytes to pass to destination chain for
    ///   programmable actions
    ///
    /// # Hooks
    ///
    /// Hook data is opaque to CCTP but can be used by integrators to trigger
    /// actions on the destination chain (e.g., swap, lend, stake).
    #[allow(clippy::too_many_arguments)]
    pub fn deposit_for_burn_with_hooks_transaction(
        &self,
        from_address: Address,
        recipient: Address,
        destination_domain: DomainId,
        token_address: Address,
        amount: U256,
        max_fee: U256,
        min_finality_threshold: u32,
        hook_data: Bytes,
    ) -> TransactionRequest {
        self.deposit_for_burn_with_hooks_configured_transaction(
            from_address,
            recipient,
            destination_domain,
            token_address,
            amount,
            max_fee,
            min_finality_threshold,
            FixedBytes::ZERO,
            hook_data,
        )
    }

    /// Create a `depositForBurnWithHook` transaction restricted to one
    /// destination caller.
    ///
    /// This preserves the full CCTP `bytes32` caller word. Use it only when the
    /// destination contract flow is designed to invoke `receiveMessage`; the
    /// normal bridge APIs deliberately remain permissionless. This method does
    /// not implement Stellar's separate `CctpForwarder` recipient and hook
    /// requirements.
    #[allow(clippy::too_many_arguments)]
    pub fn deposit_for_burn_with_hooks_restricted_transaction(
        &self,
        from_address: Address,
        recipient: Address,
        destination_domain: DomainId,
        token_address: Address,
        amount: U256,
        max_fee: U256,
        min_finality_threshold: u32,
        destination_caller: RestrictedDestinationCaller,
        hook_data: Bytes,
    ) -> TransactionRequest {
        self.deposit_for_burn_with_hooks_configured_transaction(
            from_address,
            recipient,
            destination_domain,
            token_address,
            amount,
            max_fee,
            min_finality_threshold,
            destination_caller.as_bytes32(),
            hook_data,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn deposit_for_burn_with_hooks_configured_transaction(
        &self,
        from_address: Address,
        recipient: Address,
        destination_domain: DomainId,
        token_address: Address,
        amount: U256,
        max_fee: U256,
        min_finality_threshold: u32,
        destination_caller: FixedBytes<32>,
        hook_data: Bytes,
    ) -> TransactionRequest {
        info!(
            from_address = %from_address,
            recipient = %recipient,
            destination_domain = %destination_domain,
            token_address = %token_address,
            amount = %amount,
            max_fee = %max_fee,
            destination_caller = %destination_caller,
            hook_data_len = hook_data.len(),
            contract_address = %self.instance.address(),
            version = "v2",
            has_hooks = true,
            finality_threshold = min_finality_threshold,
            event = "deposit_for_burn_hooks_transaction_created"
        );

        self.instance
            .depositForBurnWithHook(
                amount,
                destination_domain.as_u32(),
                recipient.into_word(),
                token_address,
                destination_caller,
                max_fee,
                min_finality_threshold,
                hook_data,
            )
            .from(from_address)
            .into_transaction_request()
    }

    /// Returns the contract address
    pub fn address(&self) -> Address {
        *self.instance.address()
    }
}

sol!(
    #[allow(clippy::too_many_arguments)]
    #[allow(missing_docs)]
    #[sol(rpc)]
    TokenMessengerV2,
    "abis/v2/token_messenger_v2.json"
);

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_provider::ProviderBuilder;
    use alloy_sol_types::SolCall;

    fn contract() -> TokenMessengerV2Contract<impl Provider<Ethereum>> {
        let provider = ProviderBuilder::new().connect_http("http://127.0.0.1:1/".parse().unwrap());
        TokenMessengerV2Contract::new(Address::repeat_byte(0x11), provider)
    }

    #[test]
    fn restricted_burn_encodes_full_destination_caller_word() {
        let from = Address::repeat_byte(0x22);
        let recipient = Address::repeat_byte(0x33);
        let token = Address::repeat_byte(0x44);
        let caller_address = Address::repeat_byte(0x55);
        let caller_word = caller_address.into_word();
        let caller = RestrictedDestinationCaller::from_evm_address(caller_address).unwrap();

        let transaction = contract().deposit_for_burn_restricted_transaction(
            from,
            recipient,
            DomainId::Base,
            token,
            U256::from(1_000_000_u64),
            U256::from(100_u64),
            1_000,
            caller,
        );

        assert_eq!(transaction.from, Some(from));
        let calldata = transaction.input.input().expect("burn calldata");
        let decoded = TokenMessengerV2::depositForBurnCall::abi_decode(calldata)
            .expect("depositForBurn calldata");
        assert_eq!(decoded.destinationCaller, caller_word);
        assert_eq!(decoded.mintRecipient, recipient.into_word());
        assert_eq!(decoded.destinationDomain, DomainId::Base.as_u32());
        assert_eq!(decoded.maxFee, U256::from(100_u64));
        assert_eq!(decoded.minFinalityThreshold, 1_000);
    }

    #[test]
    fn restricted_hook_burn_encodes_caller_and_hook_data() {
        let caller_address = Address::repeat_byte(0x66);
        let caller_word = caller_address.into_word();
        let caller = RestrictedDestinationCaller::from_evm_address(caller_address).unwrap();
        let hook_data = Bytes::from_static(&[0xde, 0xad, 0xbe, 0xef]);

        let transaction = contract().deposit_for_burn_with_hooks_restricted_transaction(
            Address::repeat_byte(0x22),
            Address::repeat_byte(0x33),
            DomainId::Base,
            Address::repeat_byte(0x44),
            U256::from(1_000_000_u64),
            U256::ZERO,
            2_000,
            caller,
            hook_data.clone(),
        );

        let calldata = transaction.input.input().expect("hook burn calldata");
        let decoded = TokenMessengerV2::depositForBurnWithHookCall::abi_decode(calldata)
            .expect("depositForBurnWithHook calldata");
        assert_eq!(decoded.destinationCaller, caller_word);
        assert_eq!(decoded.hookData, hook_data);
        assert_eq!(decoded.minFinalityThreshold, 2_000);
    }
}
