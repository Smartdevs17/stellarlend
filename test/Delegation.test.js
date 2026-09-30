const { expect } = require("chai");
const { ethers } = require("hardhat");

describe("Delegation Contract", function () {
  let Delegation, delegation, owner, addr1, addr2, addr3;

  beforeEach(async () => {
    [owner, addr1, addr2, addr3] = await ethers.getSigners();
    Delegation = await ethers.getContractFactory("Delegation");
    delegation = await Delegation.deploy();
    await delegation.deployed();

    // Mint initial voting power (simulated via direct storage manipulation in test)
    await delegation.setVotingPower(owner.address, 1000);
    await delegation.setVotingPower(addr1.address, 1000);
    await delegation.setVotingPower(addr2.address, 1000);
  });

  describe("Delegation", function () {
    it("Should allow delegation of voting power", async function () {
      await delegation.connect(addr1).delegate(addr2.address, 500);
      expect(await delegation.getDelegate(addr1.address)).to.equal(addr2.address);
      expect(await delegation.getDelegatedVotingPower(addr1.address)).to.equal(500);
      expect(await delegation.votingPower(addr1.address)).to.equal(500);
      expect(await delegation.votingPower(addr2.address)).to.equal(1500);
    });

    it("Should prevent self-delegation", async function () {
      await expect(
        delegation.connect(addr1).delegate(addr1.address, 500)
      ).to.be.revertedWith("Cannot delegate to self");
    });

    it("Should prevent circular delegation", async function () {
      await delegation.connect(addr1).delegate(addr2.address, 500);
      await expect(
        delegation.connect(addr2).delegate(addr1.address, 500)
      ).to.be.revertedWith("Circular delegation detected");
    });

    it("Should prevent delegation with insufficient voting power", async function () {
      await expect(
        delegation.connect(addr1).delegate(addr2.address, 1500)
      ).to.be.revertedWith("Insufficient voting power");
    });

    it("Should allow undelegation", async function () {
      await delegation.connect(addr1).delegate(addr2.address, 500);
      await delegation.connect(addr1).undelegate();
      expect(await delegation.getDelegate(addr1.address)).to.equal(ethers.constants.AddressZero);
      expect(await delegation.votingPower(addr1.address)).to.equal(1000);
      expect(await delegation.votingPower(addr2.address)).to.equal(1000);
    });

    it("Should revert undelegation when no delegate is set", async function () {
      await expect(
        delegation.connect(addr1).undelegate()
      ).to.be.revertedWith("No delegate set");
    });
  });
});

// Helper function for tests (not part of production contract)
// Added via Hardhat's contract factory for test setup
Object.assign(Delegation.prototype, {
  async setVotingPower(address, amount) {
    const tx = await this.setVotingPowerForTest(address, amount);
    await tx.wait();
  }
});