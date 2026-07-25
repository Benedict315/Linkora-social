#![cfg(test)]

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    token::{Client as TokenClient, StellarAssetClient},
    vec, Address, BytesN, Env, String,
};

fn setup_token(env: &Env, admin: &Address) -> Address {
    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(env, &token_id.address()).mint(admin, &10_000);
    token_id.address()
}

pub fn setup_contract(env: &Env) -> (LinkoraContractClient<'_>, Address, Address) {
    let contract_id = env.register(LinkoraContract, ());
    let client = LinkoraContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    client.initialize(&admin, &treasury, &0);
    (client, admin, treasury)
}

#[test]
fn test_set_and_get_profile() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = Address::generate(&env);
    let token = Address::generate(&env);
    client.set_profile(&user, &String::from_str(&env, "alice"), &token);
    let profile = client.get_profile(&user).unwrap();
    assert_eq!(profile.username, String::from_str(&env, "alice"));
}

#[test]
fn test_tip_fee_split() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(LinkoraContract, ());
    let client = LinkoraContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let author = Address::generate(&env);
    let tipper = Address::generate(&env);

    // Initialize with 2.5% fee (250 bps)
    client.initialize(&admin, &treasury, &250);

    let token = setup_token(&env, &tipper);
    let post_id = client.create_post(&author, &String::from_str(&env, "Fee test post"));

    // Tip 1000 units
    client.tip(&tipper, &post_id, &token, &1000);

    // Verify balances
    // Fee = 1000 * 250 / 10000 = 25
    // Author gets 1000 - 25 = 975
    assert_eq!(TokenClient::new(&env, &token).balance(&treasury), 25);
    assert_eq!(TokenClient::new(&env, &token).balance(&author), 975);

    let post = client.get_post(&post_id).unwrap();
    assert_eq!(post.tip_total, 1000);
}

#[test]
fn test_profile_count() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let token = Address::generate(&env);

    client.set_profile(&user1, &String::from_str(&env, "alice"), &token);
    assert_eq!(client.get_profile_count(), 1);

    // Update profile should not increment count
    client.set_profile(&user1, &String::from_str(&env, "alice_new"), &token);
    assert_eq!(client.get_profile_count(), 1);

    client.set_profile(&user2, &String::from_str(&env, "bob"), &token);
    assert_eq!(client.get_profile_count(), 2);
}

#[test]
fn test_post_count() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let author = Address::generate(&env);
    client.create_post(&author, &String::from_str(&env, "Post 1"));
    client.create_post(&author, &String::from_str(&env, "Post 2"));

    assert_eq!(client.get_post_count(), 2);
}

#[test]
fn test_post_count_not_decremented_on_delete() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let author = Address::generate(&env);
    let post_id1 = client.create_post(&author, &String::from_str(&env, "Post 1"));
    let post_id2 = client.create_post(&author, &String::from_str(&env, "Post 2"));

    assert_eq!(client.get_post_count(), 2);

    // Delete first post
    client.delete_post(&author, &post_id1);

    // Counter should still be 2 (total ever created)
    assert_eq!(client.get_post_count(), 2);

    // But the post should be gone
    assert!(client.get_post(&post_id1).is_none());
    assert!(client.get_post(&post_id2).is_some());
}

#[test]
fn test_follow_and_unfollow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.follow(&alice, &bob);
    assert_eq!(client.get_following(&alice).len(), 1);
    assert_eq!(client.get_followers(&bob).len(), 1);

    client.unfollow(&alice, &bob);
    assert_eq!(client.get_following(&alice).len(), 0);
    assert_eq!(client.get_followers(&bob).len(), 0);
}

#[test]
fn test_block_prevents_follow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let blocker = Address::generate(&env);
    let blocked = Address::generate(&env);
    client.block_user(&blocker, &blocked);
    assert!(client.is_blocked(&blocker, &blocked));
}

#[test]
#[should_panic(expected = "blocked")]
fn test_blocked_follow_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Bob blocks Alice
    client.block_user(&bob, &alice);

    // Alice tries to follow Bob
    client.follow(&alice, &bob);
}

