// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;
import "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import "forge-std/console.sol";
import "../staking/MovementStaking.sol";

contract MCR {

    IMovementStaking public stakingContract;

    // the number of blocks that can be submitted ahead of the lastAcceptedBlockHeight
    // this allows for things like batching to take place without some attesters locking down the attester set by pushing too far ahead
    // ? this could be replaced by a 2/3 stake vote on the block height to epoch assignment
    // ? however, this protocol becomes more complex as you to take steps to ensure that...
    // ? 1. Block heights have a non-decreasing mapping to epochs
    // ? 2. Votes get accumulated reasonable near the end of the epoch (i.e., your vote is cast for the epoch you vote fore and the next)
    // ? if howevever, you simply allow a race with the tolerance below, both of these are satisfied without the added complexity
    uint256 public leadingBlockTolerance;

    // track the last accepted block height, so that we can require blocks are submitted in order and handle staking effectively
    uint256 public lastAcceptedBlockHeight;

    struct BlockCommitment {
        // currently, to simplify the api, we'll say 0 is uncommitted all other numbers are legitimate heights
        uint256 height;
        bytes32 commitment;
        bytes32 blockId;
    }

    // map each block height to an epoch
    mapping(uint256 => uint256) public blockHeightEpochAssignments;

    // track each commitment from each attester for each block height
    mapping(uint256 => mapping(address => BlockCommitment)) public commitments;

    // track the total stake accumulate for each commitment for each block height
    mapping(uint256 => mapping(bytes32=> uint256)) public commitmentStakes;

    // map block height to accepted block hash 
    mapping(uint256 => BlockCommitment) public acceptedBlocks;

    event BlockAccepted(bytes32 indexed blockHash, bytes32 stateCommitment, uint256 height);
    event BlockCommitmentSubmitted(bytes32 indexed blockHash, bytes32 stateCommitment, uint256 attesterStake);

    // todo: initializer
    constructor(
        IMovementStaking _stakingContract,
        uint256 _leadingBlockTolerance,
        uint256 _lastAcceptedBlockHeight,
        uint256 _epochDuration,
        address[] memory _custodians
    ) {
        stakingContract = _stakingContract;
        leadingBlockTolerance = _leadingBlockTolerance;
        lastAcceptedBlockHeight = _lastAcceptedBlockHeight;
        stakingContract.registerDomain(
            address(this),
            _epochDuration,
            _custodians
        );
    }

    // creates a commitment 
    function createBlockCommitment(
        uint256 height,
        bytes32 commitment,
        bytes32 blockId
    ) public pure returns (BlockCommitment memory) {
        return BlockCommitment(height, commitment, blockId);
    }

    // gets whether the genesis ceremony has ended
    function hasGenesisCeremonyEnded() public view returns (bool) {
        assert(false);
    }

    // gets the max tolerable block height
    function getMaxTolerableBlockHeight() public view returns (uint256) {
        return lastAcceptedBlockHeight + leadingBlockTolerance;
    }

    // gets the would be epoch for the current block time
    function getEpochByBlockTime() public view returns (uint256) {
        return stakingContract.getEpochByBlockTime(address(this));
    }

    // gets the current epoch up to which blocks have been accepted
    function getCurrentEpoch() public view returns (uint256) {
        return stakingContract.getCurrentEpoch(address(this));
    }

    // gets the next epoch
    function getNextEpoch() public view returns (uint256) {
        return stakingContract.getNextEpoch(address(this));
    }

    // gets the stake for a given attester at a given epoch
    function getStakeAtEpoch(uint256 epoch, address custodian, address attester) public view returns (uint256) {
        return stakingContract.getStakeAtEpoch(
            address(this), 
            epoch, 
            custodian, 
            attester
        );
    }

    // todo: memoize this
    function computeAllStakeAtEpoch(uint256 epoch, address attester) public view returns (uint256) {
        address[] memory custodians = stakingContract.getCustodiansByDomain(address(this));
        uint256 totalStake = 0;
        for (uint256 i = 0; i < custodians.length; i++){
            // for now, each custodian has weight of 1
            totalStake += getStakeAtEpoch(epoch, custodians[i], attester);
        }
        return totalStake;
    }

    // gets the stake for a given attester at the current epoch
    function getCurrentEpochStake(address custodian, address attester) public view returns (uint256) {
        return getStakeAtEpoch(getCurrentEpoch(), custodian, attester);
    }

    function computeAllCurrentEpochStake(address attester) public view returns (uint256) {
        return computeAllStakeAtEpoch(getCurrentEpoch(), attester);
    }

    // gets the total stake for a given epoch
    function getTotalStakeForEpoch(uint256 epoch, address custodian) public view returns (uint256) {
        return stakingContract.getTotalStakeForEpoch(
            address(this), 
            epoch,
            custodian
        );
    }

    function computeAllTotalStakeForEpoch(uint256 epoch) public view returns (uint256) {
        address[] memory custodians = stakingContract.getCustodiansByDomain(address(this));
        uint256 totalStake = 0;
        for (uint256 i = 0; i < custodians.length; i++){
            // for now, each custodian has weight of 1
            totalStake += getTotalStakeForEpoch(epoch, custodians[i]);
        }
        return totalStake;
    }

    // gets the total stake for the current epoch
    function getTotalStakeForCurrentEpoch(address custodian) public view returns (uint256) {
        return getTotalStakeForEpoch(getCurrentEpoch(), custodian);
    }

    function computeAllTotalStakeForCurrentEpoch() public view returns (uint256) {
        return computeAllTotalStakeForEpoch(getCurrentEpoch());
    }

    // gets the commitment at a given block height
    function getAttesterCommitmentAtBlockHeight(uint256 blockHeight, address attester) public view returns (BlockCommitment memory) {
        return commitments[blockHeight][attester];
    }

    // gets the accepted commitment at a given block height
    function getAcceptedCommitmentAtBlockHeight(uint256 blockHeight) public view returns (BlockCommitment memory) {
        return acceptedBlocks[blockHeight];
    }

    function getAttesters() public view returns (address[] memory) {
        return stakingContract.getAttestersByDomain(address(this));
    }

    // commits a attester to a particular block
    function submitBlockCommitmentForAttester(
        address attester, 
        BlockCommitment memory blockCommitment
    ) internal {

        require(commitments[blockCommitment.height][attester].height == 0, "Attester has already committed to a block at this height");

        // note: do no uncomment the below, we want to allow this in case we have lagging attesters
        // require(blockCommitment.height > lastAcceptedBlockHeight, "Attester has committed to an already accepted block");

        require(blockCommitment.height < lastAcceptedBlockHeight + leadingBlockTolerance, "Attester has committed to a block too far ahead of the last accepted block");

        // assign the block height to the current epoch if it hasn't been assigned yet
        if (blockHeightEpochAssignments[blockCommitment.height] == 0) {
            // note: this is an intended race condition, but it is benign because of the tolerance
            blockHeightEpochAssignments[blockCommitment.height] = getEpochByBlockTime();
        }

        // register the attester's commitment
        commitments[blockCommitment.height][attester] = blockCommitment;

        // increment the commitment count by stake
        uint256 allCurrentEpochStake = computeAllCurrentEpochStake(attester);
        commitmentStakes[blockCommitment.height][blockCommitment.commitment] += allCurrentEpochStake;

        emit BlockCommitmentSubmitted(blockCommitment.blockId, blockCommitment.commitment, allCurrentEpochStake);

        // keep ticking through to find accepted blocks
        // note: this is what allows for batching to be successful
        // we can commit to blocks out to the tolerance point
        // then we can accept them in order
        // ! however, this does potentially become very costly for whomever submits this last block
        // ! rewards need to be managed accordingly
        while (tickOnBlockHeight(lastAcceptedBlockHeight + 1)) {}
      
    }

    function tickOnBlockHeight(uint256 blockHeight) internal returns (bool) {

        // get the epoch assigned to the block height
        uint256 blockEpoch = blockHeightEpochAssignments[blockHeight];

        // if the current epoch is far behind, that's okay that just means there weren't blocks submitted
        // so long as we ensure that we go through the blocks in order and that the block to epoch assignment is non-decreasing, we're good
        // so, we'll just keep rolling over the epoch until we catch up
        while (getCurrentEpoch() < blockEpoch) {
            rollOverEpoch(getCurrentEpoch());
        }

        // note: we could keep track of seen commitments in a set
        // but since the operations we're doing are very cheap, the set actually adds overhead

        uint256 supermajority = (2 * computeAllTotalStakeForEpoch(blockEpoch))/3;
        address[] memory attesters = getAttesters();

        // iterate over the attester set
        for (uint256 i = 0; i < attesters.length; i++){

            address attester = attesters[i];

            // get a commitment for the attester at the block height
            BlockCommitment memory blockCommitment = commitments[blockHeight][attester];

            // check the total stake on the commitment
            uint256 totalStakeOnCommitment = commitmentStakes[blockCommitment.height][blockCommitment.commitment];

            if (totalStakeOnCommitment > supermajority) {

                // accept the block commitment (this may trigger a roll over of the epoch)
                acceptBlockCommitment(blockCommitment, blockEpoch);

                // we found a commitment that was accepted
                return true;

            }

        }

        return false;

    }

    function submitBlockCommitment(
        BlockCommitment memory blockCommitment
    ) public {

        submitBlockCommitmentForAttester(msg.sender, blockCommitment);

    }

    function submitBatchBlockCommitment(
        BlockCommitment[] memory blockCommitments
    ) public {

        for (uint256 i = 0; i < blockCommitments.length; i++){
            submitBlockCommitment(blockCommitments[i]);
        }

    }

    function acceptBlockCommitment(
        BlockCommitment memory blockCommitment,
        uint256 epochNumber
    ) internal {
      
        // set accepted block commitment
        acceptedBlocks[blockCommitment.height] = blockCommitment;

        // set last accepted block height
        lastAcceptedBlockHeight = blockCommitment.height;

        // slash minority attesters w.r.t. to the accepted block commitment
        slashMinority(blockCommitment, epochNumber);

        // emit the block accepted event
        emit BlockAccepted(blockCommitment.blockId, blockCommitment.commitment, blockCommitment.height);

        // if the timestamp epoch is greater than the current epoch, roll over the epoch
        if (getEpochByBlockTime() > epochNumber) {
            rollOverEpoch(epochNumber);
        }
       
    }

    function slashMinority(
        BlockCommitment memory blockCommitment,
        uint256 totalStake
    ) internal {

        // stakingContract.slash(custodians, attesters, amounts, refundAmounts);

    }

    function rollOverEpoch(uint256 epochNumber) internal {

        stakingContract.rollOverEpoch();

    }

}