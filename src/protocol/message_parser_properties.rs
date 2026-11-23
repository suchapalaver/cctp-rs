// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! Property checks for canonical CCTP v2 message encoding and parsing.

use alloy_primitives::{Bytes, FixedBytes, U256};
use proptest::prelude::*;
use proptest::test_runner::{Config as ProptestConfig, FileFailurePersistence, RngSeed};
use strum::IntoEnumIterator;

use crate::{BurnMessageV2, DomainId, MessageHeader, ParseMessageError, ParsedV2Message};

fn property_config() -> ProptestConfig {
    ProptestConfig {
        cases: 512,
        rng_seed: RngSeed::Fixed(0xCC7F_0002_2026_0142),
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    }
}

fn domain_strategy() -> impl Strategy<Value = DomainId> {
    prop::sample::select(DomainId::iter().collect::<Vec<_>>())
}

fn raw_word_strategy() -> BoxedStrategy<FixedBytes<32>> {
    any::<[u8; 32]>().prop_map(FixedBytes::from).boxed()
}

fn canonical_evm_word_strategy() -> BoxedStrategy<FixedBytes<32>> {
    any::<[u8; 20]>()
        .prop_map(|suffix| {
            let mut word = [0_u8; 32];
            word[12..].copy_from_slice(&suffix);
            FixedBytes::from(word)
        })
        .boxed()
}

fn domain_word_strategy(domain: DomainId) -> BoxedStrategy<FixedBytes<32>> {
    if domain.is_evm() {
        canonical_evm_word_strategy()
    } else {
        raw_word_strategy()
    }
}

fn u256_strategy() -> impl Strategy<Value = U256> {
    prop_oneof![
        Just(U256::ZERO),
        Just(U256::MAX),
        any::<[u64; 4]>().prop_map(U256::from_limbs),
    ]
}

fn hook_data_strategy() -> impl Strategy<Value = Bytes> {
    prop_oneof![
        Just(Bytes::new()),
        Just(Bytes::from(vec![0xa5])),
        Just(Bytes::from(vec![0x5a; 255])),
        Just(Bytes::from(vec![0x5a; 256])),
        prop::collection::vec(any::<u8>(), 0..=512).prop_map(Bytes::from),
    ]
}

fn header_for_domains_strategy(
    source_domain: DomainId,
    destination_domain: DomainId,
) -> BoxedStrategy<MessageHeader> {
    (
        (
            raw_word_strategy(),
            raw_word_strategy(),
            raw_word_strategy(),
            raw_word_strategy(),
        ),
        (any::<u32>(), any::<u32>()),
    )
        .prop_map(
            move |((nonce, sender, recipient, destination_caller), (minimum, executed))| {
                MessageHeader::new(
                    source_domain,
                    destination_domain,
                    nonce,
                    sender,
                    recipient,
                    destination_caller,
                    minimum,
                    executed,
                )
            },
        )
        .boxed()
}

fn header_strategy() -> impl Strategy<Value = MessageHeader> {
    (domain_strategy(), domain_strategy())
        .prop_flat_map(|(source, destination)| header_for_domains_strategy(source, destination))
}

fn body_for_domains_strategy(
    source_domain: DomainId,
    destination_domain: DomainId,
) -> BoxedStrategy<BurnMessageV2> {
    (
        (
            domain_word_strategy(source_domain),
            domain_word_strategy(destination_domain),
            domain_word_strategy(source_domain),
        ),
        (
            u256_strategy(),
            u256_strategy(),
            u256_strategy(),
            u256_strategy(),
        ),
        hook_data_strategy(),
    )
        .prop_map(
            |(
                (burn_token, mint_recipient, message_sender),
                (amount, max_fee, fee_executed, expiration_block),
                hook_data,
            )| {
                BurnMessageV2::from_raw_words(
                    burn_token,
                    mint_recipient,
                    amount,
                    message_sender,
                    max_fee,
                    fee_executed,
                    expiration_block,
                    hook_data,
                )
            },
        )
        .boxed()
}

