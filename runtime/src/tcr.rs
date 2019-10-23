use crate::token;
use codec::{Decode, Encode};
use rstd::prelude::*;
use sr_primitives::traits::{CheckedAdd, CheckedDiv, CheckedMul, Hash};
use support::{
  decl_event, decl_module, decl_storage, dispatch::Result, print, ensure,
};
use {system::ensure_signed, timestamp};

// Read TCR concepts here:
// https://www.gautamdhameja.com/token-curated-registries-explain-eli5-a5d4cce0ddbe/

// The module trait
pub trait Trait: timestamp::Trait + token::Trait {
  type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

#[cfg_attr(feature = "std", derive(Debug))]
#[derive(Encode, Decode, Default, Clone, PartialEq)]
// Generic type parameters - Balance, AccountId, timestamp::Moment
pub struct Listing<U, V, W> {
  id: u32,
  data: Vec<u8>,
  deposit: U,
  owner: V,
  application_expiry: W,
  whitelisted: bool,
  challenge_id: u32,
}

#[cfg_attr(feature = "std", derive(Debug))]
#[derive(Encode, Decode, Default, Clone, PartialEq)]
// Generic type parameters - Hash, Balance, AccountId, timestamp::Moment
pub struct Challenge<T, U, V, W> {
  listing_hash: T,
  deposit: U,
  owner: V,
  voting_ends: W,
  resolved: bool,
  reward_pool: U,
  total_tokens: U,
}

#[cfg_attr(feature = "std", derive(Debug))]
#[derive(Encode, Decode, Default, Clone, PartialEq)]
// Generic type parameters - Balance
pub struct Vote<U> {
  value: bool,
  deposit: U,
  claimed: bool,
}

#[cfg_attr(feature = "std", derive(Debug))]
#[derive(Encode, Decode, Default, Clone, PartialEq)]
// Generic type parameters - Balance
pub struct Poll<T, U> {
  listing_hash: T,
  votes_for: U,
  votes_against: U,
  passed: bool,
}

// Storage
decl_storage! {
  trait Store for Module<T: Trait> as Tcr {
    // Stores the owner in the genesis config.
    Owner get(owner) config(): T::AccountId;
    // Stores a list of admins who can set config.
    Admins get(admins): map T::AccountId => bool;
    // TCR parameter - minimum deposit.
    MinDeposit get(min_deposit) config(): Option<T::TokenBalance>;
    // TCR parameter - apply stage length - deadline for challenging before a listing gets accepted.
    ApplyStageLen get(apply_stage_len) config(): Option<T::Moment>;
    // TCR parameter - commit stage length - deadline for voting before a challenge gets resolved.
    CommitStageLen get(commit_stage_len) config(): Option<T::Moment>;
    // The TCR - list of proposals.
    Listings get(listings): map T::Hash => Listing<T::TokenBalance, T::AccountId, T::Moment>;
    // To make querying of listings easier, maintaining a list of indexes and corresponding listing hashes.
    ListingCount get(listing_count): u32;
    ListingIndexHash get(index_hash): map u32 => T::Hash;
    // global nonce for poll count.
    PollNonce get(poll_nonce) config(): u32;
    // Challenges.
    Challenges get(challenges): map u32 => Challenge<T::Hash, T::TokenBalance, T::AccountId, T::Moment>;
    // Polls.
    Polls get(polls): map u32 => Poll<T::Hash, T::TokenBalance>;
    // Votes.
    // Mapping is between a poll id and a vec of votes.
    // Poll and vote have a 1:n relationship.
    Votes get(votes): map (u32, T::AccountId) => Vote<T::TokenBalance>;
  }
}

// Events
decl_event!(
  pub enum Event<T> where AccountId = <T as system::Trait>::AccountId, 
  Balance = <T as token::Trait>::TokenBalance, 
  Hash = <T as system::Trait>::Hash {
    // When a listing is proposed.
    Proposed(AccountId, Hash, Balance),
    // When a listing is challenged.
    Challenged(AccountId, Hash, u32, Balance),
    // When a challenge is voted on.
    Voted(AccountId, u32, Balance),
    // When a challenge is resolved.
    Resolved(Hash, u32),
    // When a listing is accepted in the registry.
    Accepted(Hash),
    // When a listing is rejected from the registry.
    Rejected(Hash),
    // When a vote reward is claimed for a challenge.
    Claimed(AccountId, u32),
  }
);

// Module impl
decl_module! {
  pub struct Module<T: Trait> for enum Call where origin: T::Origin {
    // Initialize events for this module.
    fn deposit_event() = default;

    // Initialize the TCR.
    // Initialize token.
    // Make sender an admin if it's the owner account set in genesis config.
    // Owner then has all the tokens and admin rights to the TCR.
    // They can then distribute tokens in conventional ways.
    fn init(origin) {
      let sender = ensure_signed(origin)?;
      ensure!(sender == Self::owner(), "Only the owner set in genesis config can initialize the TCR");
      <token::Module<T>>::init(sender.clone())?;
      <Admins<T>>::insert(sender, true);
    }

    // Propose a listing on the registry.
    // Takes the listing name (data) as a byte vector.
    // Takes deposit as stake backing the listing.
    // Checks if the stake is less than minimum deposit needed.
    fn propose(origin, data: Vec<u8>, #[compact] deposit: T::TokenBalance) -> Result {
      let sender = ensure_signed(origin)?;

      // To avoid byte arrays with unlimited length.
      ensure!(data.len() <= 256, "listing data cannot be more than 256 bytes");

      let min_deposit = Self::min_deposit().ok_or("Min deposit not set")?;
      ensure!(deposit >= min_deposit, "deposit should be more than min_deposit");

      // Set application expiry for the listing.
      // Using the `Timestamp` SRML module for getting the block timestamp.
      // Generating a future timestamp by adding the apply stage length.
      let now = <timestamp::Module<T>>::get();
      let apply_stage_len = Self::apply_stage_len().ok_or("Apply stage length not set.")?;
      let app_exp = now.checked_add(&apply_stage_len).ok_or("Overflow when setting application expiry.")?;

      let hashed = <T as system::Trait>::Hashing::hash(&data);

      let listing_id = Self::listing_count();

      // Create a new listing instance and store it.
      let listing = Listing {
        id: listing_id,
        data,
        deposit,
        owner: sender.clone(),
        whitelisted: false,
        challenge_id: 0,
        application_expiry: app_exp,
      };

      ensure!(!<Listings<T>>::exists(hashed), "Listing already exists");

      // Deduct the deposit for application.
      <token::Module<T>>::lock(sender.clone(), deposit, hashed.clone())?;

      <ListingCount>::put(listing_id + 1);
      <Listings<T>>::insert(hashed, listing);
      <ListingIndexHash<T>>::insert(listing_id, hashed);

      // Let the world know.
      // Raise the event.
      Self::deposit_event(RawEvent::Proposed(sender, hashed.clone(), deposit));
      print("Listing created!");

      Ok(())
    }

    // Challenge a listing.
    // For simplicity, only three checks are being done.
    //    a. If the listing exists.
    //    c. If the challenger is not the owner of the listing.
    //    b. If enough deposit is sent for challenge.
    fn challenge(origin, listing_id: u32, #[compact] deposit: T::TokenBalance) -> Result {
      let sender = ensure_signed(origin)?;

      ensure!(<ListingIndexHash<T>>::exists(listing_id), "Listing not found.");

      let listing_hash = Self::index_hash(listing_id);
      let listing = Self::listings(listing_hash);

      ensure!(listing.challenge_id == 0, "Listing is already challenged.");
      ensure!(listing.owner != sender, "You cannot challenge your own listing.");
      ensure!(deposit >= listing.deposit, "Not enough deposit to challenge.");

      // Get current time.
      let now = <timestamp::Module<T>>::get();

      // Get commit stage length.
      let commit_stage_len = Self::commit_stage_len().ok_or("Commit stage length not set.")?;
      let voting_exp = now.checked_add(&commit_stage_len).ok_or("Overflow when setting voting expiry.")?;

      // Check apply stage length not passed.
      // Ensure listing.application_expiry < now.
      ensure!(listing.application_expiry > now, "Apply stage length has passed.");

      let challenge = Challenge {
        listing_hash,
        deposit,
        owner: sender.clone(),
        voting_ends: voting_exp,
        resolved: false,
        reward_pool: 0u32.into(),
        total_tokens: 0u32.into(),
      };

      let poll = Poll {
        listing_hash,
        votes_for: listing.deposit,
        votes_against: deposit,
        passed: false,
      };

      // Deduct the deposit for challenge.
      <token::Module<T>>::lock(sender.clone(), deposit, listing_hash)?;

      // Global poll nonce.
      // Helps keep the count of challenges and in maping votes.
      let poll_nonce = <PollNonce>::get();

      // Add a new challenge and the corresponding poll in the respective collections.
      <Challenges<T>>::insert(poll_nonce, challenge);
      <Polls<T>>::insert(poll_nonce, poll);

      // Update listing with challenge id.
      <Listings<T>>::mutate(listing_hash, |listing| {
        listing.challenge_id = poll_nonce;
      });

      // Update the poll nonce.
      <PollNonce>::put(poll_nonce + 1);

      // Raise the event.
      Self::deposit_event(RawEvent::Challenged(sender, listing_hash, poll_nonce, deposit));
      print("Challenge created!");

      Ok(())
    }

    // Registers a vote for a particular challenge.
    // Checks if the listing is challenged, and
    // if the commit stage length has not passed.
    // To keep it simple, we just store the choice as a bool - true: aye; false: nay.
    fn vote(origin, challenge_id: u32, value: bool, #[compact] deposit: T::TokenBalance) -> Result {
      let sender = ensure_signed(origin)?;

      // Check if listing is challenged.
      ensure!(<Challenges<T>>::exists(challenge_id), "Challenge does not exist.");
      let challenge = Self::challenges(challenge_id);
      ensure!(challenge.resolved == false, "Challenge is already resolved.");

      // Check commit stage length not passed.
      let now = <timestamp::Module<T>>::get();
      ensure!(challenge.voting_ends > now, "Commit stage length has passed.");

      // Deduct the deposit for vote.
      <token::Module<T>>::lock(sender.clone(), deposit, challenge.listing_hash)?;

      let mut poll_instance = Self::polls(challenge_id);
      // Based on vote value, increase the count of votes (for or against).
      match value {
        true => poll_instance.votes_for += deposit,
        false => poll_instance.votes_against += deposit,
      }

      // Create a new vote instance with the input params.
      let vote_instance = Vote {
        value,
        deposit,
        claimed: false,
      };

      // Mutate polls collection to update the poll instance.
      <Polls<T>>::mutate(challenge_id, |poll| *poll = poll_instance);

      // Insert new vote into votes collection.
      <Votes<T>>::insert((challenge_id, sender.clone()), vote_instance);

      // Raise the event.
      Self::deposit_event(RawEvent::Voted(sender, challenge_id, deposit));
      print("Vote created!");
      Ok(())
    }

    // Resolves the status of a listing.
    // Changes the value of whitelisted to either true or false.
    // Checks if the listing is challenged or not.
    // Further checks if apply stage or commit stage has passed.
    // Compares if votes are in favour of whitelisting.
    // Updates the listing status.
    fn resolve(_origin, listing_id: u32) -> Result {
      ensure!(<ListingIndexHash<T>>::exists(listing_id), "Listing not found.");

      let listing_hash = Self::index_hash(listing_id);
      let listing = Self::listings(listing_hash);

      let now = <timestamp::Module<T>>::get();
      let challenge;
      let poll;

      // Check if listing is challenged.
      if listing.challenge_id > 0 {
        // Challenge.
        challenge = Self::challenges(listing.challenge_id);
        poll = Self::polls(listing.challenge_id);

        // Check commit stage length has passed.
        ensure!(challenge.voting_ends < now, "Commit stage length has not passed.");
      } else {
        // No challenge.
        // Check if apply stage length has passed.
        ensure!(listing.application_expiry < now, "Apply stage length has not passed.");

        // Update listing status.
        <Listings<T>>::mutate(listing_hash, |listing|
        {
          listing.whitelisted = true;
        });

        Self::deposit_event(RawEvent::Accepted(listing_hash));
        return Ok(());
      }

      let mut whitelisted = false;

      // Mutate polls collection to update the poll instance.
      <Polls<T>>::mutate(listing.challenge_id, |poll| {
        if poll.votes_for >= poll.votes_against {
            poll.passed = true;
            whitelisted = true;
        } else {
            poll.passed = false;
        }
      });

      // Update listing status.
      <Listings<T>>::mutate(listing_hash, |listing| {
        listing.whitelisted = whitelisted;
        listing.challenge_id = 0;
      });

      // Update challenge.
      <Challenges<T>>::mutate(listing.challenge_id, |challenge| {
        challenge.resolved = true;
        if whitelisted == true {
          challenge.total_tokens = poll.votes_for;
          challenge.reward_pool = challenge.deposit + poll.votes_against;
        } else {
          challenge.total_tokens = poll.votes_against;
          challenge.reward_pool = listing.deposit + poll.votes_for;
        }
      });

      // Raise appropriate event as per whitelisting status.
      if whitelisted == true {
        Self::deposit_event(RawEvent::Accepted(listing_hash));
      } else {
        // If rejected, give challenge deposit back to the challenger.
        <token::Module<T>>::unlock(challenge.owner, challenge.deposit, listing_hash)?;
        Self::deposit_event(RawEvent::Rejected(listing_hash));
      }

      Self::deposit_event(RawEvent::Resolved(listing_hash, listing.challenge_id));
      Ok(())
    }

    // Claim reward for a vote.
    fn claim_reward(origin, challenge_id: u32) -> Result {
      let sender = ensure_signed(origin)?;

      // Ensure challenge exists and has been resolved.
      ensure!(<Challenges<T>>::exists(challenge_id), "Challenge not found.");
      let challenge = Self::challenges(challenge_id);
      ensure!(challenge.resolved == true, "Challenge is not resolved.");

      // Get the poll and vote instances.
      // Reward depends on poll passed status and vote value.
      let poll = Self::polls(challenge_id);
      let vote = Self::votes((challenge_id, sender.clone()));

      // Ensure vote reward is not already claimed.
      ensure!(vote.claimed == false, "Vote reward has already been claimed.");

      // If winning party, calculate reward and transfer.
      if poll.passed == vote.value {
            let reward_ratio = challenge.reward_pool.checked_div(&challenge.total_tokens).ok_or("overflow in calculating reward")?;
            let reward = reward_ratio.checked_mul(&vote.deposit).ok_or("overflow in calculating reward")?;
            let total = reward.checked_add(&vote.deposit).ok_or("overflow in calculating reward")?;
            <token::Module<T>>::unlock(sender.clone(), total, challenge.listing_hash)?;

            Self::deposit_event(RawEvent::Claimed(sender.clone(), challenge_id));
        }

        // Update vote reward claimed status.
        <Votes<T>>::mutate((challenge_id, sender), |vote| vote.claimed = true);

      Ok(())
    }

    // Sets the TCR parameters.
    // Currently only min deposit, apply stage length and commit stage length are supported.
    // Only admins can set config.
    // Repeated setting just overrides, for simplicity.
    fn set_config(origin,
      min_deposit: T::TokenBalance,
      apply_stage_len: T::Moment,
      commit_stage_len: T::Moment) -> Result {

      Self::ensure_admin(origin)?;

      <MinDeposit<T>>::put(min_deposit);
      <ApplyStageLen<T>>::put(apply_stage_len);
      <CommitStageLen<T>>::put(commit_stage_len);

      Ok(())
    }

    // Add a new admin for the TCR.
    // Admins can do specific operations.
    // Set config.
    fn add_admin(origin, new_admin: T::AccountId) -> Result {
      Self::ensure_admin(origin)?;

      <Admins<T>>::insert(new_admin, true);
      print("New admin added!");
      Ok(())
    }

    // Remove an admin.
    fn remove_admin(origin, admin_to_remove: T::AccountId) -> Result {
      Self::ensure_admin(origin)?;

      ensure!(<Admins<T>>::exists(&admin_to_remove), "The admin you are trying to remove does not exist");
      <Admins<T>>::remove(admin_to_remove);
      print("Admin removed!");
      Ok(())
    }
  }
}

// Utility and private functions.
impl<T: Trait> Module<T> {
  // Ensure that a user is an admin.
  fn ensure_admin(origin: T::Origin) -> Result {
    let sender = ensure_signed(origin)?;

    ensure!(<Admins<T>>::exists(&sender), "Access denied. Admin only.");
    ensure!(Self::admins(sender) == true, "Admin is not active");

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use primitives::{Blake2Hasher, H256};
  use runtime_io::with_externalities;
  use runtime_primitives::{
    testing::{Digest, DigestItem, Header, UintAuthorityId},
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
  };
  use support::{assert_noop, assert_ok, impl_outer_origin};

  impl_outer_origin! {
    pub enum Origin for Test {}
  }

  // For testing the module, we construct most of a mock runtime. This means
  // first constructing a configuration type (`Test`) which `impl`s each of the
  // configuration traits of modules we want to use.
  #[derive(Clone, Eq, PartialEq)]
  pub struct Test;
  impl system::Trait for Test {
    type Origin = Origin;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type Digest = Digest;
    type AccountId = u64;
    type Lookup = IdentityLookup<u64>;
    type Header = Header;
    type Event = ();
    type Log = DigestItem;
  }
  impl consensus::Trait for Test {
    type Log = DigestItem;
    type SessionKey = UintAuthorityId;
    type InherentOfflineReport = ();
  }
  impl token::Trait for Test {
    type Event = ();
    type TokenBalance = u64;
  }
  impl timestamp::Trait for Test {
    type Moment = u64;
    type OnTimestampSet = ();
  }
  impl Trait for Test {
    type Event = ();
  }
  type Tcr = Module<Test>;
  type Token = token::Module<Test>;

  // Builds the genesis config store and sets mock values.
  fn new_test_ext() -> runtime_io::TestExternalities<Blake2Hasher> {
    let mut t = system::GenesisConfig::<Test>::default()
      .build_storage()
      .unwrap()
      .0;
    t.extend(
      token::GenesisConfig::<Test> { total_supply: 1000 }
        .build_storage()
        .unwrap()
        .0,
    );
    t.extend(
      GenesisConfig::<Test> {
        owner: 1,
        min_deposit: 100,
        apply_stage_len: 10,
        commit_stage_len: 10,
        poll_nonce: 1,
      }
      .build_storage()
      .unwrap()
      .0,
    );
    t.into()
  }

  #[test]
  fn should_fail_low_deposit() {
    with_externalities(&mut new_test_ext(), || {
      assert_noop!(
        Tcr::propose(Origin::signed(1), "ListingItem1".as_bytes().into(), 99),
        "deposit should be more than min_deposit"
      );
    });
  }

  #[test]
  fn should_init() {
    with_externalities(&mut new_test_ext(), || {
      assert_ok!(Tcr::init(Origin::signed(1)));
    });
  }

  #[test]
  fn should_pass_propose() {
    with_externalities(&mut new_test_ext(), || {
      assert_ok!(Tcr::init(Origin::signed(1)));
      assert_ok!(Tcr::propose(
        Origin::signed(1),
        "ListingItem1".as_bytes().into(),
        101
      ));
    });
  }

  #[test]
  fn should_fail_challenge_same_owner() {
    with_externalities(&mut new_test_ext(), || {
      assert_ok!(Tcr::init(Origin::signed(1)));
      assert_ok!(Tcr::propose(
        Origin::signed(1),
        "ListingItem1".as_bytes().into(),
        101
      ));
      assert_noop!(
        Tcr::challenge(Origin::signed(1), 0, 101),
        "You cannot challenge your own listing."
      );
    });
  }

  #[test]
  fn should_pass_challenge() {
    with_externalities(&mut new_test_ext(), || {
      assert_ok!(Tcr::init(Origin::signed(1)));
      assert_ok!(Tcr::propose(
        Origin::signed(1),
        "ListingItem1".as_bytes().into(),
        101
      ));
      assert_ok!(Token::transfer(Origin::signed(1), 2, 200));
      assert_ok!(Tcr::challenge(Origin::signed(2), 0, 101));
    });
  }
}
