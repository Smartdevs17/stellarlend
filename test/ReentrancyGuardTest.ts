import { expect } from "chai";
import { ethers } from "hardhat";
import { ReentrancyGuard, LendPool, BorrowPool } from "../typechain-types";

describe("Reentrancy Protection", function () {
    let reentrancyGuard: ReentrancyGuard;
    let lendPool: LendPool;
    let borrowPool: BorrowPool;
    let attacker: any;
    let user: any;
    let asset: any;
    
    before(async function () {
        [user, attacker] = await ethers.getSigners();
        
        const ReentrancyGuardFactory = await ethers.getContractFactory("ReentrancyGuard");
        reentrancyGuard = await ReentrancyGuardFactory.deploy();
        
        const LendPoolFactory = await ethers.getContractFactory("LendPool");
        lendPool = await LendPoolFactory.deploy();
        
        const BorrowPoolFactory = await ethers.getContractFactory("BorrowPool");
        borrowPool = await BorrowPoolFactory.deploy();
        
        // Mock asset
        const MockERC20 = await ethers.getContractFactory("MockERC20");
        asset = await MockERC20.deploy("TestAsset", "TA");
        await asset.deployed();
    });
    
    it("should prevent reentrancy in deposit", async function () {
        // Fund attacker with asset
        await asset.mint(attacker.address, ethers.utils.parseEther("1000"));
        await asset.connect(attacker).approve(lendPool.address, ethers.utils.parseEther("1000"));
        
        // Attempt reentrant deposit
        await expect(
            lendPool.connect(attacker).deposit(
                asset.address,
                ethers.utils.parseEther("100"),
                attacker.address
            )
        ).to.be.revertedWith("ReentrancyGuard: reentrant call");
    });
    
    it("should prevent reentrancy in borrow", async function () {
        // Fund attacker with collateral
        await asset.mint(attacker.address, ethers.utils.parseEther("1000"));
        await asset.connect(attacker).approve(borrowPool.address, ethers.utils.parseEther("1000"));
        
        // Attempt reentrant borrow
        await expect(
            borrowPool.connect(attacker).borrow(
                asset.address, // collateral
                asset.address, // borrowed asset
                ethers.utils.parseEther("100"),
                0, // interest rate
                attacker.address
            )
        ).to.be.revertedWith("ReentrancyGuard: reentrant call");
    });
    
    it("should allow normal operations when not reentrant", async function () {
        // Fund user with asset
        await asset.mint(user.address, ethers.utils.parseEther("1000"));
        await asset.connect(user).approve(lendPool.address, ethers.utils.parseEther("1000"));
        
        // Normal deposit should succeed
        await expect(
            lendPool.connect(user).deposit(
                asset.address,
                ethers.utils.parseEther("100"),
                user.address
            )
        ).to.emit(lendPool, "Deposit");
    });
    
    it("should prevent cross-contract reentrancy", async function () {
        // Create malicious contract that attempts reentrancy
        const MaliciousContract = await ethers.getContractFactory("MaliciousContract");
        const malicious = await MaliciousContract.deploy(
            lendPool.address,
            asset.address,
            ethers.utils.parseEther("100")
        );
        
        // Fund malicious contract
        await asset.mint(malicious.address, ethers.utils.parseEther("1000"));
        await asset.connect(malicious.signer).approve(lendPool.address, ethers.utils.parseEther("1000"));
        
        // Attempt reentrancy attack
        await expect(
            malicious.attack()
        ).to.be.revertedWith("ReentrancyGuard: reentrant call");
    });
});