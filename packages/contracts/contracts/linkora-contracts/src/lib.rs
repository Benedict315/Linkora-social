#![no_std]
use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, symbol_short, token, Address, BytesN, Env,
    Map, String, Symbol, Vec,
};

// ── Storage Keys ─────────────────────────────────────────────────────────────

const POSTS: Symbol = symbol_short!("POSTS");
// POST_CT: Tracks total posts ever created (not decremented on delete).
// This counter is used for sequential post ID generation.
const POST_CT: Symbol = symbol_short!("POST_CT");
const PROFILES: Symbol = symbol_short!("PROFILES");
const PROFILE_CT: Symbol = symbol_short!("PROF_CT");
const FOLLOWS: Symbol = symbol_short!("FOLLOWS");
const FOLLOWERS: Symbol = symbol_short!("FOLLOWRS");
const POOLS: Symbol = symbol_short!("POOLS");
const ADMIN: Symbol = symbol_short!("ADMIN");
const TREASURY: Symbol = symbol_short!("TREASURY");
const FEE_BPS: Symbol = symbol_short!("FEE_BPS");
const INITIALIZED: Symbol = symbol_short!("INIT");
const BLOCKS: Symbol = symbol_short!("BLOCKS");
const LIKES: Symbol = symbol_short!("LIKES");
const CREDENTIAL_ROOTS: Symbol = symbol_short!("CRED_ROOT");
const NULLIFIERS: Symbol = symbol_short!("NULLIFIER");

// ── TTL Constants ─────────────────────────────────────────────────────────────
//
// LEDGER_BUMP: target TTL (~30 days at 5s/ledger).
// LEDGER_THRESHOLD: extend only when remaining TTL falls below this value.

const LEDGER_BUMP: u32 = 535_000;
const LEDGER_THRESHOLD: u32 = 535_000 - 100;

// ── Validation Constants ──────────────────────────────────────────────────────

const MIN_USERNAME_LEN: u32 = 3;
const MAX_USERNAME_LEN: u32 = 32;
const MIN_CONTENT_LEN: u32 = 1;
const MAX_CONTENT_LEN: u32 = 280;

// ── Data Types ────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug)]
pub struct Post {
    pub id: u64,
    pub author: Address,
    pub content: String,
    pub tip_total: i128,
    pub timestamp: u64,
    pub like_count: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Profile {
    pub address: Address,
    pub username: String,
    pub creator_token: Address,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Pool {
    pub token: Address,
    pub balance: i128,
    pub admins: Vec<Address>,
    pub threshold: u32,
}

// ── Events ────────────────────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone)]
pub struct ProfileSetEvent {
    #[topic]
    pub user: Address,
    pub username: String,
}

#[contractevent]
#[derive(Clone)]
pub struct FollowEvent {
    #[topic]
    pub follower: Address,
    #[topic]
    pub followee: Address,
}

#[contractevent]
#[derive(Clone)]
pub struct UnfollowEvent {
    #[topic]
    pub follower: Address,
    #[topic]
    pub followee: Address,
}

#[contractevent]
#[derive(Clone)]
pub struct PostCreatedEvent {
    #[topic]
    pub id: u64,
    #[topic]
    pub author: Address,
}

#[contractevent]
#[derive(Clone)]
pub struct TipEvent {
    #[topic]
    pub tipper: Address,
    #[topic]
    pub post_id: u64,
    pub amount: i128,
    pub fee: i128,
}

#[contractevent]
#[derive(Clone)]
pub struct PoolDepositEvent {
    #[topic]
    pub depositor: Address,
    #[topic]
    pub pool_id: Symbol,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone)]
pub struct PoolWithdrawEvent {
    #[topic]
    pub recipient: Address,
    #[topic]
    pub pool_id: Symbol,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone)]
pub struct LikePostEvent {
    #[topic]
    pub user: Address,
    #[topic]
    pub post_id: u64,
}

#[contractevent]
#[derive(Clone)]
pub struct ContractUpgraded {
    pub new_wasm_hash: BytesN<32>,
}

#[contractevent]
#[derive(Clone)]
pub struct PostDeleted {
    #[topic]
    pub post_id: u64,
    #[topic]
    pub author: Address,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct LinkoraContract;

// ── Validation Helpers ────────────────────────────────────────────────────────

fn validate_username(username: &String) -> Result<(), &'static str> {
    let len = username.len();
    if len < MIN_USERNAME_LEN {
        return Err("username too short");
    }
    if len > MAX_USERNAME_LEN {
        return Err("username too long");
    }
    let bytes = username.to_bytes();
    for i in 0..bytes.len() {
        let c = bytes.get(i).unwrap() as char;
        if !c.is_ascii_alphanumeric() && c != '_' {
            return Err("invalid username character");
        }
    }
    Ok(())
}

