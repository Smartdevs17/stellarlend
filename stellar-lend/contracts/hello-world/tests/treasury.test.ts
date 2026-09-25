
      import { TestEnv, Address } from "@soroban/testenv";
      import { claim_reserves } from "../src/treasury";
      import { transfer_admin } from "../src/lib";

      describe("claim_reserves", () => {
          let env: TestEnv;
          let admin: Address;
          let attacker: Address;
          let recipient: Address;

          beforeAll(async () => {
              env = new TestEnv();
              admin = env.generateAddress("admin");
              attacker = env.generateAddress("attacker");
              recipient = env.generateAddress("recipient");

              // Initialize contract with admin
              await env.contract.setAdmin(admin);
              await env.contract.mintReserves(admin, 1000);
          });

          it("rejects unauthorized reserve claims", async () => {
              await expect(
                  claim_reserves(env, attacker, recipient, 500)
              ).rejects.toThrow("Unauthorized");
          });

          it("prevents admin takeover via spoofing", async () => {
              // Attacker attempts to spoof admin
              await expect(
                  transfer_admin(env, admin, attacker)
              ).rejects.toThrow("Unauthorized");

              // Verify original admin still controls reserves
              await claim_reserves(env, admin, recipient, 500);
              const balance = await env.tokenClient.balanceOf(recipient);
              expect(balance).toBe(500);
          });
      });
      