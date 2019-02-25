use rstd::prelude::*;
use runtime_io;
use parity_codec_derive::{Encode, Decode};
use runtime_primitives::traits::{Hash, As, CheckedDiv, CheckedMul, CheckedAdd};
use support::{dispatch::Result, StorageMap, StorageValue, decl_storage, decl_module, decl_event, ensure};
use {system::ensure_signed, timestamp};
use crate::token;

// Read TCR concepts here
// https://www.gautamdhameja.com/token-curated-registries-explain-eli5-a5d4cce0ddbe/

// the module trait
pub trait Trait: timestamp::Trait + token::Trait {
  type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug)]
// generic type parameters - Balance, AccountId, timestamp::Moment
pub struct Listing<U, V, W> {
  id: u32,
  data: Vec<u8>,
  deposit: U,
  owner: V,
  application_expiry: W,
  whitelisted: bool,
  challenge_id: u32,
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug)]
// generic type parameters - Balance, AccountId, timestamp::Moment
pub struct Challenge<T, U, V, W> {
  listing_hash: T,
  deposit: U,
  owner: V,
  voting_ends: W,
  resolved: bool,
  reward_pool: U,
  total_tokens: U
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug)]
// generic type parameters - Balance
pub struct Vote<U> {
  value: bool,
  deposit: U,
  claimed: bool,
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug)]
// generic type parameters - Balance
pub struct Poll<T, U> {
  listing_hash: T,
  votes_for: U,
  votes_against: U,
  passed: bool,
}

// storage
decl_storage! {
  trait Store for Module<T: Trait> as Tcr {
    // stores the owner in the genesis config
    Owner get(owner) config(): T::AccountId;
    // stores a list of admins who can set config
    Admins get(admins): map T::AccountId => bool;
    // TCR parameter - minimum deposit
    MinDeposit get(min_deposit) config(): Option<T::TokenBalance>;
    // TCR parameter - apply stage length - deadline for challenging before a listing gets accepted
    ApplyStageLen get(apply_stage_len) config(): Option<T::Moment>;
    // TCR parameter - commit stage length - deadline for voting before a challenge gets resolved
    CommitStageLen get(commit_stage_len) config(): Option<T::Moment>;
    // the TCR - list of proposals
    Listings get(listings): map T::Hash => Listing<T::TokenBalance, T::AccountId, T::Moment>;
    // to make querying of listings easier, maintaining a list of indexes and corresponding listing hashes
    ListingCount get(listing_count): u32;
    ListingIndexHash get(index_hash): map u32 => T::Hash;
    // global nonce for poll count
    PollNonce get(poll_nonce) config(): u32;
    // challenges
    Challenges get(challenges): map u32 => Challenge<T::Hash, T::TokenBalance, T::AccountId, T::Moment>;
    // polls
    Polls get(polls): map u32 => Poll<T::Hash, T::TokenBalance>;
    // votes
    // mapping is between a poll id and a vec of votes
    // poll and vote have a 1:n relationship
    Votes get(votes): map (u32, T::AccountId) => Vote<T::TokenBalance>;
  }
}

// events
decl_event!(
    pub enum Event<T> where AccountId = <T as system::Trait>::AccountId, 
    Balance = <T as token::Trait>::TokenBalance, 
    Hash = <T as system::Trait>::Hash {
      // when a listing is proposed
      Proposed(AccountId, Hash, Balance),
      // when a listing is challenged
      Challenged(AccountId, Hash, u32, Balance),
      // when a challenge is voted on
      Voted(AccountId, u32, Balance),
      // when a challenge is resolved
      Resolved(Hash, u32),
      // when a listing is accepted in the registry
      Accepted(Hash),
      // when a listing is rejected from the registry
      Rejected(Hash),
      // when a vote reward is claimed for a challenge
      Claimed(AccountId, u32),
    }
);

