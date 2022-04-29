Oleksii Vynogradov 🇺🇦, [18/04/2022 10:30]
// SPDX-License-Identifier: UNLICENSED
// (c) Oleksii Vynogradov 2021, All rights reserved, contact alex@cfc.io if you like to use code
pragma solidity ^0.8.0;
import "@openzeppelin/contracts/utils/math/SafeMath.sol";
import "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

interface Token {
    function transferFrom(address, address, uint) external returns (bool);
    function transfer(address, uint) external returns (bool);
}

contract StakingOBS is Ownable {
    using SafeMath for uint;
    using EnumerableSet for EnumerableSet.AddressSet;

    event RewardsTransferred(address holder, uint amount);

    // OBS token contract address
    address public tokenAddress;

    uint public stakingAndDaoTokens;

    constructor(address _tokenAddress, uint _stakingAndDaoTokens) public {
        tokenAddress = _tokenAddress;
        stakingAndDaoTokens = _stakingAndDaoTokens;
    }

    // reward rate 18% per year
    uint public constant rewardRate = 1800;
    uint public constant rewardInterval = 365 days;

    // staking fee 0.25%
    uint public constant stakingFeeRate = 25;

    // unstaking fee 0.25%
    uint public constant unstakingFeeRate = 25;

    // unstaking possible after 10 days
    uint public constant cliffTime = 10 days;

    uint public totalClaimedRewards = 0;

    EnumerableSet.AddressSet private holders;

    mapping (address => uint) public depositedTokens;
    mapping (address => uint) public stakingTime;
    mapping (address => uint) public lastClaimedTime;
    mapping (address => uint) public totalEarnedTokens;

    function updateAccount(address account) private {
        uint pendingDivs = getPendingDivs(account);
        if (pendingDivs > 0) {
            require(Token(tokenAddress).transfer(account, pendingDivs), "Could not transfer tokens.");
            totalEarnedTokens[account] = totalEarnedTokens[account].add(pendingDivs);
            totalClaimedRewards = totalClaimedRewards.add(pendingDivs);
            emit RewardsTransferred(account, pendingDivs);
        }
        lastClaimedTime[account] = block.timestamp;
    }

    function getPendingDivs(address _holder) public view returns (uint) {
        if (!holders.contains(_holder)) return 0;
        if (depositedTokens[_holder] == 0) return 0;

        uint timeDiff = block.timestamp.sub(lastClaimedTime[_holder]);
        uint stakedAmount = depositedTokens[_holder];

        uint pendingDivs = stakedAmount
        .mul(rewardRate)
        .mul(timeDiff)
        .div(rewardInterval)
        .div(1e4);

        return pendingDivs;
    }

    function getNumberOfHolders() public view returns (uint) {
        return holders.length();
    }


    function deposit(uint amountToStake) public {
        require(amountToStake > 0, "Cannot deposit 0 Tokens");
        require(Token(tokenAddress).transferFrom(msg.sender, address(this), amountToStake), "Insufficient Token Allowance");

        updateAccount(msg.sender);

        uint fee = amountToStake.mul(stakingFeeRate).div(1e4);
        uint amountAfterFee = amountToStake.sub(fee);
        require(Token(tokenAddress).transfer(owner(), fee), "Could not transfer deposit fee.");

        depositedTokens[msg.sender] = depositedTokens[msg.sender].add(amountAfterFee);

        if (!holders.contains(msg.sender)) {
            holders.add(msg.sender);
            stakingTime[msg.sender] = block.timestamp;
        }
    }

    function withdraw(uint amountToWithdraw) public {
        require(depositedTokens[msg.sender] >= amountToWithdraw, "Invalid amount to withdraw");

        require(block.timestamp.sub(stakingTime[msg.sender]) > cliffTime, "You recently staked, please wait before withdrawing.");

        updateAccount(msg.sender);

        require(Token(tokenAddress).transfer(msg.sender, amountToWithdraw), "Could not transfer tokens.");

        depositedTokens[msg.sender] = depositedTokens[msg.sender].sub(amountToWithdraw);

        if (holders.contains(msg.sender) && depositedTokens[msg.sender] == 0) {
            holders.remove(msg.sender);
        }
    }

    function claimDivs() public {
        updateAccount(msg.sender);
    }

    function getStakingAndDaoAmount() public view returns (uint) {
        if (totalClaimedRewards >= stakingAndDaoTokens) {
            return 0;
        }
        uint remaining = stakingAndDaoTokens.sub(totalClaimedRewards);
        return remaining;
    }

    // function to allow admin to claim *any* ERC20 tokens sent to this contract
    function transferAnyERC20Tokens(address _tokenAddr, address _to, uint _amount) public onlyOwner {
        if (_tokenAddr == tokenAddress) {
            totalClaimedRewards = totalClaimedRewards.add(_amount);
        }
        Token(_tokenAddr).transfer(_to, _amount);
    }


}