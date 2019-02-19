/// runtime module implementing the ERC20 token interface
/// with added lock and unlock functions for staking in TCR runtime
/// implements a custom type `TokenBalance` for representing account balance
/// `TokenBalance` type is exactly the same as the `Balance` type in `balances` SRML module

use rstd::prelude::*;
use parity_codec::Codec;
use support::{dispatch::Result, StorageMap, Parameter, StorageValue, decl_storage, decl_module, decl_event, ensure};
use system::{self, ensure_signed};
use runtime_primitives::traits::{CheckedSub, CheckedAdd, Member, SimpleArithmetic, As};

// trait for this module
// contains type definitions
pub trait Trait: system::Trait {
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
    type TokenBalance: Parameter + Member + SimpleArithmetic + Codec + Default + Copy + As<usize> + As<u64>;
}

// public interface for this runtime module
decl_module! {
  pub struct Module<T: Trait> for enum Call where origin: T::Origin {
      // initialize the default event for this module
      fn deposit_event<T>() = default;

      // transfer tokens from one account to another
      fn transfer(origin, to: T::AccountId, value: T::TokenBalance) -> Result {
          let sender = ensure_signed(origin)?;
          Self::_transfer(sender, to, value)
      }

      // approve token transfer from one account to another
      // once this is done, then transfer_from can be called with corresponding values
      fn approve(origin, spender: T::AccountId, value: T::TokenBalance) -> Result {
          let sender = ensure_signed(origin)?;
          // make sure the approver/owner owns this token
          ensure!(<BalanceOf<T>>::exists(&sender), "Account does not own this token");

          // get the current value of the allowance for this sender and spender combination
          // if doesnt exist then default 0 will be returned
          let allowance = Self::allowance((sender.clone(), spender.clone()));
          
          // add the value to the current allowance
          // using checked_add (safe math) to avoid overflow
          let updated_allowance = allowance.checked_add(&value).ok_or("overflow in calculating allowance")?;

          // insert the new allownace value of this sender and spender combination
          <Allowance<T>>::insert((sender.clone(), spender.clone()), updated_allowance);
          
          // raise the approval event
          Self::deposit_event(RawEvent::Approval(sender, spender, value));
          Ok(())
      }

      // if approved, transfer from an account to another account without needing owner's signature
      fn transfer_from(_origin, from: T::AccountId, to: T::AccountId, value: T::TokenBalance) -> Result {
          ensure!(<Allowance<T>>::exists((from.clone(), to.clone())), "Allowance does not exist.");
          let allowance = Self::allowance((from.clone(), to.clone()));
          ensure!(allowance >= value, "Not enough allowance.");

          // using checked_sub (safe math) to avoid overflow
          let updated_allowance = allowance.checked_sub(&value).ok_or("overflow in calculating allowance")?;
          // insert the new allownace value of this sender and spender combination
          <Allowance<T>>::insert((from.clone(), to.clone()), updated_allowance);

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
    TotalSupply get(total_supply) config(): T::TokenBalance;
    // mapping of balances to accounts
    BalanceOf get(balance_of): map T::AccountId => T::TokenBalance;
    // mapping of allowances to accounts
    Allowance get(allowance): map (T::AccountId, T::AccountId) => T::TokenBalance;
    // stores the total deposit for a listing
    // maps a listing hash with the total tokensface
    // TCR specific; not part of standard ERC20 interface
    LockedDeposits get(locked_deposits): map T::Hash => T::TokenBalance;
  }
}

// events
decl_event!(
    pub enum Event<T> where AccountId = <T as system::Trait>::AccountId, TokenBalance = <T as self::Trait>::TokenBalance {
        // event for transfer of tokens
        // from, to, value
        Transfer(AccountId, AccountId, TokenBalance),
        // event when an approval is made
        // owner, spender, value
        Approval(AccountId, AccountId, TokenBalance),
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
    // similar to the ERC20 smart contract constructor
    pub fn init(sender: T::AccountId) -> Result {
        ensure!(Self::is_init() == false, "Token already initialized.");

        <BalanceOf<T>>::insert(sender, Self::total_supply());
        <Init<T>>::put(true);

        Ok(())
    }

    // lock user deposits for curation actions
    // TCR specific; not part of standard ERC20 interface
    pub fn lock(from: T::AccountId, value: T::TokenBalance, listing_hash: T::Hash) -> Result {
        ensure!(<BalanceOf<T>>::exists(from.clone()), "Account does not own this token");

        let sender_balance = Self::balance_of(from.clone());
        ensure!(sender_balance > value, "Not enough balance.");
        let updated_from_balance = sender_balance.checked_sub(&value).ok_or("overflow in calculating balance")?;
        let deposit = Self::locked_deposits(listing_hash);
        let updated_deposit = deposit.checked_add(&value).ok_or("overflow in calculating deposit")?;

        // deduct the deposit from balance
        <BalanceOf<T>>::insert(from, updated_from_balance);
        
        // add to deposits
        <LockedDeposits<T>>::insert(listing_hash, updated_deposit);

        Ok(())
    }

    // unlock user's deposit for reward claims and challenge wins
    // TCR specific; not part of standard ERC20 interface
    pub fn unlock(to: T::AccountId, value: T::TokenBalance, listing_hash: T::Hash) -> Result {
        let to_balance = Self::balance_of(to.clone());
        let updated_to_balance = to_balance.checked_add(&value).ok_or("overflow in calculating balance")?;
        let deposit = Self::locked_deposits(listing_hash);
        let updated_deposit = deposit.checked_sub(&value).ok_or("overflow in calculating deposit")?;

        // add to user's balance
        <BalanceOf<T>>::insert(to, updated_to_balance);

        // decrease from locked deposits
        <LockedDeposits<T>>::insert(listing_hash, updated_deposit);

        Ok(())
    }

    // internal transfer function for ERC20 interface
    fn _transfer(
        from: T::AccountId,
        to: T::AccountId,
        value: T::TokenBalance,
    ) -> Result {
        ensure!(<BalanceOf<T>>::exists(from.clone()), "Account does not own this token");
        let sender_balance = Self::balance_of(from.clone());
        ensure!(sender_balance >= value, "Not enough balance.");
        let updated_from_balance = sender_balance.checked_sub(&value).ok_or("overflow in calculating balance")?;
        let receiver_balance = Self::balance_of(to.clone());
        let updated_to_balance = receiver_balance.checked_add(&value).ok_or("overflow in calculating balance")?;
        
        // reduce sender's balance
        <BalanceOf<T>>::insert(from.clone(), updated_from_balance);

        // increase receiver's balance
        <BalanceOf<T>>::insert(to.clone(), updated_to_balance);

        Self::deposit_event(RawEvent::Transfer(from, to, value));
        Ok(())
    }
}