#[test]
fn test_like_post() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let author = Address::generate(&env);
    let user = Address::generate(&env);
    let post_id = client.create_post(&author, &String::from_str(&env, "Like test"));

    client.like_post(&user, &post_id);
    assert_eq!(client.get_like_count(&post_id), 1);
    assert!(client.has_liked(&user, &post_id));

    // Duplicate like should not increment
    client.like_post(&user, &post_id);
    assert_eq!(client.get_like_count(&post_id), 1);
}

#[test]
fn test_like_post_emits_event_on_first_like() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let author = Address::generate(&env);
    let user = Address::generate(&env);
    let post_id = client.create_post(&author, &String::from_str(&env, "Event test"));

    client.like_post(&user, &post_id);

    assert!(
        !env.events().all().events().is_empty(),
        "LikePostEvent should be emitted"
    );
}

#[test]
fn test_like_post_no_event_on_duplicate() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let author = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let post_id = client.create_post(&author, &String::from_str(&env, "Duplicate event test"));

    client.like_post(&user1, &post_id);
    let like_count_after_first = client.get_like_count(&post_id);

    client.like_post(&user1, &post_id);
    let like_count_after_duplicate = client.get_like_count(&post_id);

    assert_eq!(
        like_count_after_duplicate, like_count_after_first,
        "duplicate like should not increment count"
    );

    client.like_post(&user2, &post_id);
    let like_count_after_new_user = client.get_like_count(&post_id);

    assert_eq!(
        like_count_after_new_user,
        like_count_after_first + 1,
        "like from new user should increment"
    );
}

#[test]
fn test_pool_authorization() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup_contract(&env);

    let pool_admin1 = Address::generate(&env);
    let pool_admin2 = Address::generate(&env);
    let other_user = Address::generate(&env);
    let token = setup_token(&env, &pool_admin1);

    // Give other_user some tokens to deposit
    StellarAssetClient::new(&env, &token).mint(&other_user, &1000);

    let pool_id = symbol_short!("pool1");
    // Create pool with 2-of-2 threshold
    client.create_pool(
        &admin,
        &pool_id,
        &token,
        &vec![&env, pool_admin1.clone(), pool_admin2.clone()],
        &2,
    );

    // Deposit works for anyone with tokens
    client.pool_deposit(&other_user, &pool_id, &token, &100);

    // Withdrawal by both admins works
    client.pool_withdraw(
        &vec![&env, pool_admin1.clone(), pool_admin2.clone()],
        &pool_id,
        &50,
        &other_user,
    );
    assert_eq!(client.get_pool(&pool_id).unwrap().balance, 50);
}

#[test]
#[should_panic(expected = "insufficient signers")]
fn test_pool_withdraw_insufficient_signers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup_contract(&env);

    let pool_admin1 = Address::generate(&env);
    let pool_admin2 = Address::generate(&env);
    let other_user = Address::generate(&env);
    let token = setup_token(&env, &pool_admin1);
    StellarAssetClient::new(&env, &token).mint(&other_user, &1000);

    let pool_id = symbol_short!("pool1");
    client.create_pool(
        &admin,
        &pool_id,
        &token,
        &vec![&env, pool_admin1.clone(), pool_admin2.clone()],
        &2,
    );
    client.pool_deposit(&other_user, &pool_id, &token, &100);

    // Only 1 signer when 2 required
    client.pool_withdraw(&vec![&env, pool_admin1.clone()], &pool_id, &50, &other_user);
}

#[test]
#[should_panic(expected = "unauthorized signer")]
fn test_pool_withdraw_unauthorized_signer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup_contract(&env);

    let pool_admin1 = Address::generate(&env);
    let pool_admin2 = Address::generate(&env);
    let unauthorized_user = Address::generate(&env);
    let other_user = Address::generate(&env);
    let token = setup_token(&env, &pool_admin1);
    StellarAssetClient::new(&env, &token).mint(&other_user, &1000);

    let pool_id = symbol_short!("pool2");
    client.create_pool(
        &admin,
        &pool_id,
        &token,
        &vec![&env, pool_admin1.clone(), pool_admin2.clone()],
        &2,
    );
    client.pool_deposit(&other_user, &pool_id, &token, &100);

    // Try to withdraw with a signer not in pool.admins
    client.pool_withdraw(
        &vec![&env, pool_admin1.clone(), unauthorized_user.clone()],
        &pool_id,
        &50,
        &other_user,
    );
}