fn raw_body_strategy() -> impl Strategy<Value = BurnMessageV2> {
    (
        raw_word_strategy(),
        raw_word_strategy(),
        raw_word_strategy(),
        u256_strategy(),
        u256_strategy(),
        u256_strategy(),
        u256_strategy(),
        hook_data_strategy(),
    )
        .prop_map(
            |(
                burn_token,
                mint_recipient,
                message_sender,
                amount,
                max_fee,
                fee_executed,
                expiration_block,
                hook_data,
            )| {
                BurnMessageV2::from_raw_words(
                    burn_token,
                    mint_recipient,
                    amount,
                    message_sender,
                    max_fee,
                    fee_executed,
                    expiration_block,
                    hook_data,
                )
            },
        )
}

fn canonical_message_strategy() -> impl Strategy<Value = ParsedV2Message> {
    (domain_strategy(), domain_strategy()).prop_flat_map(|(source, destination)| {
        (
            header_for_domains_strategy(source, destination),
            body_for_domains_strategy(source, destination),
        )
            .prop_map(|(header, body)| {
                ParsedV2Message::new(header, body)
                    .expect("domain-aware strategies must construct canonical messages")
            })
    })
}

fn unknown_domain_strategy() -> impl Strategy<Value = u32> {
    any::<u32>().prop_filter("domain must be unknown", |value| {
        DomainId::from_u32(*value).is_none()
    })
}

fn unsupported_version_strategy() -> impl Strategy<Value = u32> {
    any::<u32>().prop_filter("version must be unsupported", |version| {
        *version != MessageHeader::SUPPORTED_VERSION
    })
}

fn witness_word(domain: DomainId, byte: u8) -> FixedBytes<32> {
    let mut word = [byte; 32];
    if domain.is_evm() {
        word[..12].fill(0);
    }
    FixedBytes::from(word)
}

#[test]
fn every_modeled_source_destination_pair_has_a_canonical_round_trip() {
    for source_domain in DomainId::iter() {
        for destination_domain in DomainId::iter() {
            let header = MessageHeader::new(
                source_domain,
                destination_domain,
                FixedBytes::from([0x11; 32]),
                FixedBytes::from([0x22; 32]),
                FixedBytes::from([0x33; 32]),
                FixedBytes::from([0x44; 32]),
                1_000,
                2_000,
            );
            let body = BurnMessageV2::from_raw_words(
                witness_word(source_domain, 0x55),
                witness_word(destination_domain, 0x66),
                U256::MAX,
                witness_word(source_domain, 0x77),
                U256::from(1_u8),
                U256::MAX,
                U256::from(2_u8),
                Bytes::from(vec![0x88; 256]),
            );
            let message = ParsedV2Message::new(header, body)
                .expect("domain-aware witness words must be canonical");
            let encoded = message.encode();

            assert_eq!(
                ParsedV2Message::parse(&encoded),
                Ok(message),
                "round trip failed for {source_domain:?} -> {destination_domain:?}",
            );
        }
    }
}

