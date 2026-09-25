
      use soroban_sdk::{contractimport, Address, Env, Symbol};

      contractimport!(file = "admin.rs");
      contractimport!(file = "treasury.rs");

      pub mod timelock;

      #[no_mangle]
      pub fn transfer_admin(
          env: Env,
          caller: Address,
          new_admin: Address,
      ) -> Result<(), LendingError> {
          // Enforce cryptographic authorization before admin validation
          caller.require_auth();
          admin::set_admin(&env, new_admin, Some(caller)).map_err(Into::into)
      }
      