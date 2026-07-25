#![cfg(test)]

use crate::test::*;
use soroban_sdk::{vec, BytesN, Env, Vec};

/// Credential subsystem invariant tests
/// These tests verify that the credential subsystem maintains
/// important security and correctness invariants.

#[test]
fn test_invariant_credential_root_persistence() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
    let root = BytesN::from_array(&env, &[1u8; 32]);

    client.update_credential_root(&user, &root);

    // Invariant: Once set, the root should persist across multiple reads
    assert_eq!(client.get_credential_root(&user).unwrap(), root);
    assert_eq!(client.get_credential_root(&user).unwrap(), root);
    assert_eq!(client.get_credential_root(&user).unwrap(), root);
}

#[test]
#[should_panic(expected = "nullifier already used")]
fn test_invariant_nullifier_uniqueness() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
    let leaf = BytesN::from_array(&env, &[1u8; 32]);
    let root = leaf.clone();
    let proof: Vec<BytesN<32>> = vec![&env];

    client.update_credential_root(&user, &root);

    // Invariant: Each nullifier can only be used once
    let nullifier1 = BytesN::from_array(&env, &[10u8; 32]);
    let nullifier2 = BytesN::from_array(&env, &[20u8; 32]);
    let nullifier3 = BytesN::from_array(&env, &[30u8; 32]);

    assert!(client.verify_credential(&user, &leaf, &proof, &nullifier1));
    assert!(client.verify_credential(&user, &leaf, &proof, &nullifier2));
    assert!(client.verify_credential(&user, &leaf, &proof, &nullifier3));

    // Reusing any nullifier should panic
    client.verify_credential(&user, &leaf, &proof, &nullifier1);
}

#[test]
#[should_panic(expected = "nullifier already used")]
fn test_invariant_nullifier_replay_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
    let leaf = BytesN::from_array(&env, &[1u8; 32]);
    let root = leaf.clone();
    let proof: Vec<BytesN<32>> = vec![&env];
    let nullifier = BytesN::from_array(&env, &[10u8; 32]);

    client.update_credential_root(&user, &root);

    // First verification should succeed
    assert!(client.verify_credential(&user, &leaf, &proof, &nullifier));

    // Second verification with same nullifier should panic
    client.verify_credential(&user, &leaf, &proof, &nullifier);
}

#[test]
fn test_invariant_user_root_isolation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user1 = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
    let user2 = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
    let user3 = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

    let root1 = BytesN::from_array(&env, &[1u8; 32]);
    let root2 = BytesN::from_array(&env, &[2u8; 32]);
    let root3 = BytesN::from_array(&env, &[3u8; 32]);

    client.update_credential_root(&user1, &root1);
    client.update_credential_root(&user2, &root2);
    client.update_credential_root(&user3, &root3);

    // Invariant: Each user's root is independent
    assert_eq!(client.get_credential_root(&user1).unwrap(), root1);
    assert_eq!(client.get_credential_root(&user2).unwrap(), root2);
    assert_eq!(client.get_credential_root(&user3).unwrap(), root3);

    // Updating one user should not affect others
    let new_root1 = BytesN::from_array(&env, &[99u8; 32]);
    client.update_credential_root(&user1, &new_root1);

    assert_eq!(client.get_credential_root(&user1).unwrap(), new_root1);
    assert_eq!(client.get_credential_root(&user2).unwrap(), root2);
    assert_eq!(client.get_credential_root(&user3).unwrap(), root3);
}

#[test]
#[should_panic(expected = "no credential root set")]
fn test_invariant_verification_requires_root() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
    let leaf = BytesN::from_array(&env, &[1u8; 32]);
    let proof: Vec<BytesN<32>> = vec![&env];
    let nullifier = BytesN::from_array(&env, &[10u8; 32]);

    // Invariant: Verification must fail without a root set
    client.verify_credential(&user, &leaf, &proof, &nullifier);
}

#[test]
fn test_invariant_invalid_proof_does_not_consume_nullifier() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
    let root = BytesN::from_array(&env, &[1u8; 32]);
    let wrong_leaf = BytesN::from_array(&env, &[2u8; 32]);
    let proof: Vec<BytesN<32>> = vec![&env];
    let nullifier = BytesN::from_array(&env, &[10u8; 32]);

    client.update_credential_root(&user, &root);

    // Invariant: Failed verification should not consume the nullifier
    assert!(!client.verify_credential(&user, &wrong_leaf, &proof, &nullifier));

    // Same nullifier should still work for a valid proof
    let valid_leaf = root.clone();
    assert!(client.verify_credential(&user, &valid_leaf, &proof, &nullifier));
}

#[test]
fn test_invariant_root_size_fixed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

    // Invariant: Root must be exactly 32 bytes
    // This is enforced by the type system (BytesN<32>)
    let root = BytesN::from_array(&env, &[1u8; 32]);
    client.update_credential_root(&user, &root);

    let retrieved = client.get_credential_root(&user).unwrap();
    assert_eq!(retrieved.to_array().len(), 32);
}