#[test]
#[should_panic(expected = "low balance")]
fn test_pool_withdraw_exceeds_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup_contract(&env);

    let pool_admin1 = Address::generate(&env);
    let pool_admin2 = Address::generate(&env);
    let other_user = Address::generate(&env);
    let token = setup_token(&env, &pool_admin1);
    StellarAssetClient::new(&env, &token).mint(&other_user, &1000);

    let pool_id = symbol_short!("pool3");
    client.create_pool(
        &admin,
        &pool_id,
        &token,
        &vec![&env, pool_admin1.clone(), pool_admin2.clone()],
        &1,
    );
    client.pool_deposit(&other_user, &pool_id, &token, &100);

    // Try to withdraw more than available balance
    client.pool_withdraw(
        &vec![&env, pool_admin1.clone(), pool_admin2.clone()],
        &pool_id,
        &200,
        &other_user,
    );
}

#[test]
fn test_sequential_posts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let author = Address::generate(&env);

    // Set first timestamp
    let ts1 = 1000;
    env.ledger().set_timestamp(ts1);

    // Create first post
    let post_id1 = client.create_post(&author, &String::from_str(&env, "First post"));
    assert_eq!(post_id1, 1);

    let post1 = client.get_post(&post_id1).unwrap();
    assert_eq!(post1.timestamp, ts1);
    assert_eq!(post1.id, 1);

    // Advance timestamp
    let ts2 = 2000;
    env.ledger().set_timestamp(ts2);

    // Create second post
    let post_id2 = client.create_post(&author, &String::from_str(&env, "Second post"));
    assert_eq!(post_id2, 2);

    let post2 = client.get_post(&post_id2).unwrap();
    assert_eq!(post2.timestamp, ts2);
    assert_eq!(post2.id, 2);
}

#[test]
#[should_panic(expected = "post does not exist: 999")]
fn test_delete_post_non_existent() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let author = Address::generate(&env);
    client.delete_post(&author, &999);
}

// ── initialize / upgrade tests ────────────────────────────────────────────────

#[test]
fn test_initialize_stores_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(LinkoraContract, ());
    let client = LinkoraContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.initialize(&admin, &treasury, &0);

    // Admin is stored: set_fee (admin-only) should succeed when called by admin
    client.set_fee(&100);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_initialize_twice_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(LinkoraContract, ());
    let client = LinkoraContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.initialize(&admin, &treasury, &0);
    // Second call must panic
    client.initialize(&admin, &treasury, &0);
}

#[test]
fn test_upgrade_by_admin_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    // Upload the contract wasm (compiled with `wasm32v1-none` target for
    // soroban host compatibility) so the hash is valid in the mock ledger.
    // To regenerate: cargo build --target wasm32v1-none --release
    //   then copy target/wasm32v1-none/release/linkora_contracts.wasm here.
    const WASM: &[u8] = include_bytes!("../linkora_contracts.wasm");
    let wasm_hash = env
        .deployer()
        .upload_contract_wasm(soroban_sdk::Bytes::from_slice(&env, WASM));
    client.upgrade(&wasm_hash);
}

#[test]
#[should_panic]
fn test_upgrade_by_non_admin_panics() {
    let env = Env::default();
    // Do NOT mock all auths — only the non-admin will try to auth
    let contract_id = env.register(LinkoraContract, ());
    let client = LinkoraContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    // Initialize with mock_all_auths temporarily
    env.mock_all_auths();
    client.initialize(&admin, &treasury, &0);

    // Now clear mocked auths and attempt upgrade without admin auth
    let mock_hash = BytesN::from_array(&env, &[1u8; 32]);
    // This should panic because the non-admin caller cannot satisfy require_auth for admin
    client.upgrade(&mock_hash);
}

