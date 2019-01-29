use rstd::prelude::*;
use srml_support::{dispatch::Result, StorageMap, StorageValue};
use {balances, system::ensure_signed};

// trait for this module
// contains type definitions
pub trait Trait: balances::Trait {
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

// public interface for this runtime module
decl_module! {
  pub struct Module<T: Trait> for enum Call where origin: T::Origin {
      // initialize the default event for this module
      fn deposit_event<T>() = default;

      // transfer tokens from one account to another
      fn transfer(_origin, to: T::AccountId, value: T::Balance) -> Result {
          let sender = ensure_signed(_origin)?;
          Self::_transfer(sender, to, value)
      }

      // approve token transfer from one account to another
      // if this is done, then transfer_from can be called with corresponding values
      fn approve(_origin, spender: T::AccountId, value: T::Balance) -> Result {
          let sender = ensure_signed(_origin)?;
          ensure!(<BalanceOf<T>>::exists(&sender), "Account does not own this token");
          Self::deposit_event(RawEvent::Approval(sender.clone(), spender.clone(), value));

          <Allowance<T>>::mutate((sender, spender), |allowance| *allowance += value);

          Ok(())
      }

      // if approved, transfer from an account to another account without needed owner's signature
      fn transfer_from(_origin, from: T::AccountId, to: T::AccountId, value: T::Balance) -> Result {
          ensure!(<Allowance<T>>::exists((from.clone(), to.clone())), "Allowance does not exist.");
          ensure!(Self::allowance((from.clone(), to.clone())) >= value, "Not enough allowance.");

          <Allowance<T>>::mutate((from.clone(), to.clone()), |allowance| *allowance -= value);
          Self::deposit_event(RawEvent::Approval(from.clone(), to.clone(), value));

          Self::_transfer(from, to, value)
      }
  }
}

// storage for this runtime module
decl_storage! {
  trait Store for Module<T: Trait> as Token {
    // bool flag to allow init to be called only once
    Init get(is_init): bool;
    // total supply of the token
    // set in the genesis config
    // see ../src/chain_spec.rs - line 105
    TotalSupply get(total_supply) config(): T::Balance;
    // mapping of balances to accounts
    BalanceOf get(balance_of): map T::AccountId => T::Balance;
    // mapping of allowances to accounts
    Allowance get(allowance): map (T::AccountId, T::AccountId) => T::Balance;
    // stores the total deposit for a listing
    // maps a listing hash with the total tokensface
    // TCR specific; not part of standard ERC20 interface
    LockedDeposits get(locked_deposits): map T::Hash => T::Balance;
  }
}

// events
decl_event!(
    pub enum Event<T> where AccountId = <T as system::Trait>::AccountId, 
    Balance = <T as balances::Trait>::Balance {
        // event for transfer of tokens
        // from, to, value
        Transfer(AccountId, AccountId, Balance),
        // event when an approval is made
        // owner, spender, value
        Approval(AccountId, AccountId, Balance),
    }
);

// implementation of mudule
// utility and private functions
// if marked public, accessible by other modules
impl<T: Trait> Module<T> {
    // initialize the token
    // transfers the total_supply amout to the caller
    // the token becomes usable
    // not part of ERC20 standard interface
    // replicates the ERC20 smart contract constructor functionality
    pub fn init(sender: T::AccountId) -> Result {
        ensure!(Self::is_init() == false, "Token already initialized.");

        <BalanceOf<T>>::insert(sender, Self::total_supply());
        <Init<T>>::put(true);

        Ok(())
    }

    // lock user deposits for curation actions
    // TCR specific; not part of standard ERC20 interface
    pub fn lock(from: T::AccountId, value: T::Balance, listing_hash: T::Hash) -> Result {
        ensure!(<BalanceOf<T>>::exists(from.clone()), "Account does not own this token");

        let sender_balance = Self::balance_of(from.clone());
        ensure!(sender_balance > value, "Not enough balance.");
        
        // deduct the deposit from balance
        <BalanceOf<T>>::mutate(from, |from_balance| *from_balance -= value);
        
        // add to deposits
        <LockedDeposits<T>>::mutate(listing_hash, |deposit| *deposit += value);

        Ok(())
    }

    // unlock user's deposit for reward claims and challenge wins
    // TCR specific; not part of standard ERC20 interface
    pub fn unlock(to: T::AccountId, value: T::Balance, listing_hash: T::Hash) -> Result {
        // add to user's balance
        <BalanceOf<T>>::mutate(to, |balance| *balance += value);
        // decrease from locked deposits
        <LockedDeposits<T>>::mutate(listing_hash, |deposit| *deposit -= value);

        Ok(())
    }

    // internal transfer function for ERC20 interface
    fn _transfer(
        from: T::AccountId,
        to: T::AccountId,
        value: T::Balance,
    ) -> Result {
        ensure!(<BalanceOf<T>>::exists(from.clone()), "Account does not own this token");

        let sender_balance = Self::balance_of(from.clone());
        ensure!(sender_balance > value, "Not enough balance.");

        Self::deposit_event(RawEvent::Transfer(from.clone(), to.clone(), value));
        
        // reduce sender's balance
        <BalanceOf<T>>::mutate(from, |from_balance| *from_balance -= value);

        // increase receiver's balance
        <BalanceOf<T>>::mutate(to, |balance| *balance += value);

        Ok(())
    }
}