// module declaration
// public interface
decl_module! {
  pub struct Module<T: Trait> for enum Call where origin: T::Origin {
    // initialize events for this module
    fn deposit_event<T>() = default;

    // initialize the tcr
    // initialize token
    // make sender an admin if it's the owner account set in genesis config
    // owner then has all the tokens and admin rights to the TCR
    // they can then distribute tokens in conventional ways
    fn init(origin) {
      let sender = ensure_signed(origin)?;
      ensure!(sender == Self::owner(), "Only the owner set in genesis config can initialize the TCR");
      <token::Module<T>>::init(sender.clone())?;
      <Admins<T>>::insert(sender, true);
    }

    // propose a listing on the registry
    // takes the listing name (data) as a byte vector
    // takes deposit as stake backing the listing
    // checks if the stake is less than minimum deposit needed
    fn propose(origin, data: Vec<u8>, deposit: T::TokenBalance) -> Result {
      let sender = ensure_signed(origin)?;
      
      // to avoid byte arrays with unlimited length
      ensure!(data.len() <= 256, "listing data cannot be more than 256 bytes");
      
      let min_deposit = Self::min_deposit().ok_or("Min deposit not set")?;
      ensure!(deposit >= min_deposit, "deposit should be more than min_deposit");
      
      // set application expiry for the listing
      // using the `Timestamp` SRML module for getting the block timestamp
      // generating a future timestamp by adding the apply stage length
      let now = <timestamp::Module<T>>::get();
      let apply_stage_len = Self::apply_stage_len().ok_or("Apply stage length not set.")?;
      let app_exp = now.checked_add(&apply_stage_len).ok_or("Overflow when setting application expiry.")?;

      let hashed = <T as system::Trait>::Hashing::hash(&data);

      let listing_id = Self::listing_count();

      // create a new listing instance and store it
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

      // deduct the deposit for application
      <token::Module<T>>::lock(sender.clone(), deposit, hashed.clone())?;

      <ListingCount<T>>::put(listing_id + 1);
      <Listings<T>>::insert(hashed, listing);
      <ListingIndexHash<T>>::insert(listing_id, hashed);

      // let the world know
      // raise the event
      Self::deposit_event(RawEvent::Proposed(sender, hashed.clone(), deposit));
      runtime_io::print("Listing created!");

      Ok(())
    }

    // challenge a listing
    // for simplicity, only three checks are being done
    //    a. if the listing exists
    //    c. if the challenger is not the owner of the listing
    //    b. if enough deposit is sent for challenge
    fn challenge(origin, listing_id: u32, deposit: T::TokenBalance) -> Result {
      let sender = ensure_signed(origin)?;
      
      ensure!(<ListingIndexHash<T>>::exists(listing_id), "Listing not found.");
      
      let listing_hash = Self::index_hash(listing_id);
      let listing = Self::listings(listing_hash);

      ensure!(listing.challenge_id == 0, "Listing is already challenged.");
      ensure!(listing.owner != sender, "You cannot challenge your own listing.");
      ensure!(deposit >= listing.deposit, "Not enough deposit to challenge.");
      
      // get current time
      let now = <timestamp::Module<T>>::get();

      // get commit stage length
      let commit_stage_len = Self::commit_stage_len().ok_or("Commit stage length not set.")?;
      let voting_exp = now.checked_add(&commit_stage_len).ok_or("Overflow when setting voting expiry.")?;

      // check apply stage length not passed
      // ensure listing.application_expiry < now
      ensure!(listing.application_expiry > now, "Apply stage length has passed.");

      let challenge = Challenge {
        listing_hash,
        deposit,
        owner: sender.clone(),
        voting_ends: voting_exp,
        resolved: false,
        reward_pool: <T::TokenBalance as As<u64>>::sa(0),
        total_tokens: <T::TokenBalance as As<u64>>::sa(0),
      };

      let poll = Poll {
        listing_hash,
        votes_for: listing.deposit,
        votes_against: deposit,
        passed: false,
      };

      // deduct the deposit for challenge
      <token::Module<T>>::lock(sender.clone(), deposit, listing_hash)?;

      // global poll nonce
      // helps keep the count of challenges and in maping votes
      let poll_nonce = <PollNonce<T>>::get();
      // add a new challenge and the corresponding poll in the respective collections
      <Challenges<T>>::insert(poll_nonce, challenge);
      <Polls<T>>::insert(poll_nonce, poll);

      // update listing with challenge id
      <Listings<T>>::mutate(listing_hash, |listing| {
        listing.challenge_id = poll_nonce;
      });

      // update the poll nonce
      <PollNonce<T>>::put(poll_nonce + 1);

      // raise the event
      Self::deposit_event(RawEvent::Challenged(sender, listing_hash, poll_nonce, deposit));
      runtime_io::print("Challenge created!");

      Ok(())
    }

    // registers a vote for a particular challenge
    // checks if the listing is challenged and
    // if the commit stage length has not passed
    // to keep it simple, we just store the choice as a bool - true: aye; false: nay
    fn vote(origin, challenge_id: u32, value: bool, deposit: T::TokenBalance) -> Result {
      let sender = ensure_signed(origin)?;

      // check if listing is challenged
      ensure!(<Challenges<T>>::exists(challenge_id), "Challenge does not exist.");
      let challenge = Self::challenges(challenge_id);
      ensure!(challenge.resolved == false, "Challenge is already resolved.");

      // check commit stage length not passed
      let now = <timestamp::Module<T>>::get();
      ensure!(challenge.voting_ends > now, "Commit stage length has passed.");

      // deduct the deposit for vote
      <token::Module<T>>::lock(sender.clone(), deposit, challenge.listing_hash)?;

      let mut poll_instance = Self::polls(challenge_id);
      // based on vote value, increase the count of votes (for or against)
      match value {
        true => poll_instance.votes_for += deposit,
        false => poll_instance.votes_against += deposit,
      }

      // create a new vote instance with the input params
      let vote_instance = Vote {
        value,
        deposit,
        claimed: false,
      };

      // mutate polls collection to update the poll instance
      <Polls<T>>::mutate(challenge_id, |poll| *poll = poll_instance);

      // insert new vote into votes collection
      <Votes<T>>::insert((challenge_id, sender.clone()), vote_instance);

      // raise the event
      Self::deposit_event(RawEvent::Voted(sender, challenge_id, deposit));
      runtime_io::print("Vote created!");
      Ok(())
    }

    // resolves the status of a listing
    // changes the value of whitelisted to either true or false
    // checks if the listing is challenged or not
    //    further checks if apply stage or commit stage has passed
    // compares if votes are in favour of whitelisting
    // updates the listing status
    fn resolve(_origin, listing_id: u32) -> Result {
      ensure!(<ListingIndexHash<T>>::exists(listing_id), "Listing not found.");
      
      let listing_hash = Self::index_hash(listing_id);
      let listing = Self::listings(listing_hash);

      let now = <timestamp::Module<T>>::get();
      let challenge;
      let poll;

      // check if listing is challenged
      if listing.challenge_id > 0 {
        // challenge
        challenge = Self::challenges(listing.challenge_id);
        poll = Self::polls(listing.challenge_id);

        // check commit stage length has passed
        ensure!(challenge.voting_ends < now, "Commit stage length has not passed.");
      } else {
        // no challenge
        // check if apply stage length has passed
        ensure!(listing.application_expiry < now, "Apply stage length has not passed.");
        
        // update listing status
        <Listings<T>>::mutate(listing_hash, |listing| 
        {
          listing.whitelisted = true;
        });

        Self::deposit_event(RawEvent::Accepted(listing_hash));
        return Ok(());
      }

      let mut whitelisted = false;

      // mutate polls collection to update the poll instance
      <Polls<T>>::mutate(listing.challenge_id, |poll| {
        if poll.votes_for >= poll.votes_against {
            poll.passed = true;
            whitelisted = true;
        } else {
            poll.passed = false;
        }
      });

      // update listing status
      <Listings<T>>::mutate(listing_hash, |listing| {
        listing.whitelisted = whitelisted;
        listing.challenge_id = 0;
      });

      // update challenge
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

      // raise appropriate event as per whitelisting status
      if whitelisted == true {
        Self::deposit_event(RawEvent::Accepted(listing_hash));
      } else {
        // if rejected, give challenge deposit back to the challenger
        <token::Module<T>>::unlock(challenge.owner, challenge.deposit, listing_hash)?;
        Self::deposit_event(RawEvent::Rejected(listing_hash));
      }

      Self::deposit_event(RawEvent::Resolved(listing_hash, listing.challenge_id));
      Ok(())
    }

    // claim reward for a vote
    fn claim_reward(origin, challenge_id: u32) -> Result {
      let sender = ensure_signed(origin)?;

      // ensure challenge exists and has been resolved
      ensure!(<Challenges<T>>::exists(challenge_id), "Challenge not found.");
      let challenge = Self::challenges(challenge_id);
      ensure!(challenge.resolved == true, "Challenge is not resolved.");

      // get the poll and vote instances
      // reward depends on poll passed status and vote value
      let poll = Self::polls(challenge_id);
      let vote = Self::votes((challenge_id, sender.clone()));

      // ensure vote reward is not already claimed
      ensure!(vote.claimed == false, "Vote reward has already been claimed.");

      // if winning party, calculate reward and transfer
      if (poll.passed == true && vote.value == true) || 
          (poll.passed == false && vote.value == false) {
            let reward_ratio = challenge.reward_pool.checked_div(&challenge.total_tokens).ok_or("overflow in calculating reward")?;
            let reward = reward_ratio.checked_mul(&vote.deposit).ok_or("overflow in calculating reward")?;
            let total = reward.checked_add(&vote.deposit).ok_or("overflow in calculating reward")?;
            <token::Module<T>>::unlock(sender.clone(), total, challenge.listing_hash)?;
            
            Self::deposit_event(RawEvent::Claimed(sender.clone(), challenge_id));
        }

        // update vote reward claimed status
        <Votes<T>>::mutate((challenge_id, sender), |vote| vote.claimed = true);

      Ok(())
    }

    // sets the TCR parameters
    // currently only min deposit, apply stage length and commit stage length are supported
    // only admins can set config
    // repeated setting just overrides, for simplicity
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

    // add a new admin for the TCR
    // admins can do specific operations
    //    set config
    fn add_admin(origin, new_admin: T::AccountId) -> Result {
      Self::ensure_admin(origin)?;

      <Admins<T>>::insert(new_admin, true);
      runtime_io::print("New admin added!");
      Ok(())
    }

    // remove an admin
    fn remove_admin(origin, admin_to_remove: T::AccountId) -> Result {
      Self::ensure_admin(origin)?;

      ensure!(<Admins<T>>::exists(&admin_to_remove), "The admin you are trying to remove does not exist");
      <Admins<T>>::remove(admin_to_remove);
      runtime_io::print("Admin removed!");
      Ok(())
    }
  }
}