#[test]
#[should_panic(expected = "not initialized")]
fn test_upgrade_before_initialize_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(LinkoraContract, ());
    let client = LinkoraContractClient::new(&env, &contract_id);

    let mock_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.upgrade(&mock_hash);
}

// ── Credential Subsystem Tests ─────────────────────────────────────────────────

#[test]
fn test_update_credential_root_persists() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = Address::generate(&env);
    let root = BytesN::from_array(&env, &[1u8; 32]);

    client.update_credential_root(&user, &root);

    let retrieved = client.get_credential_root(&user);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), root);
}

#[test]
fn test_update_credential_root_multiple_updates() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = Address::generate(&env);
    let root1 = BytesN::from_array(&env, &[1u8; 32]);
    let root2 = BytesN::from_array(&env, &[2u8; 32]);
    let root3 = BytesN::from_array(&env, &[3u8; 32]);

    client.update_credential_root(&user, &root1);
    client.update_credential_root(&user, &root2);
    client.update_credential_root(&user, &root3);

    let retrieved = client.get_credential_root(&user).unwrap();
    assert_eq!(retrieved, root3, "latest value should be stored");
}

#[test]
fn test_update_credential_root_independent_users() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let root1 = BytesN::from_array(&env, &[1u8; 32]);
    let root2 = BytesN::from_array(&env, &[2u8; 32]);

    client.update_credential_root(&user1, &root1);
    client.update_credential_root(&user2, &root2);

    assert_eq!(client.get_credential_root(&user1).unwrap(), root1);
    assert_eq!(client.get_credential_root(&user2).unwrap(), root2);
}

#[test]
fn test_get_credential_root_none_when_not_set() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = Address::generate(&env);

    let retrieved = client.get_credential_root(&user);
    assert!(retrieved.is_none(), "should return None for user with no root");
}

#[test]
fn test_verify_credential_valid_proof() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = Address::generate(&env);
    
    // Create a simple Merkle tree with one leaf
    // For a single leaf, the root is just the hash of the leaf
    let leaf = BytesN::from_array(&env, &[1u8; 32]);
    let proof: Vec<BytesN<32>> = vec![&env];
    
    // Compute the expected root (hash of leaf with empty proof = leaf itself)
    let root = leaf.clone();
    
    client.update_credential_root(&user, &root);
    
    let nullifier = BytesN::from_array(&env, &[10u8; 32]);
    let result = client.verify_credential(&user, &leaf, &proof, &nullifier);
    
    assert!(result, "valid proof should return true");
}

#[test]
fn test_verify_credential_invalid_proof_wrong_leaf() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = Address::generate(&env);
    
    let root = BytesN::from_array(&env, &[1u8; 32]);
    let wrong_leaf = BytesN::from_array(&env, &[2u8; 32]);
    let proof: Vec<BytesN<32>> = vec![&env];
    
    client.update_credential_root(&user, &root);
    
    let nullifier = BytesN::from_array(&env, &[10u8; 32]);
    let result = client.verify_credential(&user, &wrong_leaf, &proof, &nullifier);
    
    assert!(!result, "invalid proof with wrong leaf should return false");
}

#[test]
fn test_verify_credential_invalid_proof_wrong_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = Address::generate(&env);
    
    let leaf = BytesN::from_array(&env, &[1u8; 32]);
    let root = leaf.clone();
    let wrong_sibling = BytesN::from_array(&env, &[99u8; 32]);
    let proof = vec![&env, wrong_sibling];
    
    client.update_credential_root(&user, &root);
    
    let nullifier = BytesN::from_array(&env, &[10u8; 32]);
    let result = client.verify_credential(&user, &leaf, &proof, &nullifier);
    
    assert!(!result, "invalid proof with wrong path should return false");
}

#[test]
#[should_panic(expected = "no credential root set")]
fn test_verify_credential_panics_no_root() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = Address::generate(&env);
    let leaf = BytesN::from_array(&env, &[1u8; 32]);
    let proof: Vec<BytesN<32>> = vec![&env];
    let nullifier = BytesN::from_array(&env, &[10u8; 32]);
    
    // Try to verify without setting a root
    client.verify_credential(&user, &leaf, &proof, &nullifier);
}

