/// Runtime module implementing the token transfer functions.
/// With added lock and unlock functions for staking in TCR runtime.
/// Implements a custom type `TokenBalance` for representing account balance.
/// `TokenBalance` type is similar to the `Balance` type in `balances` SRML module.

use rstd::prelude::*;
use rstd::fmt::Debug;
use codec::Codec;
use support::{dispatch::Result, Parameter, decl_storage, decl_module, decl_event, ensure};
use system::{self, ensure_signed};
use sr_primitives::traits::{CheckedSub, CheckedAdd, Member, SimpleArithmetic, MaybeSerializeDeserialize};

// Configuration trait for this module.
pub trait Trait: system::Trait {
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
    type TokenBalance: Parameter + Member + SimpleArithmetic + Codec + Default + Copy +
     MaybeSerializeDeserialize + Debug + From<Self::BlockNumber>;
}

decl_module! {
  pub struct Module<T: Trait> for enum Call where origin: T::Origin {
      // Initialize the default event for this module.
      fn deposit_event() = default;

      // Transfer tokens from one account to another.
      pub fn transfer(origin, to: T::AccountId, #[compact] value: T::TokenBalance) -> Result {
          let sender = ensure_signed(origin)?;
          Self::_transfer(sender, to, value)
      }

      // Approve token transfer from one account to another.
      // Once this is done, then transfer_from can be called with corresponding values.
      pub fn approve(origin, spender: T::AccountId, #[compact] value: T::TokenBalance) -> Result {
          let sender = ensure_signed(origin)?;
          // Make sure the approver/owner owns this token.
          ensure!(<BalanceOf<T>>::exists(&sender), "Account does not own this token");

          // Get the current value of the allowance for this sender and spender combination.
          // If doesnt exist then default 0 will be returned.
          let allowance = Self::allowance((sender.clone(), spender.clone()));
          
          // Add the value to the current allowance.
          // Using checked_add (safe math) to avoid overflow.
          let updated_allowance = allowance.checked_add(&value).ok_or("overflow in calculating allowance")?;

          // Insert the new allownace value of this sender and spender combination.
          <Allowance<T>>::insert((sender.clone(), spender.clone()), updated_allowance);
          
          // Raise the approval event.
          Self::deposit_event(RawEvent::Approval(sender, spender, value));
          Ok(())
      }

      // If approved, transfer from an account to another account without needing owner's signature.
      pub fn transfer_from(_origin, from: T::AccountId, to: T::AccountId, #[compact] value: T::TokenBalance) -> Result {
          ensure!(<Allowance<T>>::exists((from.clone(), to.clone())), "Allowance does not exist.");
          let allowance = Self::allowance((from.clone(), to.clone()));
          ensure!(allowance >= value, "Not enough allowance.");

          // Using checked_sub (safe math) to avoid overflow.
          let updated_allowance = allowance.checked_sub(&value).ok_or("overflow in calculating allowance")?;

          // Insert the new allownace value of this sender and spender combination.
          <Allowance<T>>::insert((from.clone(), to.clone()), updated_allowance);

          Self::deposit_event(RawEvent::Approval(from.clone(), to.clone(), value));
          Self::_transfer(from, to, value)
      }
  }
}

// Storage for this runtime module.
decl_storage! {
  trait Store for Module<T: Trait> as Token {
    // Bool flag to allow init to be called only once.
    Init get(is_init): bool;
    // Total supply of the token.
    // Set in the genesis config.
    // See ../src/chain_spec.rs
    TotalSupply get(total_supply) config(): T::TokenBalance;
    // Mapping of balances to accounts.
    BalanceOf get(balance_of): map T::AccountId => T::TokenBalance;
    // Mapping of allowances to accounts.
    Allowance get(allowance): map (T::AccountId, T::AccountId) => T::TokenBalance;
    // Stores the total deposit for a listing.
    // Maps a listing hash with the total tokensface.
    LockedDeposits get(locked_deposits): map T::Hash => T::TokenBalance;
  }
}

// events
decl_event!(
    pub enum Event<T> where AccountId = <T as system::Trait>::AccountId, TokenBalance = <T as self::Trait>::TokenBalance {
        // Event for transfer of tokens.
        Transfer(AccountId, AccountId, TokenBalance),
        // Event when an approval is made.
        Approval(AccountId, AccountId, TokenBalance),
    }
);

/// All functions in the decl_module macro become part of the public interface of the module.
/// If they are there, they are accessible via extrinsics calls whether they are public or not.
/// However, in the impl module section (this, below) the functions can be public and private.
/// Private functions are internal to this module e.g.: _transfer.
/// Public functions can be called from other modules e.g.: lock and unlock (being called from the tcr module).
/// All functions in the impl module section are not part of public interface because they are not part of the Call enum.
impl<T: Trait> Module<T> {
    // Initialize the token.
    // Transfers the total_supply amout to the caller.
    // The token becomes usable.
    // Similar to the ERC20 smart contract constructor.
    pub fn init(sender: T::AccountId) -> Result {
        ensure!(Self::is_init() == false, "Token already initialized.");

        <BalanceOf<T>>::insert(sender, Self::total_supply());
        <Init>::put(true);

        Ok(())
    }

    // Lock user deposits for curation actions.
    pub fn lock(from: T::AccountId, value: T::TokenBalance, listing_hash: T::Hash) -> Result {
        ensure!(<BalanceOf<T>>::exists(from.clone()), "Account does not own this token");

        let sender_balance = Self::balance_of(from.clone());
        ensure!(sender_balance > value, "Not enough balance.");
        let updated_from_balance = sender_balance.checked_sub(&value).ok_or("overflow in calculating balance")?;
        let deposit = Self::locked_deposits(listing_hash);
        let updated_deposit = deposit.checked_add(&value).ok_or("overflow in calculating deposit")?;

        // Deduct the deposit from balance.
        <BalanceOf<T>>::insert(from, updated_from_balance);
        
        // Add to deposits.
        <LockedDeposits<T>>::insert(listing_hash, updated_deposit);

        Ok(())
    }

    // Unlock user's deposit for reward claims and challenge wins.
    pub fn unlock(to: T::AccountId, value: T::TokenBalance, listing_hash: T::Hash) -> Result {
        let to_balance = Self::balance_of(to.clone());
        let updated_to_balance = to_balance.checked_add(&value).ok_or("overflow in calculating balance")?;
        let deposit = Self::locked_deposits(listing_hash);
        let updated_deposit = deposit.checked_sub(&value).ok_or("overflow in calculating deposit")?;

        // Add to user's balance.
        <BalanceOf<T>>::insert(to, updated_to_balance);

        // Decrease from locked deposits.
        <LockedDeposits<T>>::insert(listing_hash, updated_deposit);

        Ok(())
    }

    // Internal transfer function for ERC20 interface.
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
        
        // Reduce sender's balance.
        <BalanceOf<T>>::insert(from.clone(), updated_from_balance);

        // Increase receiver's balance.
        <BalanceOf<T>>::insert(to.clone(), updated_to_balance);

        Self::deposit_event(RawEvent::Transfer(from, to, value));
        Ok(())
    }
}