proptest! {
    #![proptest_config(property_config())]

    #[test]
    fn header_encode_parse_round_trip_is_byte_exact(header in header_strategy()) {
        let encoded = header.encode();
        let parsed = MessageHeader::parse(&encoded).expect("encoded header must parse");

        prop_assert_eq!(&parsed, &header);
        prop_assert_eq!(parsed.encode(), encoded);
    }

    #[test]
    fn body_encode_parse_round_trip_preserves_all_words(body in raw_body_strategy()) {
        let encoded = body.encode();
        let parsed = BurnMessageV2::parse(&encoded).expect("encoded body must parse");

        prop_assert_eq!(&parsed, &body);
        prop_assert_eq!(parsed.encode(), encoded);
    }

    #[test]
    fn canonical_full_messages_preserve_every_field_and_byte(
        message in canonical_message_strategy(),
    ) {
        let encoded = message.encode();
        let parsed = ParsedV2Message::parse(&encoded).expect("canonical message must parse");

        prop_assert_eq!(&parsed, &message);
        prop_assert_eq!(parsed.encode(), encoded);
    }

    #[test]
    fn every_header_truncation_reports_its_exact_length(
        message in canonical_message_strategy(),
        actual in 0..MessageHeader::SIZE,
    ) {
        let encoded = message.encode();

        prop_assert_eq!(
            ParsedV2Message::parse(&encoded[..actual]),
            Err(ParseMessageError::HeaderTooShort { actual }),
        );
    }

    #[test]
    fn every_body_truncation_reports_its_exact_length(
        message in canonical_message_strategy(),
        actual in 0..BurnMessageV2::MIN_SIZE,
    ) {
        let encoded = message.encode();
        let end = MessageHeader::SIZE + actual;

        prop_assert_eq!(
            ParsedV2Message::parse(&encoded[..end]),
            Err(ParseMessageError::BurnBodyTooShort { actual }),
        );
    }

    #[test]
    fn unsupported_versions_and_unknown_domains_have_exact_typed_errors(
        message in canonical_message_strategy(),
        version in unsupported_version_strategy(),
        source_domain in unknown_domain_strategy(),
        destination_domain in unknown_domain_strategy(),
    ) {
        let canonical = message.encode();

        let mut invalid = canonical.to_vec();
        invalid[0..4].copy_from_slice(&version.to_be_bytes());
        prop_assert_eq!(
            ParsedV2Message::parse(&invalid),
            Err(ParseMessageError::UnsupportedHeaderVersion { version }),
        );

        let mut invalid = canonical.to_vec();
        invalid[4..8].copy_from_slice(&source_domain.to_be_bytes());
        prop_assert_eq!(
            ParsedV2Message::parse(&invalid),
            Err(ParseMessageError::UnknownSourceDomain { domain: source_domain }),
        );

        let mut invalid = canonical.to_vec();
        invalid[8..12].copy_from_slice(&destination_domain.to_be_bytes());
        prop_assert_eq!(
            ParsedV2Message::parse(&invalid),
            Err(ParseMessageError::UnknownDestinationDomain { domain: destination_domain }),
        );

        let mut invalid = canonical.to_vec();
        invalid[MessageHeader::SIZE..MessageHeader::SIZE + 4]
            .copy_from_slice(&version.to_be_bytes());
        prop_assert_eq!(
            ParsedV2Message::parse(&invalid),
            Err(ParseMessageError::UnsupportedBurnBodyVersion { version }),
        );
    }

    #[test]
    fn fee_words_and_hook_mutations_are_preserved_byte_exactly(
        message in canonical_message_strategy(),
        max_fee in u256_strategy(),
        fee_executed in u256_strategy(),
        expiration_block in u256_strategy(),
        hook_data in hook_data_strategy(),
    ) {
        let mut mutated = message.encode().to_vec();
        let body = MessageHeader::SIZE;
        mutated[body + 132..body + 164].copy_from_slice(&max_fee.to_be_bytes::<32>());
        mutated[body + 164..body + 196].copy_from_slice(&fee_executed.to_be_bytes::<32>());
        mutated[body + 196..body + 228]
            .copy_from_slice(&expiration_block.to_be_bytes::<32>());
        mutated.truncate(body + BurnMessageV2::MIN_SIZE);
        mutated.extend_from_slice(&hook_data);

        let parsed = ParsedV2Message::parse(&mutated)
            .expect("fee words and hook data admit every byte pattern");
        prop_assert_eq!(parsed.body().max_fee(), max_fee);
        prop_assert_eq!(parsed.body().fee_executed(), fee_executed);
        prop_assert_eq!(parsed.body().expiration_block(), expiration_block);
        prop_assert_eq!(parsed.body().hook_data(), &hook_data);
        let reencoded = parsed.encode();
        prop_assert_eq!(reencoded.as_ref(), mutated.as_slice());
    }

    #[test]
    fn noncanonical_evm_body_words_name_the_exact_field(
        header in header_for_domains_strategy(DomainId::Ethereum, DomainId::Base),
        body in body_for_domains_strategy(DomainId::Ethereum, DomainId::Base),
        padding_offset in 0usize..12,
        nonzero in 1u8..=u8::MAX,
    ) {
        let message = ParsedV2Message::new(header, body)
            .expect("EVM strategy must construct canonical words");
        let canonical = message.encode();

        for (body_offset, expected) in [
            (4, ParseMessageError::NonCanonicalBurnToken),
            (36, ParseMessageError::NonCanonicalMintRecipient),
            (100, ParseMessageError::NonCanonicalMessageSender),
        ] {
            let mut invalid = canonical.to_vec();
            invalid[MessageHeader::SIZE + body_offset + padding_offset] = nonzero;
            prop_assert_eq!(ParsedV2Message::parse(&invalid), Err(expected));
        }
    }
}