// implementation of mudule
// utility and private functions
impl<T: Trait> Module<T> {
    // ensure that a user is an admin
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

	use runtime_io::with_externalities;
	use primitives::{H256, Blake2Hasher};
	use support::{impl_outer_origin, assert_ok, assert_noop};
	use runtime_primitives::{
		BuildStorage,
		traits::{BlakeTwo256, IdentityLookup},
		testing::{Digest, DigestItem, Header, UintAuthorityId}
	};

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

	// builds the genesis config store and sets mock values
	fn new_test_ext() -> runtime_io::TestExternalities<Blake2Hasher> {
		let mut t = system::GenesisConfig::<Test>::default().build_storage().unwrap().0;
    t.extend(
			token::GenesisConfig::<Test> {
				total_supply: 1000
			}.build_storage().unwrap().0);
		t.extend(GenesisConfig::<Test> {
      owner: 1,
			min_deposit: 100,
      apply_stage_len: 10,
      commit_stage_len: 10,
      poll_nonce: 1
		}.build_storage().unwrap().0);
    t.into()
	}

	#[test]
	fn should_fail_low_deposit() {
    with_externalities(&mut new_test_ext(), || {
			assert_noop!(Tcr::propose(Origin::signed(1), "ListingItem1".as_bytes().into(), 99), "deposit should be more than min_deposit");
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
			assert_ok!(Tcr::propose(Origin::signed(1), "ListingItem1".as_bytes().into(), 101));
		});
	}

  #[test]
  fn should_fail_challenge_same_owner() {
    with_externalities(&mut new_test_ext(), || {
      assert_ok!(Tcr::init(Origin::signed(1)));
			assert_ok!(Tcr::propose(Origin::signed(1), "ListingItem1".as_bytes().into(), 101));
      assert_noop!(Tcr::challenge(Origin::signed(1), 0, 101), "You cannot challenge your own listing.");
		});
	}

  #[test]
  fn should_pass_challenge() {
    with_externalities(&mut new_test_ext(), || {
      assert_ok!(Tcr::init(Origin::signed(1)));
			assert_ok!(Tcr::propose(Origin::signed(1), "ListingItem1".as_bytes().into(), 101));
      assert_ok!(Token::transfer(Origin::signed(1), 2, 200));
      assert_ok!(Tcr::challenge(Origin::signed(2), 0, 101));
		});
	}
}