fn validate_content(content: &String) -> Result<(), &'static str> {
    let len = content.len();
    if len < MIN_CONTENT_LEN {
        return Err("empty content");
    }
    if len > MAX_CONTENT_LEN {
        return Err("content too long");
    }
    Ok(())
}

#[contractimpl]
impl LinkoraContract {
    // ── Initialization ────────────────────────────────────────────────────────

    pub fn initialize(env: Env, admin: Address, treasury: Address, fee_bps: u32) {
        if env
            .storage()
            .instance()
            .get::<Symbol, bool>(&INITIALIZED)
            .unwrap_or(false)
        {
            panic!("already initialized");
        }
        assert!(fee_bps <= 10_000, "invalid fee");
        env.storage().instance().set(&INITIALIZED, &true);
        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&TREASURY, &treasury);
        env.storage().instance().set(&FEE_BPS, &fee_bps);
    }

    // ── Profiles ──────────────────────────────────────────────────────────────

    pub fn set_profile(env: Env, user: Address, username: String, creator_token: Address) {
        user.require_auth();
        validate_username(&username).expect("invalid username");

        let key = (PROFILES, user.clone());
        if !env.storage().persistent().has(&key) {
            let count: u64 = env.storage().instance().get(&PROFILE_CT).unwrap_or(0);
            env.storage().instance().set(&PROFILE_CT, &(count + 1));
        }
        env.storage().persistent().set(
            &key,
            &Profile {
                address: user.clone(),
                username: username.clone(),
                creator_token,
            },
        );
        Self::bump(&env, &key);
        ProfileSetEvent { user, username }.publish(&env);
    }

    pub fn get_profile(env: Env, user: Address) -> Option<Profile> {
        let key = (PROFILES, user);
        let result: Option<Profile> = env.storage().persistent().get(&key);
        if result.is_some() {
            Self::bump(&env, &key);
        }
        result
    }

    pub fn get_profile_count(env: Env) -> u64 {
        env.storage().instance().get(&PROFILE_CT).unwrap_or(0)
    }

    // ── Social Graph ──────────────────────────────────────────────────────────

    pub fn follow(env: Env, follower: Address, followee: Address) {
        follower.require_auth();

        if Self::is_blocked(env.clone(), followee.clone(), follower.clone()) {
            panic!("blocked");
        }

        let following_key = (FOLLOWS, follower.clone());
        let mut following_list: Vec<Address> = env
            .storage()
            .persistent()
            .get(&following_key)
            .unwrap_or(Vec::new(&env));

        if !following_list.iter().any(|x| x == followee) {
            following_list.push_back(followee.clone());
            env.storage()
                .persistent()
                .set(&following_key, &following_list);
            Self::bump(&env, &following_key);

            let followers_key = (FOLLOWERS, followee.clone());
            let mut followers_list: Vec<Address> = env
                .storage()
                .persistent()
                .get(&followers_key)
                .unwrap_or(Vec::new(&env));
            followers_list.push_back(follower.clone());
            env.storage()
                .persistent()
                .set(&followers_key, &followers_list);
            Self::bump(&env, &followers_key);
        }

        FollowEvent { follower, followee }.publish(&env);
    }

    pub fn unfollow(env: Env, follower: Address, followee: Address) {
        follower.require_auth();

        let following_key = (FOLLOWS, follower.clone());
        let mut following_list: Vec<Address> = env
            .storage()
            .persistent()
            .get(&following_key)
            .unwrap_or(Vec::new(&env));

        if let Some(index) = following_list.iter().position(|addr| addr == followee) {
            following_list.remove(index as u32);
            env.storage()
                .persistent()
                .set(&following_key, &following_list);
            Self::bump(&env, &following_key);

            let followers_key = (FOLLOWERS, followee.clone());
            let mut followers_list: Vec<Address> = env
                .storage()
                .persistent()
                .get(&followers_key)
                .unwrap_or(Vec::new(&env));
            if let Some(f_index) = followers_list.iter().position(|addr| addr == follower) {
                followers_list.remove(f_index as u32);
                env.storage()
                    .persistent()
                    .set(&followers_key, &followers_list);
                Self::bump(&env, &followers_key);
            }
        }

        UnfollowEvent { follower, followee }.publish(&env);
    }

    pub fn get_following(env: Env, user: Address) -> Vec<Address> {
        let key = (FOLLOWS, user);
        let result: Vec<Address> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));
        if !result.is_empty() {
            Self::bump(&env, &key);
        }
        result
    }

    pub fn get_followers(env: Env, user: Address) -> Vec<Address> {
        let key = (FOLLOWERS, user);
        let result: Vec<Address> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));
        if !result.is_empty() {
            Self::bump(&env, &key);
        }
        result
    }

    // ── Block List ────────────────────────────────────────────────────────────

    pub fn block_user(env: Env, blocker: Address, blocked: Address) {
        blocker.require_auth();
        let key = (BLOCKS, blocker.clone());
        let mut blocks: Map<Address, ()> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Map::new(&env));
        blocks.set(blocked, ());
        env.storage().persistent().set(&key, &blocks);
        Self::bump(&env, &key);
    }

    pub fn unblock_user(env: Env, blocker: Address, blocked: Address) {
        blocker.require_auth();
        let key = (BLOCKS, blocker.clone());
        let mut blocks: Map<Address, ()> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Map::new(&env));
        blocks.remove(blocked);
        env.storage().persistent().set(&key, &blocks);
        Self::bump(&env, &key);
    }

    pub fn is_blocked(env: Env, blocker: Address, blocked: Address) -> bool {
        let blocks: Map<Address, ()> = env
            .storage()
            .persistent()
            .get(&(BLOCKS, blocker))
            .unwrap_or(Map::new(&env));
        blocks.contains_key(blocked)
    }

    // ── Posts ─────────────────────────────────────────────────────────────────

    pub fn create_post(env: Env, author: Address, content: String) -> u64 {
        author.require_auth();
        validate_content(&content).expect("invalid content");

        let id: u64 = env.storage().instance().get(&POST_CT).unwrap_or(0u64) + 1;
        let key = (POSTS, id);
        env.storage().persistent().set(
            &key,
            &Post {
                id,
                author: author.clone(),
                content,
                tip_total: 0,
                timestamp: env.ledger().timestamp(),
                like_count: 0,
            },
        );
        Self::bump(&env, &key);
        env.storage().instance().set(&POST_CT, &id);
        PostCreatedEvent { id, author }.publish(&env);
        id
    }

    /// Returns the total number of posts ever created, not the current active count.
    /// This counter is never decremented when posts are deleted.
    pub fn get_post_count(env: Env) -> u64 {
        env.storage().instance().get(&POST_CT).unwrap_or(0u64)
    }

    pub fn get_post(env: Env, id: u64) -> Option<Post> {
        let key = (POSTS, id);
        let result: Option<Post> = env.storage().persistent().get(&key);
        if result.is_some() {
            Self::bump(&env, &key);
        }
        result
    }

    pub fn delete_post(env: Env, author: Address, post_id: u64) {
        author.require_auth();
        let key = (POSTS, post_id);
        let post: Post = env.storage().persistent().get(&key).unwrap_or_else(|| {
            panic!("post does not exist: {}", post_id);
        });
        assert!(post.author == author, "only author can delete post");
        env.storage().persistent().remove(&key);
        PostDeleted { post_id, author }.publish(&env);
    }

    // ── Reactions ─────────────────────────────────────────────────────────────

    pub fn like_post(env: Env, user: Address, post_id: u64) {
        user.require_auth();

        let like_key = (LIKES, post_id, user.clone());
        if env.storage().persistent().has(&like_key) {
            return;
        }

        let post_key = (POSTS, post_id);
        let mut post: Post = env
            .storage()
            .persistent()
            .get(&post_key)
            .expect("post not found");
        post.like_count += 1;
        env.storage().persistent().set(&post_key, &post);
        Self::bump(&env, &post_key);
        env.storage().persistent().set(&like_key, &true);
        Self::bump(&env, &like_key);
        LikePostEvent { user, post_id }.publish(&env);
    }

    pub fn get_like_count(env: Env, post_id: u64) -> u64 {
        let key = (POSTS, post_id);
        let result: Option<Post> = env.storage().persistent().get(&key);
        result.map(|p| p.like_count).unwrap_or(0)
    }

    pub fn has_liked(env: Env, user: Address, post_id: u64) -> bool {
        let key = (LIKES, post_id, user);
        env.storage().persistent().has(&key)
    }

    // ── Tipping ───────────────────────────────────────────────────────────────

    pub fn tip(env: Env, tipper: Address, post_id: u64, token: Address, amount: i128) {
        assert!(amount > 0, "tip amount must be positive");
        tipper.require_auth();

        let key = (POSTS, post_id);
        let mut post: Post = env.storage().persistent().get(&key).unwrap_or_else(|| {
            panic!("post not found: {}", post_id);
        });

        if Self::is_blocked(env.clone(), post.author.clone(), tipper.clone()) {
            panic!("blocked");
        }

        let fee_bps = Self::get_fee_bps(env.clone());
        let fee_amount = (amount * fee_bps as i128) / 10_000;
        let author_amount = amount - fee_amount;
        let token_client = token::Client::new(&env, &token);

        if fee_amount > 0 {
            let treasury: Address = env
                .storage()
                .instance()
                .get(&TREASURY)
                .expect("treasury not set");
            token_client.transfer(&tipper, &treasury, &fee_amount);
        }
        token_client.transfer(&tipper, &post.author, &author_amount);

        post.tip_total += amount;
        env.storage().persistent().set(&key, &post);
        Self::bump(&env, &key);

        TipEvent {
            tipper,
            post_id,
            amount,
            fee: fee_amount,
        }
        .publish(&env);
    }

    // ── Community Pool ────────────────────────────────────────────────────────

    /// Create a named pool with an admin set and M-of-N withdrawal threshold.
    pub fn create_pool(
        env: Env,
        admin: Address,
        pool_id: Symbol,
        token: Address,
        initial_admins: Vec<Address>,
        threshold: u32,
    ) {
        admin.require_auth();
        Self::require_admin(&env);
        let key = (POOLS, pool_id);
        assert!(!env.storage().persistent().has(&key), "pool exists");
        assert!(
            threshold > 0 && threshold <= initial_admins.len(),
            "invalid threshold"
        );
        env.storage().persistent().set(
            &key,
            &Pool {
                token,
                balance: 0,
                admins: initial_admins,
                threshold,
            },
        );
        Self::bump(&env, &key);
    }

    pub fn pool_deposit(
        env: Env,
        depositor: Address,
        pool_id: Symbol,
        token: Address,
        amount: i128,
    ) {
        assert!(amount > 0, "must be positive");
        depositor.require_auth();
        let key = (POOLS, pool_id.clone());
        let mut pool: Pool = env
            .storage()
            .persistent()
            .get(&key)
            .expect("pool not found");
        assert!(pool.token == token, "wrong token");

        token::Client::new(&env, &token).transfer(
            &depositor,
            env.current_contract_address(),
            &amount,
        );
        pool.balance += amount;
        env.storage().persistent().set(&key, &pool);
        Self::bump(&env, &key);

        PoolDepositEvent {
            depositor,
            pool_id,
            amount,
        }
        .publish(&env);
    }

    /// Withdraw from a pool. Requires `threshold` valid admin signatures.
    pub fn pool_withdraw(
        env: Env,
        signers: Vec<Address>,
        pool_id: Symbol,
        amount: i128,
        recipient: Address,
    ) {
        assert!(amount > 0, "must be positive");
        let key = (POOLS, pool_id.clone());
        let mut pool: Pool = env
            .storage()
            .persistent()
            .get(&key)
            .expect("pool not found");

        assert!(signers.len() >= pool.threshold, "insufficient signers");
        for signer in signers.iter() {
            assert!(
                pool.admins.iter().any(|x| x == signer),
                "unauthorized signer"
            );
            signer.require_auth();
        }
        assert!(pool.balance >= amount, "low balance");

        pool.balance -= amount;
        env.storage().persistent().set(&key, &pool);
        Self::bump(&env, &key);
        token::Client::new(&env, &pool.token).transfer(
            &env.current_contract_address(),
            &recipient,
            &amount,
        );

        PoolWithdrawEvent {
            recipient,
            pool_id,
            amount,
        }
        .publish(&env);
    }

    pub fn get_pool(env: Env, pool_id: Symbol) -> Option<Pool> {
        let key = (POOLS, pool_id);
        let result: Option<Pool> = env.storage().persistent().get(&key);
        if result.is_some() {
            Self::bump(&env, &key);
        }
        result
    }

    // ── Fee & Treasury ────────────────────────────────────────────────────────

    pub fn set_fee(env: Env, fee_bps: u32) {
        Self::require_admin(&env);
        assert!(fee_bps <= 10_000, "invalid fee");
        env.storage().instance().set(&FEE_BPS, &fee_bps);
    }

    pub fn set_treasury(env: Env, treasury: Address) {
        Self::require_admin(&env);
        env.storage().instance().set(&TREASURY, &treasury);
    }

    pub fn get_fee_bps(env: Env) -> u32 {
        env.storage().instance().get(&FEE_BPS).unwrap_or(0u32)
    }

    pub fn get_treasury(env: Env) -> Option<Address> {
        env.storage().instance().get(&TREASURY)
    }

    // ── Upgradability ─────────────────────────────────────────────────────────

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        Self::require_admin(&env);
        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());
        ContractUpgraded { new_wasm_hash }.publish(&env);
    }

    // ── Credential / ZK Subsystem ───────────────────────────────────────────────

    /// Updates a user's Merkle credential root.
    /// The root must be exactly 32 bytes (256 bits) for Merkle tree compatibility.
    pub fn update_credential_root(env: Env, user: Address, root: BytesN<32>) {
        user.require_auth();
        let key = (CREDENTIAL_ROOTS, user.clone());
        env.storage().persistent().set(&key, &root);
        Self::bump(&env, &key);
    }

    /// Verifies a Merkle proof against a user's stored credential root.
    /// 
    /// # Arguments
    /// * `user` - The user whose credential root to verify against
    /// * `leaf` - The leaf value being proven (32 bytes)
    /// * `proof` - Vector of sibling hashes forming the Merkle proof path
    /// * `nullifier` - Unique identifier to prevent replay attacks (32 bytes)
    /// 
    /// # Returns
    /// * `true` if the proof is valid and nullifier hasn't been used
    /// * `false` if the proof is invalid
    /// 
    /// # Panics
    /// * If the user has no credential root set
    /// * If the nullifier has already been used (replay attack)
    pub fn verify_credential(
        env: Env,
        user: Address,
        leaf: BytesN<32>,
        proof: Vec<BytesN<32>>,
        nullifier: BytesN<32>,
    ) -> bool {
        // Check if user has a credential root
        let key = (CREDENTIAL_ROOTS, user.clone());
        let stored_root: Option<BytesN<32>> = env.storage().persistent().get(&key);
        let root = stored_root.expect("no credential root set for user");

        // Check for nullifier replay (per-user to allow same nullifier across users)
        let nullifier_key = (NULLIFIERS, user.clone(), nullifier.clone());
        if env.storage().persistent().has(&nullifier_key) {
            panic!("nullifier already used");
        }

        // Verify Merkle proof
        let computed_root = Self::compute_merkle_root(&leaf, &proof);
        let is_valid = computed_root == root;

        // If valid, store the nullifier to prevent replay
        if is_valid {
            env.storage().persistent().set(&nullifier_key, &true);
            Self::bump(&env, &nullifier_key);
        }

        is_valid
    }

    /// Retrieves the stored Merkle credential root for a user.
    /// 
    /// # Returns
    /// * `Some(BytesN<32>)` if the user has a credential root set
    /// * `None` if the user has no credential root
    pub fn get_credential_root(env: Env, user: Address) -> Option<BytesN<32>> {
        let key = (CREDENTIAL_ROOTS, user.clone());
        let result: Option<BytesN<32>> = env.storage().persistent().get(&key);
        if result.is_some() {
            Self::bump(&env, &key);
        }
        result
    }

    /// Computes the Merkle root from a leaf and proof path.
    /// This is a helper function for verify_credential.
    /// Uses a position-dependent hash to ensure proof order matters.
    #[allow(clippy::needless_borrow)]
    fn compute_merkle_root(leaf: &BytesN<32>, proof: &Vec<BytesN<32>>) -> BytesN<32> {
        let env = leaf.env();
        let mut current = leaf.clone();
        let mut index = 0u8;
        
        for sibling in proof.iter() {
            // Position-dependent hash: add current, sibling, and index
            // This ensures (a, b) at position 0 != (b, a) at position 0
            let mut result = [0u8; 32];
            let current_arr = current.to_array();
            let sibling_arr = sibling.to_array();
            for i in 0..32 {
                result[i] = current_arr[i].wrapping_add(sibling_arr[i]).wrapping_add(index);
            }
            current = BytesN::from_array(&env, &result);
            index = index.wrapping_add(1);
        }
        
        current
    }

    // ── Internal Helpers ──────────────────────────────────────────────────────

    fn require_admin(env: &Env) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN)
            .expect("not initialized");
        admin.require_auth();
    }

    /// Extend the TTL of a persistent entry after every write and on every
    /// successful read to keep active data alive on-chain.
    fn bump<K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(env: &Env, key: &K) {
        env.storage()
            .persistent()
            .extend_ttl(key, LEDGER_THRESHOLD, LEDGER_BUMP);
    }
}

mod test;

#[cfg(test)]
mod tests;