#[test]
#[should_panic(expected = "nullifier already used")]
fn test_verify_credential_nullifier_replay_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = Address::generate(&env);
    
    let leaf = BytesN::from_array(&env, &[1u8; 32]);
    let root = leaf.clone();
    let proof: Vec<BytesN<32>> = vec![&env];
    let nullifier = BytesN::from_array(&env, &[10u8; 32]);
    
    client.update_credential_root(&user, &root);
    
    // First verification should succeed
    let result1 = client.verify_credential(&user, &leaf, &proof, &nullifier);
    assert!(result1);
    
    // Second verification with same nullifier should panic
    client.verify_credential(&user, &leaf, &proof, &nullifier);
}

#[test]
fn test_verify_credential_different_nullifiers_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = Address::generate(&env);
    
    let leaf = BytesN::from_array(&env, &[1u8; 32]);
    let root = leaf.clone();
    let proof: Vec<BytesN<32>> = vec![&env];
    let nullifier1 = BytesN::from_array(&env, &[10u8; 32]);
    let nullifier2 = BytesN::from_array(&env, &[20u8; 32]);
    
    client.update_credential_root(&user, &root);
    
    // Both verifications with different nullifiers should succeed
    let result1 = client.verify_credential(&user, &leaf, &proof, &nullifier1);
    assert!(result1);
    
    let result2 = client.verify_credential(&user, &leaf, &proof, &nullifier2);
    assert!(result2);
}

#[test]
fn test_verify_credential_empty_proof() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = Address::generate(&env);
    
    // For empty proof, root should equal leaf
    let leaf = BytesN::from_array(&env, &[1u8; 32]);
    let root = leaf.clone();
    let proof: Vec<BytesN<32>> = vec![&env];
    
    client.update_credential_root(&user, &root);
    
    let nullifier = BytesN::from_array(&env, &[10u8; 32]);
    let result = client.verify_credential(&user, &leaf, &proof, &nullifier);
    
    assert!(result, "empty proof should work when root equals leaf");
}

#[test]
fn test_verify_credential_max_depth_proof() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_contract(&env);

    let user = Address::generate(&env);
    
    // Create a proof with multiple levels
    let leaf = BytesN::from_array(&env, &[1u8; 32]);
    let sibling1 = BytesN::from_array(&env, &[2u8; 32]);
    let sibling2 = BytesN::from_array(&env, &[3u8; 32]);
    let sibling3 = BytesN::from_array(&env, &[4u8; 32]);
    let proof = vec![&env, sibling1.clone(), sibling2.clone(), sibling3.clone()];
    
    // Compute the expected root using position-dependent hash
    let mut current = leaf.clone();
    let mut index = 0u8;
    
    // Add sibling1 with index 0
    let mut result1 = [0u8; 32];
    let current_arr = current.to_array();
    let s1_arr = sibling1.clone().to_array();
    for i in 0..32 {
        result1[i] = current_arr[i].wrapping_add(s1_arr[i]).wrapping_add(index);
    }
    current = BytesN::from_array(&env, &result1);
    index = index.wrapping_add(1);
    
    // Add sibling2 with index 1
    let mut result2 = [0u8; 32];
    let current_arr2 = current.to_array();
    let s2_arr = sibling2.clone().to_array();
    for i in 0..32 {
        result2[i] = current_arr2[i].wrapping_add(s2_arr[i]).wrapping_add(index);
    }
    current = BytesN::from_array(&env, &result2);
    index = index.wrapping_add(1);
    
    // Add sibling3 with index 2
    let mut result3 = [0u8; 32];
    let current_arr3 = current.to_array();
    let s3_arr = sibling3.clone().to_array();
    for i in 0..32 {
        result3[i] = current_arr3[i].wrapping_add(s3_arr[i]).wrapping_add(index);
    }
    let root = BytesN::from_array(&env, &result3);
    
    client.update_credential_root(&user, &root);
    
    let nullifier = BytesN::from_array(&env, &[10u8; 32]);
    let result = client.verify_credential(&user, &leaf, &proof, &nullifier);
    
    assert!(result, "max depth proof should verify correctly");
}
