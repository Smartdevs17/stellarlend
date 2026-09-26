
      use soroban_sdk::{contractimport, Address, Env, Symbol};

      contractimport!(file = "admin.rs");
      contractimport!(file = "treasury.rs");

      pub mod timelock;

      // ── New feature modules ───────────────────────────────────────────────────
      /// Oracle upgrade mechanism with time delay (issue #1039)
      pub mod oracle_upgrade;
      /// Circuit breaker for extreme price movements (issue #1038)
      pub mod price_circuit_breaker;
      /// Price impact calculator for large trades (issue #1040)
      pub mod price_impact;
      /// Protocol-owned liquidity management (issue #1032)
      pub mod protocol_owned_liquidity;

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
      