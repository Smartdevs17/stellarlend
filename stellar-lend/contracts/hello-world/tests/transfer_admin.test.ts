
      import { TestEnv, Ledger, Address } from "@soroban/testenv";
      import { transfer_admin } from "../src/lib";

      describe("transfer_admin", () => {
          let env: TestEnv;
          let admin: Address;
          let attacker: Address;
          let valid_new_admin: Address;

          beforeAll(async () => {
              env = new TestEnv();
              admin = env.generateAddress("admin");
              attacker = env.generateAddress("attacker");
              valid_new_admin = env.generateAddress("new_admin");

              // Initialize contract with admin
              await env.contract.setAdmin(admin);
          });

          it("rejects unauthorized callers", async () => {
              await expect(
                  transfer_admin(env, attacker, valid_new_admin)
              ).rejects.toThrow("Unauthorized");
          });

          it("rejects spoofed admin addresses", async () => {
              // Attacker copies admin address but lacks auth
              await expect(
                  transfer_admin(env, admin, valid_new_admin)
              ).rejects.toThrow("Unauthorized");
          });

          it("allows valid admin to transfer", async () => {
              await transfer_admin(env, admin, valid_new_admin);
              const newAdmin = await env.contract.getAdmin();
              expect(newAdmin).toEqual(valid_new_admin);
          });
      });
      