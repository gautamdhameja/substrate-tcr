use rstd::prelude::*;
use support::{dispatch::Result, StorageMap, StorageValue, decl_storage, decl_module, decl_event, ensure};
use {balances, system::ensure_signed};
use runtime_primitives::traits::{CheckedSub, CheckedAdd};

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
      // once this is done, then transfer_from can be called with corresponding values
      fn approve(_origin, spender: T::AccountId, value: T::Balance) -> Result {
          let sender = ensure_signed(_origin)?;
          ensure!(<BalanceOf<T>>::exists(&sender), "Account does not own this token");

          let allowance = Self::allowance((sender.clone(), spender.clone()));
          // using checked_add (safe math) to avoid overflow
          let updated_allowance = allowance.checked_add(&value).ok_or("overflow in calculating allowance")?;

          <Allowance<T>>::mutate((sender.clone(), spender.clone()), |allowance| {
                *allowance = updated_allowance;
          });

          Self::deposit_event(RawEvent::Approval(sender, spender, value));
          Ok(())
      }

      // if approved, transfer from an account to another account without needing owner's signature
      fn transfer_from(_origin, from: T::AccountId, to: T::AccountId, value: T::Balance) -> Result {
          ensure!(<Allowance<T>>::exists((from.clone(), to.clone())), "Allowance does not exist.");
          let allowance = Self::allowance((from.clone(), to.clone()));
          ensure!(allowance >= value, "Not enough allowance.");
          // using checked_sub (safe math) to avoid overflow
          let updated_allowance = allowance.checked_sub(&value).ok_or("overflow in calculating allowance")?;

          <Allowance<T>>::mutate((from.clone(), to.clone()), |allowance| {
                *allowance = updated_allowance;
          });

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
        let updated_from_balance = sender_balance.checked_sub(&value).ok_or("overflow in calculating balance")?;
        let deposit = Self::locked_deposits(listing_hash);
        let updated_deposit = deposit.checked_add(&value).ok_or("overflow in calculating deposit")?;

        // deduct the deposit from balance
        <BalanceOf<T>>::mutate(from, |from_balance| {
            *from_balance = updated_from_balance;
        });
        
        // add to deposits
        <LockedDeposits<T>>::mutate(listing_hash, |deposit| {
            *deposit = updated_deposit;
        });

        Ok(())
    }

    // unlock user's deposit for reward claims and challenge wins
    // TCR specific; not part of standard ERC20 interface
    pub fn unlock(to: T::AccountId, value: T::Balance, listing_hash: T::Hash) -> Result {
        let to_balance = Self::balance_of(to.clone());
        let updated_to_balance = to_balance.checked_add(&value).ok_or("overflow in calculating balance")?;
        let deposit = Self::locked_deposits(listing_hash);
        let updated_deposit = deposit.checked_sub(&value).ok_or("overflow in calculating deposit")?;

        // add to user's balance
        <BalanceOf<T>>::mutate(to, |to_balance| {
            *to_balance = updated_to_balance;
        });

        // decrease from locked deposits
        <LockedDeposits<T>>::mutate(listing_hash, |deposit| {
            *deposit = updated_deposit;
        });

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
        let updated_from_balance = sender_balance.checked_sub(&value).ok_or("overflow in calculating balance")?;
        let receiver_balance = Self::balance_of(to.clone());
        let updated_to_balance = receiver_balance.checked_add(&value).ok_or("overflow in calculating balance")?;
        
        // reduce sender's balance
        <BalanceOf<T>>::mutate(from.clone(), |from_balance| {
            *from_balance = updated_from_balance;
        });

        // increase receiver's balance
        <BalanceOf<T>>::mutate(to.clone(), |to_balance| {
            *to_balance = updated_to_balance;
        });

        Self::deposit_event(RawEvent::Transfer(from, to, value));
        Ok(())
    }
}