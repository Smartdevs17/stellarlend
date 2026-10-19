const hre = require("hardhat");

async function main() {
  const Delegation = await hre.ethers.getContractFactory("Delegation");
  const delegation = await Delegation.deploy();
  await delegation.deployed();
  console.log("Delegation deployed to:", delegation.address);
}

main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error(error);
    process.exit(1);
  });